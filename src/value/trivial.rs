use std::marker::PhantomData;

use crate::{optional, Dummy, Table, TableRow};

use super::{optional::OptionalDummy, Column, IntoColumn};

/// This trait is implemented for types that want to implement [FromColumn].
///
/// The [rust_query_macros::Dummy] derive macro will always implement this trait automatically.
pub trait FromDummy<'transaction, S> {
    /// The associated type here is the common return type of all [FromColumn] implementations.
    type Dummy<'columns>: Dummy<'columns, 'transaction, S, Out = Self>;
}

/// Trait for values that can be retrieved from the database using one reference column.
///
/// This is most likely the trait that you want to implement for your custom datatype.
///
/// Note that this trait can also be implemented using [rust_query_macros::Dummy] by
/// adding the `#[rust_query(From = Thing)]` helper attribute.
pub trait FromColumn<'transaction, S, From>: FromDummy<'transaction, S> {
    /// How to turn a column reference into the associated dummy type of [FromDummy].
    fn from_column<'columns>(col: Column<'columns, S, From>) -> Self::Dummy<'columns>;
}

/// This type implements [Dummy] for any column if there is a matching [FromColumn] implementation.
pub struct Trivial<'columns, S, T, X> {
    pub(crate) col: Column<'columns, S, T>,
    pub(crate) _p: PhantomData<X>,
}

impl<'transaction, 'columns, S, T, X> Dummy<'columns, 'transaction, S>
    for Trivial<'columns, S, T, X>
where
    X: FromColumn<'transaction, S, T>,
    X: 'transaction,
{
    type Out = X;

    type Prepared<'i> = <X::Dummy<'columns> as Dummy<'columns, 'transaction, S>>::Prepared<'i>;

    fn prepare<'i>(
        self,
        cacher: &mut crate::dummy_impl::Cacher<'columns, 'i, S>,
    ) -> Self::Prepared<'i> {
        X::from_column(self.col).prepare(cacher)
    }
}

macro_rules! from_column {
    ($typ:ty) => {
        impl<'transaction, S> FromDummy<'transaction, S> for $typ {
            type Dummy<'columns> = Column<'columns, S, $typ>;
        }
        impl<'transaction, S> FromColumn<'transaction, S, $typ> for $typ {
            fn from_column<'columns>(col: Column<'columns, S, $typ>) -> Self::Dummy<'columns> {
                col
            }
        }
    };
}

from_column! {String}
from_column! {i64}
from_column! {f64}
from_column! {bool}

impl<'transaction, T: Table> FromDummy<'transaction, T::Schema> for TableRow<'transaction, T> {
    type Dummy<'columns> = Column<'columns, T::Schema, T>;
}
impl<'t, T: Table> FromColumn<'t, T::Schema, T> for TableRow<'t, T> {
    fn from_column<'columns>(col: Column<'columns, T::Schema, T>) -> Self::Dummy<'columns> {
        col
    }
}

impl<'transaction, S, T> FromDummy<'transaction, S> for Option<T>
where
    T: FromDummy<'transaction, S>,
{
    type Dummy<'columns> = OptionalDummy<
        'columns,
        S,
        <T::Dummy<'columns> as Dummy<'columns, 'transaction, S>>::Prepared<'static>,
    >;
}
impl<'transaction, S, T: 'transaction, From: 'static, P> FromColumn<'transaction, S, Option<From>>
    for Option<T>
where
    T: FromColumn<'transaction, S, From>,
    for<'columns> T::Dummy<'columns>: Dummy<'columns, 'transaction, S, Prepared<'static> = P>,
{
    fn from_column<'columns>(col: Column<'columns, S, Option<From>>) -> Self::Dummy<'columns> {
        optional(|row| {
            let col = row.and(col);
            row.then_dummy(col.into_trivial::<T>())
        })
    }
}
