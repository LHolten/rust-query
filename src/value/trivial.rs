use std::marker::PhantomData;

use crate::{
    dummy::{Cacher, Prepared, Row},
    optional, Dummy, Table, TableRow,
};

use super::{optional::OptionalDummy, Column, IntoColumn};

/// Trait for values that can be retrieved from the database
///
/// Note that it is possible to get associated columns and even to do aggregates in here!
pub trait FromColumn<'transaction, S> {
    type From: 'static;
    type Dummy<'columns>: Dummy<'columns, 'transaction, S, Out = Self>;

    fn from_column<'columns>(col: Column<'columns, S, Self::From>) -> Self::Dummy<'columns>;
}

pub struct DynDummy<'columns, 'transaction, S, Out> {
    inner: Box<
        dyn 'columns
            + FnOnce(
                &mut Cacher<'columns, 'static, S>,
            )
                -> Box<dyn 'transaction + Prepared<'static, 'transaction, Out = Out>>,
    >,
    _p: PhantomData<fn(&'columns ()) -> &'columns ()>,
}

impl<'columns, 'transaction, S, Out> DynDummy<'columns, 'transaction, S, Out> {
    pub fn new<D>(val: D) -> Self
    where
        D: 'columns + Dummy<'columns, 'transaction, S, Out = Out>,
        D::Prepared<'static>: 'transaction,
    {
        Self {
            inner: Box::new(move |cacher| Box::new(val.prepare(cacher))),
            _p: PhantomData,
        }
    }
}

impl<'columns, 'transaction, S, Out> Dummy<'columns, 'transaction, S>
    for DynDummy<'columns, 'transaction, S, Out>
{
    type Out = Out;
    type Prepared<'i> = DynPrepared<'i, 'transaction, Out>;

    fn prepare<'i>(self, cacher: &mut Cacher<'columns, 'i, S>) -> Self::Prepared<'i> {
        DynPrepared {
            inner: (self.inner)(Cacher::from_ref(&mut cacher.columns)),
            _p: PhantomData,
        }
    }
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
