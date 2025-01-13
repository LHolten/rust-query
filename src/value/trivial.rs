use std::marker::PhantomData;

use crate::{
    dummy::{Cached, Prepared, Row},
    optional, Dummy, Table, TableRow,
};

use super::{optional::OptionalPrepared, Column, IntoColumn};

pub trait StaticPrepared<'transaction> {
    type Prepared<'i>: Prepared<'i, 'transaction, Out = Self>;
}

/// Trait for values that can be retrieved from the database
///
/// Note that it is possible to get associated columns and even to do aggregates in here!
pub trait FromColumn<'transaction, S>: StaticPrepared<'transaction> {
    type From: 'static;

    fn from_column<'columns>(
        col: Column<'columns, S, Self::From>,
    ) -> impl for<'i> Dummy<'columns, 'transaction, S, Out = Self, Prepared<'i> = Self::Prepared<'i>>;
}

pub struct DynPrepared<'i, 'transaction, Out> {
    inner: Box<dyn 'transaction + Prepared<'static, 'transaction, Out = Out>>,
    _p: PhantomData<&'i ()>,
}

impl<'i, 'transaction, Out> Prepared<'i, 'transaction> for DynPrepared<'i, 'transaction, Out> {
    type Out = Out;

    fn call(&mut self, row: crate::dummy::Row<'_, 'i, 'transaction>) -> Self::Out {
        self.inner.call(Row {
            _p: PhantomData,
            row: row.row,
            fields: row.fields,
        })
    }
}

pub struct Trivial<'columns, S, T, X> {
    pub(crate) col: Column<'columns, S, T>,
    pub(crate) _p: PhantomData<X>,
}

impl<'transaction, 'columns, S, T, X> Dummy<'columns, 'transaction, S>
    for Trivial<'columns, S, T, X>
where
    X: FromColumn<'transaction, S, From = T>,
    X: 'transaction,
{
    type Out = X;

    type Prepared<'i> = X::Prepared<'i>;

    fn prepare<'i>(self, cacher: &mut crate::dummy::Cacher<'columns, 'i, S>) -> Self::Prepared<'i> {
        X::from_column(self.col).prepare(cacher)
    }
}

macro_rules! from_column {
    ($typ:ty) => {
        impl<'transaction> StaticPrepared<'transaction> for $typ {
            type Prepared<'i> = Cached<'i, $typ>;
        }
        impl<'transaction, S> FromColumn<'transaction, S> for $typ {
            type From = $typ;

            fn from_column<'columns>(
                col: Column<'columns, S, Self::From>,
            ) -> impl for<'i> Dummy<
                'columns,
                'transaction,
                S,
                Out = Self,
                Prepared<'i> = Self::Prepared<'i>,
            > {
                col
            }
        }
    };
}

from_column! {String}
from_column! {i64}
from_column! {f64}
from_column! {bool}

impl<'transaction, T: Table> StaticPrepared<'transaction> for TableRow<'transaction, T> {
    type Prepared<'i> = Cached<'i, T>;
}
impl<'t, T: Table> FromColumn<'t, T::Schema> for TableRow<'t, T> {
    type From = T;

    fn from_column<'columns>(
        col: Column<'columns, T::Schema, Self::From>,
    ) -> impl for<'i> Dummy<'columns, 't, T::Schema, Out = Self, Prepared<'i> = Self::Prepared<'i>>
    {
        col
    }
}

impl<'transaction, T> StaticPrepared<'transaction> for Option<T>
where
    T: StaticPrepared<'transaction>,
{
    type Prepared<'i> = OptionalPrepared<'i, T::Prepared<'static>>;
}
impl<'transaction, S, T: 'static> FromColumn<'transaction, S> for Option<T>
where
    T: FromColumn<'transaction, S>,
{
    type From = Option<T::From>;

    fn from_column<'columns>(
        col: Column<'columns, S, Self::From>,
    ) -> impl for<'i> Dummy<'columns, 'transaction, S, Out = Self, Prepared<'i> = Self::Prepared<'i>>
    {
        optional(|row| {
            let col = row.lower(col);
            let col = row.and(col);
            row.then_dummy(col.trivial::<T>())
        })
    }
}
