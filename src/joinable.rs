use std::marker::PhantomData;

use sea_query::IntoIden;

use crate::{
    Expr, Table,
    alias::JoinableTable,
    value::{DynTypedExpr, MyTyp},
};

pub trait IntoJoinable<'inner, S> {
    type Typ: MyTyp;
    fn into_joinable(self) -> Joinable<'inner, S, Self::Typ>;
}

/// This struct exists because Joinable is not covariant in `'inner`.
/// We can get a covariant value by converting to [DynJoinable].
pub struct Joinable<'inner, S, T: MyTyp> {
    _p: PhantomData<Expr<'inner, S, T>>,
    pub(crate) table: JoinableTable,
    pub(crate) conds: Vec<(&'static str, DynTypedExpr)>,
}

impl<'inner, S, T: Table> Joinable<'inner, S, T> {
    pub fn table(conds: Vec<(&'static str, DynTypedExpr)>) -> Self {
        Self {
            _p: PhantomData,
            table: JoinableTable::Normal(T::NAME.into_iden()),
            conds,
        }
    }
}

impl<'inner, S, T: MyTyp> IntoJoinable<'inner, S> for Joinable<'inner, S, T> {
    type Typ = T;

    fn into_joinable(self) -> Joinable<'inner, S, Self::Typ> {
        self
    }
}
