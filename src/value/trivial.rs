use std::marker::PhantomData;

use crate::{
    dummy::{Cached, DynDummy, FromColumn},
    optional, Dummy, Table, TableRow,
};

use super::{optional::OptionalPrepared, Column, IntoColumn};

pub struct Trivial<'columns, S, T, X> {
    pub(crate) col: Column<'columns, S, T>,
    pub(crate) _p: PhantomData<X>,
}

impl<'transaction, 'columns, S, T, X> Dummy<'columns, 'transaction, S>
    for Trivial<'columns, S, T, X>
where
    X: FromColumn<'transaction, S, From = T>,
{
    type Out = X;

    type Prepared<'i> = X::Prepared<'i>;

    fn prepare<'i>(self, cacher: &mut crate::dummy::Cacher<'columns, 'i, S>) -> Self::Prepared<'i> {
        X::prepare(self.col, cacher)
    }
}

impl<S> FromColumn<'_, S> for String {
    type From = String;
    type Prepared<'i> = Cached<'i, String>;

    fn prepare<'i, 'columns>(
        col: Column<'columns, S, Self::From>,
        cacher: &mut crate::dummy::Cacher<'columns, 'i, S>,
    ) -> Self::Prepared<'i> {
        cacher.cache(col)
    }
}

impl<S> FromColumn<'_, S> for i64 {
    type From = i64;
    type Prepared<'i> = Cached<'i, i64>;

    fn prepare<'i, 'columns>(
        col: Column<'columns, S, Self::From>,
        cacher: &mut crate::dummy::Cacher<'columns, 'i, S>,
    ) -> Self::Prepared<'i> {
        cacher.cache(col)
    }
}

impl<S> FromColumn<'_, S> for f64 {
    type From = f64;
    type Prepared<'i> = Cached<'i, f64>;

    fn prepare<'i, 'columns>(
        col: Column<'columns, S, Self::From>,
        cacher: &mut crate::dummy::Cacher<'columns, 'i, S>,
    ) -> Self::Prepared<'i> {
        cacher.cache(col)
    }
}

impl<S> FromColumn<'_, S> for bool {
    type From = bool;
    type Prepared<'i> = Cached<'i, bool>;

    fn prepare<'i, 'columns>(
        col: Column<'columns, S, Self::From>,
        cacher: &mut crate::dummy::Cacher<'columns, 'i, S>,
    ) -> Self::Prepared<'i> {
        cacher.cache(col)
    }
}

impl<'t, T: Table> FromColumn<'t, T::Schema> for TableRow<'t, T> {
    type From = T;
    type Prepared<'i> = Cached<'i, T>;

    fn prepare<'i, 'columns>(
        col: Column<'columns, T::Schema, Self::From>,
        cacher: &mut crate::dummy::Cacher<'columns, 'i, T::Schema>,
    ) -> Self::Prepared<'i> {
        cacher.cache(col)
    }
}

impl<'transaction, S, T> FromColumn<'transaction, S> for Option<T>
where
    T: FromColumn<'transaction, S>,
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

impl<'t, S, T> Column<'t, S, T> {
    pub fn trivial<'x, X: FromColumn<'x, S, From = T>>(&self) -> Trivial<'t, S, T, X> {
        Trivial {
            col: self.clone(),
            _p: PhantomData,
        }
    }
}

impl<'t, T: Table> TableRow<'t, T> {
    pub fn trivial<'x, X: FromColumn<'x, T::Schema, From = T>>(
        &self,
    ) -> Trivial<'t, T::Schema, T, X> {
        Trivial {
            col: self.into_column(),
            _p: PhantomData,
        }
    }
}
