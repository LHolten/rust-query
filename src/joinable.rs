use std::{marker::PhantomData, rc::Rc};

use crate::{Expr, Table, TableRow, lower, value::DbTyp};

pub trait IntoJoinable<'inner, S> {
    type Typ: DbTyp;
    fn into_joinable(self) -> Joinable<'inner, S, Self::Typ>;
}

pub struct Joinable<'inner, S, T: DbTyp> {
    _p: PhantomData<Expr<'inner, S, T>>,
    pub(crate) table: lower::JoinableTable,
    pub(crate) conds: Vec<(&'static str, Rc<lower::Expr>)>,
    pub(crate) main_column: &'static str,
}

impl<'inner, S, T: DbTyp> Joinable<'inner, S, T> {
    pub fn new(j: lower::JoinableTable, main_column: &'static str) -> Self {
        Self {
            _p: PhantomData,
            table: j,
            conds: Vec::new(),
            main_column,
        }
    }
}

impl<'inner, S, T: Table> Joinable<'inner, S, TableRow<T>> {
    pub fn table() -> Self {
        Self::new(lower::JoinableTable::Table(T::NAME), T::ID)
    }
}
impl<'inner, S, T: DbTyp> Joinable<'inner, S, T> {
    pub fn add_cond<C: DbTyp>(mut self, col: &'static str, val: Expr<'inner, S, C>) -> Self {
        self.conds.push((col, val.inner));
        self
    }
}

impl<'inner, S, T: DbTyp> IntoJoinable<'inner, S> for Joinable<'inner, S, T> {
    type Typ = T;

    fn into_joinable(self) -> Joinable<'inner, S, Self::Typ> {
        self
    }
}
