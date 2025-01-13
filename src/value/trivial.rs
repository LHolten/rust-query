use std::marker::PhantomData;

use crate::{
    dummy::{Cacher, Prepared, Row},
    optional, Dummy, Table, TableRow,
};

use super::{Column, IntoColumn};

/// Trait for values that can be retrieved from the database
///
/// Note that it is possible to get associated columns and even to do aggregates in here!
pub trait FromColumn<'transaction, S>: Sized {
    type From: 'static;
    // type Dummy<'columns>: Dummy<'columns, 'transaction, S, Out = Self>;

    fn from_column<'columns>(
        col: Column<'columns, S, Self::From>,
    ) -> impl Dummy<
        'columns,
        'transaction,
        S,
        Out = Self,
        Prepared<'static> = impl 'transaction + Prepared<'static, 'transaction, Out = Self>,
    >;
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

    type Prepared<'i> = DynPrepared<'i, 'transaction, X>;

    fn prepare<'i>(self, cacher: &mut crate::dummy::Cacher<'columns, 'i, S>) -> Self::Prepared<'i> {
        DynPrepared {
            inner: Box::new(
                X::from_column(self.col).prepare(Cacher::from_ref(&mut cacher.columns)),
            ),
            _p: PhantomData,
        }
    }
}

macro_rules! from_column {
    ($typ:ty) => {
        impl<'transaction, S> FromColumn<'transaction, S> for $typ {
            type From = $typ;

            fn from_column<'columns>(
                col: Column<'columns, S, Self::From>,
            ) -> impl Dummy<
                'columns,
                'transaction,
                S,
                Out = Self,
                Prepared<'static> = impl 'transaction + Prepared<'static, 'transaction, Out = Self>,
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

impl<'t, T: Table> FromColumn<'t, T::Schema> for TableRow<'t, T> {
    type From = T;

    fn from_column<'columns>(
        col: Column<'columns, T::Schema, Self::From>,
    ) -> impl Dummy<
        'columns,
        't,
        T::Schema,
        Out = Self,
        Prepared<'static> = impl 't + Prepared<'static, 't, Out = Self>,
    > {
        col
    }
}

impl<'transaction, S, T: 'static> FromColumn<'transaction, S> for Option<T>
where
    T: FromColumn<'transaction, S>,
{
    type From = Option<T::From>;

    fn from_column<'columns>(
        col: Column<'columns, S, Self::From>,
    ) -> impl Dummy<
        'columns,
        'transaction,
        S,
        Out = Self,
        Prepared<'static> = impl 'transaction + Prepared<'static, 'transaction, Out = Self>,
    > {
        optional(|row| {
            let col = row.lower(col);
            let col = row.and(col);
            row.then_dummy(col.trivial::<T>())
        })
    }
}
