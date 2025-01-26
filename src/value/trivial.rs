use crate::{
    dummy_impl::{ColumnImpl, Dummy, DummyImpl},
    optional, IntoDummy, Table, TableRow,
};

use super::{optional::OptionalImpl, Column, MyTyp};

/// This trait is implemented for types that want to implement [FromColumn].
///
/// The [rust_query_macros::Dummy] derive macro will always implement this trait automatically.
pub trait FromDummy<'transaction, S> {
    /// The associated type here is the common return type of all [FromColumn] implementations.
    type Impl: DummyImpl<'transaction, S, Out = Self>;
}

/// Trait for values that can be retrieved from the database using one reference column.
///
/// This is most likely the trait that you want to implement for your custom datatype.
///
/// Note that this trait can also be implemented using [rust_query_macros::Dummy] by
/// adding the `#[rust_query(From = Thing)]` helper attribute.
pub trait FromColumn<'transaction, S, From>: FromDummy<'transaction, S> {
    /// How to turn a column reference into the associated dummy type of [FromDummy].
    fn from_column<'columns>(col: Column<'columns, S, From>) -> Dummy<'columns, Self::Impl>;
}

macro_rules! from_column {
    ($typ:ty) => {
        impl<S> FromDummy<'_, S> for $typ {
            type Impl = ColumnImpl<S, $typ>;
        }
        impl<S> FromColumn<'_, S, $typ> for $typ {
            fn from_column<'columns>(
                col: Column<'columns, S, $typ>,
            ) -> Dummy<'columns, Self::Impl> {
                col.into_dummy()
            }
        }
    };
}

from_column! {String}
from_column! {i64}
from_column! {f64}
from_column! {bool}

impl<'transaction, T: Table> FromDummy<'transaction, T::Schema> for TableRow<'transaction, T> {
    type Impl = ColumnImpl<T::Schema, T>;
}
impl<'transaction, T: Table> FromColumn<'transaction, T::Schema, T> for TableRow<'transaction, T> {
    fn from_column<'columns>(col: Column<'columns, T::Schema, T>) -> Dummy<'columns, Self::Impl> {
        col.into_dummy()
    }
}

impl<'transaction, S, T: FromDummy<'transaction, S>> FromDummy<'transaction, S> for Option<T> {
    type Impl = OptionalImpl<S, T::Impl>;
}
impl<'transaction, S, T, From: MyTyp> FromColumn<'transaction, S, Option<From>> for Option<T>
where
    T: FromColumn<'transaction, S, From>,
{
    fn from_column<'columns>(
        col: Column<'columns, S, Option<From>>,
    ) -> Dummy<'columns, Self::Impl> {
        optional(|row| {
            let col = row.and(col);
            row.then_dummy(T::from_column(col))
        })
    }
}
