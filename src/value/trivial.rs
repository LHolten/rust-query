use crate::{dummy_impl::Dummy, optional, IntoDummy, Table, TableRow};

use super::{Column, MyTyp};

/// Trait for values that can be retrieved from the database using one reference column.
///
/// This is most likely the trait that you want to implement for your custom datatype.
/// Together with the [crate::IntoColumn] trait.
///
/// Note that this trait can also be implemented using [rust_query_macros::Dummy] by
/// adding the `#[rust_query(From = Thing)]` helper attribute.
pub trait FromColumn<'transaction, S, From>: 'transaction + Sized {
    /// How to turn a column reference into a [Dummy].
    fn from_column<'columns>(
        col: Column<'columns, S, From>,
    ) -> Dummy<'columns, 'transaction, S, Self>;
}

macro_rules! from_column {
    ($typ:ty) => {
        impl<'transaction, S> FromColumn<'transaction, S, $typ> for $typ {
            fn from_column<'columns>(
                col: Column<'columns, S, $typ>,
            ) -> Dummy<'columns, 'transaction, S, Self> {
                col.into_dummy()
            }
        }
    };
}

from_column! {String}
from_column! {Vec<u8>}
from_column! {i64}
from_column! {f64}
from_column! {bool}

impl<'transaction, T: Table> FromColumn<'transaction, T::Schema, T> for TableRow<'transaction, T> {
    fn from_column<'columns>(
        col: Column<'columns, T::Schema, T>,
    ) -> Dummy<'columns, 'transaction, T::Schema, Self> {
        col.into_dummy()
    }
}

impl<'transaction, S, T, From: MyTyp> FromColumn<'transaction, S, Option<From>> for Option<T>
where
    T: FromColumn<'transaction, S, From>,
{
    fn from_column<'columns>(
        col: Column<'columns, S, Option<From>>,
    ) -> Dummy<'columns, 'transaction, S, Self> {
        optional(|row| {
            let col = row.and(col);
            row.then_dummy(T::from_column(col))
        })
    }
}

impl<'transaction, S, From> FromColumn<'transaction, S, From> for () {
    fn from_column<'columns>(
        _col: Column<'columns, S, From>,
    ) -> Dummy<'columns, 'transaction, S, Self> {
        ().into_dummy()
    }
}
