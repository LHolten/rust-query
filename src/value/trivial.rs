use std::marker::PhantomData;

use crate::{
    dummy::{OptionalDummy, Prepared},
    optional, Dummy, Table, TableRow,
};

use super::{Column, IntoColumn};

pub trait FromColumn<'transaction, S> {
    type From: 'static;
    type Dummy<'columns>: Dummy<'columns, 'transaction, S, Out = Self>;

    fn from_column<'columns>(col: Column<'columns, S, Self::From>) -> Self::Dummy<'columns>;
}

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

    type Prepared<'i> = <X::Dummy<'columns> as Dummy<'columns, 'transaction, S>>::Prepared<'i>;

    fn prepare<'i>(self, cacher: &mut crate::dummy::Cacher<'columns, 'i, S>) -> Self::Prepared<'i> {
        X::from_column(self.col).prepare(cacher)
    }
}

macro_rules! from_column {
    ($typ:ty) => {
        impl<S> FromColumn<'_, S> for $typ {
            type From = $typ;
            type Dummy<'columns> = Column<'columns, S, Self::From>;

            fn from_column<'columns>(
                col: Column<'columns, S, Self::From>,
            ) -> Self::Dummy<'columns> {
                col
            }
        }
    };
}

from_column! {String}
from_column! {i64}
from_column! {f64}
from_column! {bool}

impl<'t, T: Table> FromColumn<'t, T::Schema> for TableRow<'t, T> {
    type From = T;
    type Dummy<'columns> = Column<'columns, T::Schema, Self::From>;

    fn from_column<'columns>(
        col: Column<'columns, T::Schema, Self::From>,
    ) -> Self::Dummy<'columns> {
        col
    }
}

impl<'transaction, S, T, P> FromColumn<'transaction, S> for Option<T>
where
    T: FromColumn<'transaction, S>,
    for<'columns> T::Dummy<'columns>: Dummy<'columns, 'transaction, S, Prepared<'static> = P>,
    P: Prepared<'static, 'transaction, Out = T>,
{
    type From = Option<T::From>;
    type Dummy<'columns> = OptionalDummy<'columns, S, P>;

    fn from_column<'columns>(col: Column<'columns, S, Self::From>) -> Self::Dummy<'columns> {
        optional(|row| {
            let col = row.lower(col);
            let col = row.and(col);
            row.then_dummy(col.trivial::<T>())
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
