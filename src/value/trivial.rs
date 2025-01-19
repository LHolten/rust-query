use std::marker::PhantomData;

use crate::{
    dummy_impl::{DummyImpl, DummyParent, NewPackage, NotCached, Prepared},
    optional, Dummy, Table, TableRow,
};

use super::{optional::OptionalImpl, Column, IntoColumn};

/// This trait is implemented for types that want to implement [FromColumn].
///
/// The [rust_query_macros::Dummy] derive macro will always implement this trait automatically.
pub trait FromDummy {
    /// The associated type here is the common return type of all [FromColumn] implementations.
    type Impl: DummyImpl<Prepared: Prepared<Out = Self>>;
}

/// Trait for values that can be retrieved from the database using one reference column.
///
/// This is most likely the trait that you want to implement for your custom datatype.
///
/// Note that this trait can also be implemented using [rust_query_macros::Dummy] by
/// adding the `#[rust_query(From = Thing)]` helper attribute.
pub trait FromColumn<'transaction, S, From>: FromDummy {
    /// How to turn a column reference into the associated dummy type of [FromDummy].
    fn from_column<'columns>(col: Column<'columns, S, From>)
        -> NewPackage<'columns, S, Self::Impl>;
}

/// This type implements [Dummy] for any column if there is a matching [FromColumn] implementation.
pub struct Trivial<'columns, S, T, X> {
    pub(crate) col: Column<'columns, S, T>,
    pub(crate) _p: PhantomData<X>,
}

impl<'transaction, 'columns, S, T, X> DummyParent<'transaction> for Trivial<'columns, S, T, X>
where
    X: FromColumn<'transaction, S, T>,
{
    type Out = X;

    type Impl = X::Impl;
}

impl<'transaction, 'columns, S, T, X> Dummy<'columns, 'transaction, S>
    for Trivial<'columns, S, T, X>
where
    X: FromColumn<'transaction, S, T>,
{
    fn into_impl(self) -> NewPackage<'columns, S, Self::Impl> {
        X::from_column(self.col)
    }
}

macro_rules! from_column {
    ($typ:ty) => {
        impl FromDummy for $typ {
            type Impl = NotCached<$typ>;
        }
        impl<'transaction, S> FromColumn<'transaction, S, $typ> for $typ {
            fn from_column<'columns>(
                col: Column<'columns, S, $typ>,
            ) -> NewPackage<'columns, S, Self::Impl> {
                col.into_impl()
            }
        }
    };
}

from_column! {String}
from_column! {i64}
from_column! {f64}
from_column! {bool}

impl<'transaction, T> FromDummy for TableRow<'transaction, T> {
    type Impl = NotCached<Self>;
}
impl<'transaction, T: Table> FromColumn<'transaction, T::Schema, T> for TableRow<'transaction, T> {
    fn from_column<'columns>(
        col: Column<'columns, T::Schema, T>,
    ) -> NewPackage<'columns, T::Schema, Self::Impl> {
        col.into_impl()
    }
}

impl<T: FromDummy> FromDummy for Option<T> {
    type Impl = OptionalImpl<T::Impl>;
}
impl<'transaction, S, T, From: 'static> FromColumn<'transaction, S, Option<From>> for Option<T>
where
    T: FromColumn<'transaction, S, From>,
{
    fn from_column<'columns>(
        col: Column<'columns, S, Option<From>>,
    ) -> NewPackage<'columns, S, Self::Impl> {
        optional(|row| {
            let col = row.and(col);
            row.then_dummy(col.into_trivial::<T>())
        })
        .into_impl()
    }
}
