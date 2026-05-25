use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::{Display, Write},
    mem::take,
    rc::Rc,
};

use crate::lower::{
    Expr, Join, JoinableTable, RowLike, Rows, Unique,
    list_writer::{Alias, ListWriter},
    ord_rc::OrdRc,
};

#[derive(Default)]
pub struct Stmt {
    pub sql: String,
    pub params: Vec<OrdRc<rusqlite::types::Value>>,
}

impl Stmt {
    pub fn write(&mut self, args: impl Display) -> &mut Self {
        self.sql.write_fmt(format_args!("{args}")).unwrap();
        self
    }

    pub fn write_param(&mut self, param: &OrdRc<rusqlite::types::Value>) {
        let position = self.params.iter().position(|x| x == param);
        let pos = position.unwrap_or_else(|| {
            let pos = self.params.len();
            self.params.push(param.clone());
            pos
        });
        self.write("?").write(pos + 1);
    }

    pub fn fresh(&mut self, f: impl FnOnce(&mut Self)) -> String {
        let mut new = Self {
            sql: String::new(),
            params: take(&mut self.params),
        };
        f(&mut new);
        self.params = new.params;
        new.sql
    }
}

// Map that keeps track of when item was inserted
pub struct IndexMap<K, V> {
    inner: BTreeMap<K, (usize, V)>,
}

impl<K: Ord, V> IndexMap<K, V> {
    pub fn insert_with(&mut self, k: K, f: impl FnOnce(usize) -> V) -> (usize, &mut V) {
        let idx = self.inner.len();
        let (idx, val) = self.inner.entry(k).or_insert_with(|| (idx, f(idx)));
        (*idx, val)
    }

    fn iter(&self) -> impl Iterator<Item = (usize, &K, &V)> {
        let mut vals: Vec<_> = self.inner.iter().map(|(k, v)| (v.0, k, &v.1)).collect();
        vals.sort_by_key(|a| a.0);
        vals.into_iter()
    }

    fn values(&self) -> impl Iterator<Item = (usize, &V)> {
        self.iter().map(|(i, _k, v)| (i, v))
    }

    fn keys(&self) -> impl Iterator<Item = (usize, &K)> {
        self.iter().map(|(i, k, _v)| (i, k))
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl<K, V> Default for IndexMap<K, V> {
    fn default() -> Self {
        Self {
            inner: Default::default(),
        }
    }
}

#[derive(Default)]
struct ExprEmitDeps {
    // Outer scope tables required by the expression
    forwarded: IndexMap<Join, ()>,
    // aggregates used by the expression
    aggregate: IndexMap<Rc<Rows>, IndexMap<Rc<Expr>, ()>>,
    // unique tables used by the expression
    unique: IndexMap<Rc<Unique>, String>,
}

impl Rows {
    #[must_use]
    pub fn emit(
        &self,
        w: &mut Stmt,
        is_aggregate: bool,
        select: &IndexMap<Rc<Expr>, ()>,
    ) -> IndexMap<Join, ()> {
        let mut deps = ExprEmitDeps::default();

        let select_exprs: Vec<_> = select
            .keys()
            .map(|(idx, expr)| {
                (
                    idx,
                    w.fresh(|w| {
                        self.emit_expr(w, expr, &mut deps, Parens::No);
                    }),
                )
            })
            .collect();

        let filter_exprs: BTreeSet<_> = self
            .filter
            .iter()
            .map(|expr| {
                w.fresh(|w| {
                    self.emit_expr(w, expr, &mut deps, Parens::Yes);
                })
            })
            .collect();

        // Build the aggregates as the final step so that they are maximally combined.
        // This should only add some additional forwarded joins.
        let aggregates: Vec<_> = take(&mut deps.aggregate)
            .iter()
            .map(|(aggr_idx, aggr, exprs)| {
                w.fresh(|w| {
                    w.write("(");
                    let aggr_forwarded = aggr.emit(w, true, exprs);
                    w.write(format_args!(") AS a{aggr_idx}"));
                    if !aggr_forwarded.is_empty() {
                        w.write(" ON ");
                        let mut list = ListWriter::new(w, " AND ");
                        for (forward_idx, join) in aggr_forwarded.keys() {
                            let list_item = list.item();
                            list_item.write(format_args!("a{aggr_idx}.ff{forward_idx} = "));
                            self.emit_join(list_item, join, &mut deps);
                            list_item.write(".id"); // TODO use real primary key
                        }
                    }
                })
            })
            .collect();

        // make deps immutable from this moment
        let deps = deps;
        // no additional aggregates should exist
        assert!(deps.aggregate.is_empty());

        w.write("SELECT ");
        let mut list = ListWriter::new(w, ", ");
        for (forward_idx, _item) in deps.forwarded.keys() {
            list.item()
                .write(format_args!("f{forward_idx}.id AS ff{forward_idx}"));
        }
        for (select_idx, expr) in select_exprs.iter() {
            list.item().write(format_args!("{expr} AS s{select_idx}"));
        }
        if is_aggregate && deps.forwarded.is_empty() {
            // force aggregation even without group by
            list.item().write("count(*)");
        }
        list.default("1");

        w.write(" FROM ");
        let mut list = ListWriter::new(w, ", ");
        for (join_idx, join) in self.from.iter().enumerate() {
            let list_item = list.item();
            join.0.emit(list_item);
            list_item.write(format_args!(" AS j{join_idx}"));
        }
        for (forwarded_idx, forwarded) in deps.forwarded.keys() {
            let list_item = list.item();
            forwarded.0.emit(list_item);
            list_item.write(format_args!(" AS f{forwarded_idx}"));
        }
        list.default("(SELECT 1)");

        // aggregates can only depend on forwarded tables so we can emit them first
        for aggr in aggregates {
            w.write(" LEFT JOIN ").write(aggr);
        }

        // uniques can depends on aggregates, so these are emitted after the aggregates
        for (_unique_idx, unique) in deps.unique.values() {
            w.write(" LEFT JOIN ").write(unique);
        }

        if !filter_exprs.is_empty() {
            w.write(" WHERE ");
            let mut list = ListWriter::new(w, " AND ");
            for expr in &filter_exprs {
                list.item().write(expr);
            }
        }

        if !deps.forwarded.is_empty() {
            w.write(" GROUP BY ");
            let mut list = ListWriter::new(w, ", ");
            for (forward_idx, _) in deps.forwarded.values().enumerate() {
                list.item().write(forward_idx + 1);
            }
        }

        deps.forwarded
    }

    fn emit_join(&self, w: &mut Stmt, join: &Join, deps: &mut ExprEmitDeps) {
        if let Some((idx, _)) = self.from.iter().enumerate().find(|x| x.1 == join) {
            w.write(format_args!("j{idx}"));
        } else {
            let (idx, ()) = deps.forwarded.insert_with(join.clone(), |_| ());
            w.write(format_args!("f{idx}"));
        }
    }

    fn emit_expr(&self, w: &mut Stmt, expr: &Expr, deps: &mut ExprEmitDeps, parens: Parens) {
        match expr {
            Expr::Constant(c) => {
                w.write(c);
            }
            Expr::Parameter(param) => w.write_param(param),
            Expr::AggrIndex(rows, expr) => {
                let (aggr_idx, cols) = deps
                    .aggregate
                    .insert_with(rows.clone(), |_| IndexMap::default());
                let (col_idx, ()) = cols.insert_with(expr.clone(), |_| ());
                w.write(format_args!("a{aggr_idx}.s{col_idx}"));
            }
            Expr::RowIndex(row_like, col) => match row_like {
                RowLike::Join(join) => {
                    self.emit_join(w, join, deps);
                    w.write(format_args!(".{}", Alias(col)));
                }
                RowLike::Unique(unique) => {
                    let conds: Vec<_> = unique
                        .conds
                        .iter()
                        .map(|(col, expr)| {
                            (
                                *col,
                                w.fresh(|w| {
                                    self.emit_expr(w, expr, deps, Parens::Yes);
                                }),
                            )
                        })
                        .collect();

                    let (unique_idx, _sql) =
                        deps.unique.insert_with(unique.clone(), |unique_idx| {
                            w.fresh(|w| {
                                unique.table.emit(w);
                                // there are no unique constraints without columns, so there should always
                                // be a condition.
                                assert!(!conds.is_empty());

                                w.write(format_args!(" AS u{unique_idx}"));
                                w.write(" ON ");
                                let mut list = ListWriter::new(w, " AND ");
                                for (col, expr) in &conds {
                                    let list_item = list.item();
                                    list_item.write(format_args!(
                                        "u{unique_idx}.{} = {expr}",
                                        Alias(col)
                                    ));
                                }
                            })
                        });

                    w.write(format_args!("u{unique_idx}.{}", Alias(col)));
                }
            },
            Expr::Prefix(prefix, expr) => {
                parens.with(w, |w| {
                    w.write(prefix);
                    self.emit_expr(w, expr, deps, Parens::Yes);
                });
            }
            Expr::Infix(lhs, infix, rhs) => {
                parens.with(w, |w| {
                    self.emit_expr(w, lhs, deps, Parens::Yes);
                    w.write(format_args!(" {infix} "));
                    self.emit_expr(w, rhs, deps, Parens::Yes);
                });
            }
            Expr::Func(func, exprs) => {
                w.write(format_args!("{func}("));
                let mut list = ListWriter::new(w, ", ");
                for expr in exprs.as_ref() {
                    self.emit_expr(list.item(), expr, deps, Parens::No);
                }
                w.write(")");
            }
            Expr::Cast(expr, ty) => {
                w.write("CAST(");
                self.emit_expr(w, expr, deps, Parens::No);
                w.write(format_args!(" AS {ty})"));
            }
            Expr::Between(x, lower, upper) => {
                parens.with(w, |w| {
                    self.emit_expr(w, x, deps, Parens::Yes);
                    w.write(format_args!(" BETWEEN "));
                    self.emit_expr(w, lower, deps, Parens::Yes);
                    w.write(format_args!(" AND "));
                    self.emit_expr(w, upper, deps, Parens::Yes);
                });
            }
        }
    }
}

enum Parens {
    Yes,
    No, // Should only be used in rare cases where expr can never have wrong associativity.
}

impl Parens {
    fn with(self, w: &mut Stmt, f: impl FnOnce(&mut Stmt)) {
        match self {
            Parens::Yes => {
                w.write("(");
                f(w);
                w.write(")");
            }
            Parens::No => f(w),
        }
    }
}

impl JoinableTable {
    pub fn emit(&self, w: &mut Stmt) {
        match self {
            JoinableTable::Table(name) => {
                w.write(format_args!("main.{}", Alias(name)));
            }
            JoinableTable::Tmp(tmp) => {
                w.write(format_args!("main.{}", tmp));
            }
            JoinableTable::Pragma(func, params) => {
                w.write(format_args!("{func}("));
                let mut list = ListWriter::new(w, ", ");
                for param in params {
                    list.item().write_param(param);
                }
                w.write(")");
            }
        }
    }
}
