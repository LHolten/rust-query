mod emit;
pub(crate) mod list_writer;
mod ord_rc;

use std::{collections::BTreeSet, rc::Rc};

use ord_rc::OrdRc;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum JoinableTable {
    Table(&'static str),
    Pragma(&'static str, Vec<String>),
    // Vec(OrdRc<Vec<rusqlite::types::Value>>),
}

/// Specific join of a table
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Join(OrdRc<JoinableTable>);

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Unique {
    table: JoinableTable,
    conds: Vec<(&'static str, Rc<Expr>)>,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub enum RowLike {
    Join(Join),
    Unique(Unique),
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub enum Expr {
    Constant(&'static str),
    Parameter(OrdRc<dyn rusqlite::ToSql>),
    AggrIndex(Rc<SelectVec>, Rc<Expr>),
    RowIndex(Rc<RowLike>, &'static str),
    Prefix(&'static str, Rc<Expr>),
    Infix(Rc<Expr>, &'static str, Rc<Expr>),
    Func(&'static str, Rc<[Expr]>),
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
        let row = Rc::new(RowLike::Unique(unique));
        Rc::new(Expr::RowIndex(row, col))
    }
}

/// Select can have multiple results.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
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
