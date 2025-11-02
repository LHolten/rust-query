use std::marker::PhantomData;

use crate::{Expr, Table, value::DynTypedExpr};

pub trait Joinable<'inner> {
    type Typ: Table;
    fn conds(self) -> Vec<(&'static str, DynTypedExpr)>;
}

/// This struct exists because Joinable is not covariant in `'inner`.
/// We can restore the convariant by making [DynJoinable].
pub struct DynJoinable<'inner, T: Table> {
    _p: PhantomData<Expr<'inner, T::Schema, T>>,
    conds: Vec<(&'static str, DynTypedExpr)>,
}

impl<'inner, T: Table> DynJoinable<'inner, T> {
    pub(crate) fn new(val: impl Joinable<'inner, Typ = T>) -> Self {
        Self {
            _p: PhantomData,
            conds: val.conds(),
        }
    }
}

impl<'inner, T: Table> Joinable<'inner> for DynJoinable<'inner, T> {
    type Typ = T;

    fn conds(self) -> Vec<(&'static str, DynTypedExpr)> {
        self.conds
    }
}
