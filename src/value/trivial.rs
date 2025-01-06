use std::marker::PhantomData;

use crate::{
    dummy::{Cached, DynDummy, FromDummy},
    optional, Dummy,
};

use super::{optional::OptionalPrepared, Column};

pub struct Trivial<'columns, S, T, X> {
    pub(crate) col: Column<'columns, S, T>,
    pub(crate) _p: PhantomData<X>,
}

impl<'transaction, 'columns, S, T, X> Dummy<'columns, 'transaction, S>
    for Trivial<'columns, S, T, X>
where
    X: FromDummy<'transaction, S, From = T>,
{
    type Out = X;

    type Prepared<'i> = X::Prepared<'i>;

    fn prepare<'i>(self, cacher: &mut crate::dummy::Cacher<'columns, 'i, S>) -> Self::Prepared<'i> {
        X::prepare(self.col, cacher)
    }
}

impl<S> FromDummy<'_, S> for String {
    type From = String;
    type Prepared<'i> = Cached<'i, String>;

    fn prepare<'i, 'columns>(
        col: Column<'columns, S, Self::From>,
        cacher: &mut crate::dummy::Cacher<'columns, 'i, S>,
    ) -> Self::Prepared<'i> {
        cacher.cache(col)
    }
}

impl<S> FromDummy<'_, S> for i64 {
    type From = i64;
    type Prepared<'i> = Cached<'i, i64>;

    fn prepare<'i, 'columns>(
        col: Column<'columns, S, Self::From>,
        cacher: &mut crate::dummy::Cacher<'columns, 'i, S>,
    ) -> Self::Prepared<'i> {
        cacher.cache(col)
    }
}

impl<'transaction, S, T> FromDummy<'transaction, S> for Option<T>
where
    T: FromDummy<'transaction, S>,
{
    type From = Option<T::From>;
    type Prepared<'i> = DynDummy<'i, OptionalPrepared<T::Prepared<'static>>>;

    fn prepare<'i, 'columns>(
        col: Column<'columns, S, Self::From>,
        cacher: &mut crate::dummy::Cacher<'columns, 'i, S>,
    ) -> Self::Prepared<'i> {
        optional(|row| {
            let col = row.lower(col);
            let col = row.and(col);
            row.then_dummy(col.trivial::<T>()).prepare(cacher)
        })
    }
}
