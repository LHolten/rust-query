pub(crate) mod emit;
pub(crate) mod list_writer;
pub(crate) mod ord_rc;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::{collections::BTreeSet, rc::Rc};

use ord_rc::OrdRc;

pub const CONST_1: Expr = Expr::Constant("1");
pub const CONST_0: Expr = Expr::Constant("0");
pub const CONST_FALSE: Expr = Expr::Constant("false");
pub const CONST_NULL: Expr = Expr::Constant("NULL");

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum JoinableTable {
    Table(&'static str),
    Tmp(TmpTable),
    Pragma(&'static str, Vec<OrdRc<dyn rusqlite::ToSql>>),
    // Vec(OrdRc<Vec<rusqlite::types::Value>>),
}

/// Specific join of a table
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Join(OrdRc<JoinableTable>);

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Unique {
    table: JoinableTable,
    conds: Vec<(&'static str, Rc<Expr>)>,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum RowLike {
    Join(Join),
    Unique(Rc<Unique>),
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub enum Expr {
    Constant(&'static str),
    Parameter(OrdRc<dyn rusqlite::ToSql>),
    AggrIndex(Rc<Select>, Rc<Expr>),
    RowIndex(RowLike, &'static str),
    Prefix(&'static str, Rc<Expr>),
    Infix(Rc<Expr>, &'static str, Rc<Expr>),
    Func(&'static str, Box<[Rc<Expr>]>),
}

impl Expr {
    /// Only use this on expressions that represent the id of a table
    pub fn col(self: &Rc<Self>, table: JoinableTable, col: &'static str) -> Rc<Expr> {
        if let Expr::RowIndex(table, "id") = Rc::as_ref(self) {
            // if this is already a join then we can just change the column
            return Rc::new(Expr::RowIndex(table.clone(), col));
        }

        let unique = Unique {
            table,
            conds: vec![("id", self.clone())],
        };
        let row = RowLike::Unique(Rc::new(unique));
        Rc::new(Expr::RowIndex(row, col))
    }
}

/// Select can have multiple results.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Select {
    /// There is at most one result for every combinator of rows in the `from` tables.
    /// BTreeSet is used for easier lookup.
    from: BTreeSet<Join>,
    filter: BTreeSet<Rc<Expr>>,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SelectVec {
    from: Vec<Join>,
    filter: BTreeSet<Rc<Expr>>,
}

impl Select {
    pub fn join(self: &mut Rc<Self>, table: JoinableTable) -> Join {
        let join = Join(OrdRc(Rc::new(table)));
        let this = Rc::make_mut(self);
        assert!(this.from.insert(join.clone()));
        join
    }

    pub fn filter(self: &mut Rc<Self>, expr: Rc<Expr>) {
        let this = Rc::make_mut(self);
        this.filter.insert(expr);
    }

    pub fn into_vecs(self) -> SelectVec {
        SelectVec {
            from: self.from.into_iter().collect(),
            filter: self.filter,
        }
    }
}

impl JoinableTable {
    pub fn main_column(&self) -> &'static str {
        match self {
            JoinableTable::Normal(_) => "id",
            JoinableTable::Pragma(_, _) => "pragma_id", // should always be replaced
            #[cfg(false)]
            JoinableTable::Vec(_) => "value",
        }
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

    pub fn create(num_tables: usize, num_filter_on: usize) -> Self {
        Self {
            iden_num: AtomicUsize::new(num_tables.max(num_filter_on)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct TmpTable {
    name: usize,
}
