use std::marker::PhantomData;

use crate::{dummy::FromDummy, Dummy};

use super::Column;

pub struct Trivial<'columns, S, T, X> {
    pub(crate) col: Column<'columns, S, T>,
    pub(crate) _p: PhantomData<X>,
}

impl<'transaction, 'columns, S, T, X> Dummy<'columns, 'transaction, S>
    for Trivial<'columns, S, T, X>
where
    X: FromDummy<'transaction, From = T>,
{
    type Out = X;

    type Prepared<'i> = X::Prepared<'i>;

    fn prepare<'i>(self, cacher: &mut crate::dummy::Cacher<'columns, 'i, S>) -> Self::Prepared<'i> {
        X::prepare(self.col, cacher)
    }
}
