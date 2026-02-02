use std::marker::PhantomData;

use sea_query::IntoIden;

use crate::{
    Expr, Table, TableRow,
    alias::JoinableTable,
    value::{DbTyp, DynTypedExpr},
};

pub trait IntoJoinable<'inner, S> {
    type Typ: DbTyp;
    fn into_joinable(self) -> Joinable<'inner, S, Self::Typ>;
}

pub struct Joinable<'inner, S, T: DbTyp> {
    _p: PhantomData<Expr<'inner, S, T>>,
    pub(crate) table: JoinableTable,
    pub(crate) conds: Vec<(&'static str, DynTypedExpr)>,
}

impl<'inner, S, T: DbTyp> Joinable<'inner, S, T> {
    pub fn new(j: JoinableTable) -> Self {
        Self {
            _p: PhantomData,
            table: j,
            conds: Vec::new(),
        }
    }
}

impl<'inner, S, T: Table> Joinable<'inner, S, TableRow<T>> {
    pub fn table() -> Self {
        Self::new(JoinableTable::Normal(T::NAME.into_iden()))
    }
}
impl<'inner, S, T: DbTyp> Joinable<'inner, S, T> {
    pub fn add_cond<C: DbTyp>(mut self, col: &'static str, val: Expr<'inner, S, C>) -> Self {
        self.conds.push((col, DynTypedExpr::erase(val)));
        self
    }
}

impl<'inner, S, T: DbTyp> IntoJoinable<'inner, S> for Joinable<'inner, S, T> {
    type Typ = T;

    fn into_joinable(self) -> Joinable<'inner, S, Self::Typ> {
        self
    }
}

#[cfg(false)] // vec support doesn't work yet
trait ConstExpr<S>: crate::IntoExpr<'static, S> {
    fn into_out(self) -> <Self::Typ as DbTyp>::Out;
}

#[cfg(false)] // vec support doesn't work yet
impl<'x, S, T: IntoIterator<Item: ConstExpr<S>>> IntoJoinable<'x, S> for T {
    type Typ = <T::Item as IntoExpr<'static, S>>::Typ;

    fn into_joinable(self) -> Joinable<'x, S, Self::Typ> {
        Joinable {
            _p: PhantomData,
            table: JoinableTable::Vec(
                self.into_iter()
                    .map(|x| Self::Typ::out_to_value(x.into_out()))
                    .collect(),
            ),
            conds: Vec::new(),
        }
    }
}
