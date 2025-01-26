use std::marker::PhantomData;

use crate::{
    dummy_impl::{ColumnImpl, Dummy, DummyImpl},
    optional, IntoDummy, Table, TableRow,
};

use super::{optional::OptionalImpl, Column, IntoColumn};

/// This trait is implemented for types that want to implement [FromColumn].
///
/// The [rust_query_macros::Dummy] derive macro will always implement this trait automatically.
pub trait FromDummy<'transaction> {
    /// The associated type here is the common return type of all [FromColumn] implementations.
    type Impl: DummyImpl<'transaction, Out = Self>;
}

/// Trait for values that can be retrieved from the database using one reference column.
///
/// This is most likely the trait that you want to implement for your custom datatype.
///
/// Note that this trait can also be implemented using [rust_query_macros::Dummy] by
/// adding the `#[rust_query(From = Thing)]` helper attribute.
pub trait FromColumn<'transaction, S, From>: FromDummy<'transaction> {
    /// How to turn a column reference into the associated dummy type of [FromDummy].
    fn from_column<'columns>(col: Column<'columns, S, From>) -> Dummy<'columns, S, Self::Impl>;
}

/// This type implements [Dummy] for any column if there is a matching [FromColumn] implementation.
pub struct Trivial<C, X> {
    pub(crate) col: C,
    pub(crate) _p: PhantomData<X>,
}

impl<'transaction, 'columns, S, C, X> IntoDummy<'columns, 'transaction, S> for Trivial<C, X>
where
    C: IntoColumn<'columns, S>,
    X: FromColumn<'transaction, S, C::Typ>,
{
    type Out = X;

    type Impl = X::Impl;

    fn into_dummy(self) -> Dummy<'columns, S, Self::Impl> {
        X::from_column(self.col.into_column())
    }
}

macro_rules! from_column {
    ($typ:ty) => {
        impl FromDummy<'_> for $typ {
            type Impl = ColumnImpl<$typ>;
        }
        impl<S> FromColumn<'_, S, $typ> for $typ {
            fn from_column<'columns>(
                col: Column<'columns, S, $typ>,
            ) -> Dummy<'columns, S, Self::Impl> {
                col.into_dummy()
            }
        }
    };
}

from_column! {String}
from_column! {i64}
from_column! {f64}
from_column! {bool}

impl<'transaction, T> FromDummy<'transaction> for TableRow<'transaction, T> {
    type Impl = ColumnImpl<Self>;
}
impl<'transaction, T: Table> FromColumn<'transaction, T::Schema, T> for TableRow<'transaction, T> {
    fn from_column<'columns>(
        col: Column<'columns, T::Schema, T>,
    ) -> Dummy<'columns, T::Schema, Self::Impl> {
        col.into_dummy()
    }
}

impl<'transaction, T: FromDummy<'transaction>> FromDummy<'transaction> for Option<T> {
    type Impl = OptionalImpl<T::Impl>;
}
impl<'transaction, S, T, From: 'static> FromColumn<'transaction, S, Option<From>> for Option<T>
where
    T: FromColumn<'transaction, S, From>,
{
    fn from_column<'columns>(
        col: Column<'columns, S, Option<From>>,
    ) -> Dummy<'columns, S, Self::Impl> {
        optional(|row| {
            let col = row.and(col);
            row.then_dummy(col.into_trivial::<T>())
        })
        .into_dummy()
    }
}
