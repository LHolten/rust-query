use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::{self, Write},
    rc::{self, Rc},
};

use crate::lower::{
    Expr, Join, JoinableTable, RowLike, SelectVec, Unique, list_writer::ListWriter,
};

struct SelectInfo {
    /// These are the tables that are grouped by
    /// These are joined distinct to not influence the aggregate
    forwarded: BTreeSet<Join>,
    // unique and aggregate are similar, they don't change number of rows
    aggregate: BTreeMap<SelectVec, SelectInfo>,
    unique: BTreeSet<Unique>,
    // The results returned from the aggregate or select
    select: BTreeSet<Rc<Expr>>,
}

// All the information required to write sql
struct SelectInfoVecs {
    rows: SelectVec,
    forwarded: Vec<Join>,
    aggregate: Vec<SelectInfoVecs>,
    unique: Vec<Unique>,
    select: Vec<Rc<Expr>>,
}

impl SelectInfo {
    pub fn into_vecs(self, from: SelectVec) -> SelectInfoVecs {
        SelectInfoVecs {
            rows: from,
            forwarded: self.forwarded.into_iter().collect(),
            aggregate: self
                .aggregate
                .into_iter()
                .map(|(k, v)| v.into_vecs(k))
                .collect(),
            unique: self.unique.into_iter().collect(),
            select: self.select.into_iter().collect(),
        }
    }
}

impl SelectInfoVecs {
    pub fn emit(&self, w: &mut dyn Write, is_aggregate: bool) -> fmt::Result {
        write!(w, "SELECT ")?;
        let mut list = ListWriter::new(w, ", ");
        for (forward_idx, _item) in self.forwarded.iter().enumerate() {
            write!(list.item()?, "f{forward_idx}.id")?;
        }
        for (select_idx, expr) in self.select.iter().enumerate() {
            let list_item = list.item()?;
            self.emit_expr(list_item, expr)?;
            write!(list_item, " AS s{select_idx}")?;
        }
        if is_aggregate && self.forwarded.is_empty() {
            // force aggregation even without group by
            write!(list.item()?, "count(*)")?;
        }
        list.default("1")?;

        if !self.rows.from.is_empty() || !self.unique.is_empty() || !self.aggregate.is_empty() {
            write!(w, " FROM ")?;
            let mut list = ListWriter::new(w, ", ");
            for (join_idx, join) in self.rows.from.iter().enumerate() {
                let list_item = list.item()?;
                self.emit_joinable(list_item, &join.0)?;
                write!(list_item, " AS j{join_idx}")?;
            }
            list.default("(SELECT 1)")?;

            for (unique_idx, unique) in self.unique.iter().enumerate() {
                write!(w, " JOIN ")?;
                self.emit_joinable(w, &unique.table)?;
                write!(w, " AS u{unique_idx}")?;
                if !unique.conds.is_empty() {
                    write!(w, " ON ")?;
                    let mut list = ListWriter::new(w, " AND ");
                    for (col, expr) in &unique.conds {
                        let list_item = list.item()?;
                        write!(list_item, "u{unique_idx}.{col} = ")?;
                        self.emit_expr(list_item, expr)?;
                    }
                }
            }

            for (aggr_idx, aggr) in self.aggregate.iter().enumerate() {
                write!(w, " JOIN (")?;
                aggr.emit(w, true)?;
                write!(w, ") AS a{aggr_idx}")?;
                if !aggr.forwarded.is_empty() {
                    write!(w, " ON ")?;
                    let mut list = ListWriter::new(w, " AND ");
                    for (forward_idx, join) in aggr.forwarded.iter().enumerate() {
                        let list_item = list.item()?;
                        write!(list_item, "a{aggr_idx}.f{forward_idx} = ")?;
                        self.emit_join(list_item, join)?;
                    }
                }
            }
        }

        if !self.rows.filter.is_empty() {
            write!(w, " WHERE ")?;
            let mut list = ListWriter::new(w, " AND ");
            for expr in &self.rows.filter {
                self.emit_expr(list.item()?, expr)?;
            }
        }

        if !self.forwarded.is_empty() {
            write!(w, " GROUP BY ")?;
            let mut list = ListWriter::new(w, ", ");
            for (forward_idx, _) in self.forwarded.iter().enumerate() {
                write!(list.item()?, "{}", forward_idx + 1)?;
            }
        }

        Ok(())
    }

    pub fn emit_joinable(&self, w: &mut dyn Write, joinable: &JoinableTable) -> fmt::Result {
        match joinable {
            JoinableTable::Table(name) => write!(w, "main.{name}"),
            JoinableTable::Pragma(_, items) => todo!(),
        }
    }

    pub fn emit_join(&self, w: &mut dyn Write, join: &Join) -> fmt::Result {
        if let Ok(idx) = self.rows.from.binary_search(join) {
            write!(w, "j{idx}")
        } else {
            let idx = self.forwarded.binary_search(join).unwrap();
            write!(w, "f{idx}")
        }
    }

    pub fn emit_expr(&self, w: &mut dyn Write, expr: &Expr) -> fmt::Result {
        match expr {
            Expr::Constant(c) => write!(w, "{c}"),
            Expr::Parameter(ord_rc) => todo!(),
            Expr::AggrIndex(select_vec, expr) => {
                let aggr_idx = self
                    .aggregate
                    .binary_search_by_key(&select_vec.as_ref(), |v| &v.rows)
                    .unwrap();
                let select_idx = self.aggregate[aggr_idx]
                    .select
                    .binary_search(&expr)
                    .unwrap();
                write!(w, "a{aggr_idx}.s{select_idx}")
            }
            Expr::RowIndex(row_like, col) => match row_like.as_ref() {
                RowLike::Join(join) => {
                    self.emit_join(w, join)?;
                    write!(w, ".{col}")
                }
                RowLike::Unique(unique) => {
                    let unique_idx = self.unique.binary_search(unique).unwrap();
                    write!(w, "u{unique_idx}.{col}")
                }
            },
            Expr::Prefix(prefix, expr) => {
                write!(w, "{prefix}")?;
                self.emit_expr(w, expr)
            }
            Expr::Infix(lhs, infix, rhs) => {
                write!(w, "(")?;
                self.emit_expr(w, lhs)?;
                write!(w, "{infix}")?;
                self.emit_expr(w, rhs)?;
                write!(w, ")")
            }
            Expr::Func(func, exprs) => {
                write!(w, "{func}(")?;
                let mut list = ListWriter::new(w, ", ");
                for expr in exprs.as_ref() {
                    self.emit_expr(list.item()?, expr)?;
                }
                write!(w, ")")
            }
        }
    }
}

// impl SelectInfo {

//     pub fn new(select: Select) -> Self {
//         let mut out = SelectInfo {
//             from: select
//                 .from
//                 .into_iter()
//                 .enumerate()
//                 .map(|(idx, val)| (val, TableAlias::Join(idx)))
//                 .collect(),
//             unique: BTreeMap::new(),
//             filter: BTreeMap::new(),
//             forwarded: BTreeMap::new(),
//             select: BTreeMap::new(),
//         };
//         for expr in select.filter {
//             out.filter(expr);
//         }
//         out
//     }

//     pub fn select_alias(&mut self, val: Rc<Expr>) -> ColAlias {
//         if let Some(ops) = self.select.get(&val) {
//             return ops.alias;
//         }
//         let emitted = self.emit(&val);
//         let alias = ColAlias::Select(self.select.len());
//         let options = SelectOptions { emitted, alias };
//         assert!(self.select.insert(val, options).is_none());
//         alias
//     }

//     fn emit(&mut self, expr: &Expr) -> String {
//         match expr {
//             Expr::Constant(val) => (*val).to_owned(),
//             Expr::Parameter(ord_rc) => (),
//             Expr::AggrIndex(select, expr) => {}
//             Expr::RowIndex(row_like, _) => todo!(),
//             Expr::Prefix(_, expr) => self.emit(expr),
//             Expr::Infix(expr, _, expr1) => todo!(),
//             Expr::Func(_, exprs) => todo!(),
//         }
//     }

//     fn join_alias(&mut self, join: Join) -> TableAlias {
//         if let Some(from) = self.from.get(&join) {
//             return *from;
//         }
//         let next = self.forwarded.len();
//         let idx = *self.forwarded.entry(join).or_insert(next);
//         TableAlias::Forward(idx)
//     }

//     fn unique_alias(&mut self, unique: Unique) -> TableAlias {
//         if let Some(val) = self.unique.get(&unique) {
//             return val.alias;
//         }
//         let emitted_conds: Vec<_> = unique.conds.iter().map(|(_, v)| self.emit(v)).collect();
//         // reserve new alias after emitting all conds
//         let alias = TableAlias::Unique(self.unique.len());
//         let options = UniqueOptions {
//             alias,
//             filter: false,
//             emitted_conds,
//         };
//         assert!(self.unique.insert(unique, options,).is_none());
//         alias
//     }

//     fn filter(&mut self, expr: Rc<Expr>) {
//         if self.filter.contains_key(&expr) {
//             return;
//         }
//         // TODO: if the expr checks that some tableidx is some, we might be able
//         // to change joinoptions instead of filtering.
//         let emitted = self.emit(&expr);
//         assert!(self.filter.insert(expr, emitted).is_none());
//     }
// }
