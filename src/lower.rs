pub(crate) mod emit;
pub(crate) mod list_writer;
pub(crate) mod ord_rc;

use std::fmt::{Debug, Display};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{collections::BTreeSet, rc::Rc};

use ord_rc::OrdRc;

pub const CONST_0: Expr = Expr::Constant("0");
pub const CONST_FALSE: Expr = Expr::Constant("false");
pub const CONST_NULL: Expr = Expr::Constant("NULL");

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum JoinableTable {
    Table(&'static str),
    Tmp(TmpTable),
    Pragma(&'static str, Vec<OrdRc<rusqlite::types::Value>>),
    // Vec(OrdRc<Vec<rusqlite::types::Value>>),
}

/// Specific join of a table
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Join(OrdRc<JoinableTable>);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Unique {
    pub table: JoinableTable,
    pub conds: Vec<(&'static str, Rc<Expr>)>,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum RowLike {
    Join(Join),
    Unique(Rc<Unique>),
}

impl Debug for RowLike {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Join(arg0) => arg0.fmt(f),
            Self::Unique(arg0) => arg0.fmt(f),
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Expr {
    Constant(&'static str),
    Parameter(OrdRc<rusqlite::types::Value>),
    AggrIndex(Rows, Rc<Expr>),
    RowIndex(RowLike, &'static str),
    Prefix(&'static str, Rc<Expr>),
    Infix(Rc<Expr>, &'static str, Rc<Expr>),
    Func(&'static str, Box<[Rc<Expr>]>),
    In(Rc<Expr>, Box<[Rc<Expr>]>),
    Cast(Rc<Expr>, &'static str),
    Between(Rc<Expr>, Rc<Expr>, Rc<Expr>),
}

impl Expr {
    /// Only use this on expressions that represent the id of a table
    pub fn col(
        self: &Rc<Self>,
        table: JoinableTable,
        col: &'static str,
        main_col: &'static str,
    ) -> Rc<Expr> {
        if let Expr::RowIndex(table, old) = Rc::as_ref(self)
            && *old == main_col
        {
            // if this is already a join then we can just change the column
            return Rc::new(Expr::RowIndex(table.clone(), col));
        }

        let unique = Unique {
            table,
            conds: vec![(main_col, self.clone())],
        };
        let row = RowLike::Unique(Rc::new(unique));
        Rc::new(Expr::RowIndex(row, col))
    }
}

/// Select can have multiple results.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Rows {
    /// There is at most one result for every combinator of rows in the `from` tables.
    /// BTreeSet is used for easier lookup.
    from: Vec<Join>,
    filter: BTreeSet<Rc<Expr>>,
}

impl Rows {
    pub fn join(&mut self, table: JoinableTable) -> Join {
        let join = Join(OrdRc(Rc::new(table)));
        self.from.push(join.clone());
        join
    }

    pub fn filter(&mut self, expr: Rc<Expr>) {
        self.filter.insert(expr);
    }
}

#[derive(Default)]
pub struct Scope {
    iden_num: AtomicUsize,
}

impl Scope {
    pub fn tmp_table(&self) -> TmpTable {
        let next = self.iden_num.fetch_add(1, Ordering::Relaxed);
        TmpTable { name: next }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct TmpTable {
    name: usize,
}

impl Display for TmpTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("_tmp{}", self.name))
    }
}
