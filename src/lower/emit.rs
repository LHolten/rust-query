use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::{self, Display, Write},
    rc::Rc,
};

use crate::lower::{
    Expr, Join, JoinableTable, RowLike, RowsFrozen, Unique,
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
        self.write("$").write(pos + 1);
    }
}

pub struct Select {
    /// These are the tables that are grouped by
    /// These are joined distinct to not influence the aggregate
    forwarded: BTreeSet<Join>,
    // unique and aggregate are similar, they don't change number of rows
    aggregate: BTreeMap<RowsFrozen, Select>,
    unique: BTreeSet<Unique>,
    // The results returned from the aggregate or select
    select: BTreeSet<Rc<Expr>>,
}

// All the information required to write sql
struct SelectFrozen {
    rows: RowsFrozen,
    forwarded: Vec<Join>,
    aggregate: Vec<SelectFrozen>,
    unique: Vec<Unique>,
    select: Vec<Rc<Expr>>,
}

impl Select {
    /// rows provided should be the same as those that self was created with.
    pub fn frozen(self, rows: RowsFrozen) -> SelectFrozen {
        SelectFrozen {
            rows,
            forwarded: self.forwarded.into_iter().collect(),
            aggregate: self
                .aggregate
                .into_iter()
                .map(|(k, v)| v.frozen(k))
                .collect(),
            unique: self.unique.into_iter().collect(),
            select: self.select.into_iter().collect(),
        }
    }
}

impl SelectFrozen {
    pub fn emit(&self, w: &mut Stmt, is_aggregate: bool) -> fmt::Result {
        w.write("SELECT ");
        let mut list = ListWriter::new(w, ", ");
        for (forward_idx, _item) in self.forwarded.iter().enumerate() {
            list.item().write(format_args!("f{forward_idx}.id"));
        }
        for (select_idx, expr) in self.select.iter().enumerate() {
            let list_item = list.item();
            self.emit_expr(list_item, expr);
            list_item.write(format_args!(" AS s{select_idx}"));
        }
        if is_aggregate && self.forwarded.is_empty() {
            // force aggregation even without group by
            list.item().write("count(*)");
        }
        list.default("1");

        if !self.rows.from.is_empty() || !self.unique.is_empty() || !self.aggregate.is_empty() {
            w.write(" FROM ");
            let mut list = ListWriter::new(w, ", ");
            for (join_idx, join) in self.rows.from.iter().enumerate() {
                let list_item = list.item();
                join.0.emit(list_item);
                list_item.write(format_args!(" AS j{join_idx}"));
            }
            for (forwarded_idx, forwarded) in self.forwarded.iter().enumerate() {
                let list_item = list.item();
                forwarded.0.emit(list_item);
                list_item.write(format_args!(" AS f{forwarded_idx}"));
            }
            list.default("(SELECT 1)");

            for (unique_idx, unique) in self.unique.iter().enumerate() {
                w.write(" JOIN ");
                unique.table.emit(w);
                w.write(format_args!(" AS u{unique_idx}"));
                if !unique.conds.is_empty() {
                    w.write(" ON ");
                    let mut list = ListWriter::new(w, " AND ");
                    for (col, expr) in &unique.conds {
                        let list_item = list.item();
                        list_item.write(format_args!("u{unique_idx}.{} = ", Alias(col)));
                        self.emit_expr(list_item, expr);
                    }
                }
            }

            for (aggr_idx, aggr) in self.aggregate.iter().enumerate() {
                w.write(" JOIN (");
                aggr.emit(w, true);
                w.write(format_args!(") AS a{aggr_idx}"));
                if !aggr.forwarded.is_empty() {
                    w.write(" ON ");
                    let mut list = ListWriter::new(w, " AND ");
                    for (forward_idx, join) in aggr.forwarded.iter().enumerate() {
                        let list_item = list.item();
                        list_item.write(format_args!("a{aggr_idx}.f{forward_idx} = "));
                        self.emit_join(list_item, join);
                    }
                }
            }
        }

        if !self.rows.filter.is_empty() {
            w.write(" WHERE ");
            let mut list = ListWriter::new(w, " AND ");
            for expr in &self.rows.filter {
                self.emit_expr(list.item(), expr);
            }
        }

        if !self.forwarded.is_empty() {
            w.write(" GROUP BY ");
            let mut list = ListWriter::new(w, ", ");
            for (forward_idx, _) in self.forwarded.iter().enumerate() {
                list.item().write(forward_idx + 1);
            }
        }

        Ok(())
    }

    fn emit_join(&self, w: &mut Stmt, join: &Join) {
        if let Ok(idx) = self.rows.from.binary_search(join) {
            w.write(format_args!("j{idx}"));
        } else {
            let idx = self.forwarded.binary_search(join).unwrap();
            w.write(format_args!("f{idx}"));
        }
    }

    fn emit_expr(&self, w: &mut Stmt, expr: &Expr) {
        match expr {
            Expr::Constant(c) => {
                w.write(c);
            }
            Expr::Parameter(param) => w.write_param(param),
            Expr::AggrIndex(select_vec, expr) => {
                let aggr_idx = self
                    .aggregate
                    .binary_search_by_key(&select_vec, |v| &v.rows)
                    .unwrap();
                let alias = self.aggregate[aggr_idx].get_select_alias(expr);
                w.write(format_args!("a{aggr_idx}.{alias}"));
            }
            Expr::RowIndex(row_like, col) => match row_like {
                RowLike::Join(join) => {
                    self.emit_join(w, join);
                    w.write(format_args!(".{}", Alias(col)));
                }
                RowLike::Unique(unique) => {
                    let unique_idx = self.unique.binary_search(unique).unwrap();
                    w.write(format_args!("u{unique_idx}.{}", Alias(col)));
                }
            },
            Expr::Prefix(prefix, expr) => {
                w.write(prefix);
                self.emit_expr(w, expr)
            }
            Expr::Infix(lhs, infix, rhs) => {
                w.write("(");
                self.emit_expr(w, lhs);
                w.write(format_args!(" {infix} "));
                self.emit_expr(w, rhs);
                w.write(")");
            }
            Expr::Func(func, exprs) => {
                w.write(format_args!("{func}("));
                let mut list = ListWriter::new(w, ", ");
                for expr in exprs.as_ref() {
                    self.emit_expr(list.item(), expr);
                }
                w.write(")");
            }
            Expr::In(expr, list) => {
                self.emit_expr(w, expr);
                w.write(" IN (");
                let mut list_writer = ListWriter::new(w, ", ");
                for expr in list {
                    self.emit_expr(list_writer.item(), expr);
                }
                w.write(")");
            }
            Expr::Cast(expr, ty) => {
                w.write("CAST(");
                self.emit_expr(w, expr);
                w.write(format_args!(" AS {ty})"));
            }
            Expr::Between(x, lower, upper) => {
                w.write("(");
                self.emit_expr(w, x);
                w.write(format_args!(" BETWEEN "));
                self.emit_expr(w, lower);
                w.write(format_args!(" AND "));
                self.emit_expr(w, upper);
                w.write(")");
            }
        }
    }

    pub fn get_select_alias(&self, expr: &Rc<Expr>) -> String {
        let idx = self.select.binary_search(expr).unwrap();
        format!("s{idx}")
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

impl Select {
    /// create info associated with rows.
    pub fn new(rows: &RowsFrozen) -> Self {
        let mut out = Self {
            forwarded: BTreeSet::new(),
            aggregate: BTreeMap::new(),
            unique: BTreeSet::new(),
            select: BTreeSet::new(),
        };
        for expr in &rows.filter {
            out.analyze(rows, expr);
        }
        out
    }

    /// rows provided should be the same as those that self was created with.
    pub fn add_select(&mut self, rows: &RowsFrozen, expr: &Rc<Expr>) {
        if self.select.insert(expr.clone()) {
            self.analyze(rows, expr);
        }
    }

    /// rows provided should be the same as those that self was created with.
    fn analyze(&mut self, rows: &RowsFrozen, expr: &Expr) {
        match expr {
            Expr::Constant(_const) => {}
            Expr::Parameter(_ord_rc) => {}
            Expr::AggrIndex(aggr_rows, expr) => {
                self.aggregate
                    .entry(aggr_rows.clone())
                    .or_insert_with(|| Self::new(aggr_rows))
                    .add_select(aggr_rows, expr);
            }
            Expr::RowIndex(row_like, _col) => match row_like {
                RowLike::Join(join) => {
                    if !rows.from.contains(join) {
                        self.forwarded.insert(join.clone());
                    }
                }
                RowLike::Unique(unique) => {
                    self.unique.insert(unique.as_ref().clone());
                }
            },
            Expr::Prefix(_prefix, expr) => self.analyze(rows, expr),
            Expr::Infix(lhs, _infix, rhs) => {
                self.analyze(rows, lhs);
                self.analyze(rows, rhs);
            }
            Expr::Func(_func, exprs) => {
                for expr in exprs.as_ref() {
                    self.analyze(rows, expr);
                }
            }
            Expr::In(expr, list) => {
                self.analyze(rows, expr);
                for expr in list {
                    self.analyze(rows, expr);
                }
            }
            Expr::Cast(expr, _ty) => {
                self.analyze(rows, expr);
            }
            Expr::Between(x, lower, upper) => {
                self.analyze(rows, x);
                self.analyze(rows, lower);
                self.analyze(rows, upper);
            }
        }
    }
}
