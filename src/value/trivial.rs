use crate::{IntoDummy, IntoExpr, Table, TableRow, dummy_impl::Dummy, optional};

use super::MyTyp;

/// Trait for values that can be retrieved from the database using one reference column.
///
/// This is most likely the trait that you want to implement for your custom datatype.
/// Together with the [crate::IntoExpr] trait.
///
/// Note that this trait can also be implemented using [rust_query_macros::Dummy] by
/// adding the `#[rust_query(From = Thing)]` helper attribute.
pub trait FromExpr<'transaction, S, From>: 'transaction + Sized {
    /// How to turn a column reference into a [Dummy].
    fn from_expr<'columns>(
        col: impl IntoExpr<'columns, S, Typ = From>,
    ) -> Dummy<'columns, 'transaction, S, Self>;
}

macro_rules! from_expr {
    ($typ:ty) => {
        impl<'transaction, S> FromExpr<'transaction, S, $typ> for $typ {
            fn from_expr<'columns>(
                col: impl IntoExpr<'columns, S, Typ = $typ>,
            ) -> Dummy<'columns, 'transaction, S, Self> {
                col.into_dummy()
            }
        }
    };
}

from_expr! {String}
from_expr! {Vec<u8>}
from_expr! {i64}
from_expr! {f64}
from_expr! {bool}

impl<'transaction, T: Table> FromExpr<'transaction, T::Schema, T> for TableRow<'transaction, T> {
    fn from_expr<'columns>(
        col: impl IntoExpr<'columns, T::Schema, Typ = T>,
    ) -> Dummy<'columns, 'transaction, T::Schema, Self> {
        col.into_dummy()
    }
}

impl<'transaction, S, T, From: MyTyp> FromExpr<'transaction, S, Option<From>> for Option<T>
where
    T: FromExpr<'transaction, S, From>,
{
    fn from_expr<'columns>(
        col: impl IntoExpr<'columns, S, Typ = Option<From>>,
    ) -> Dummy<'columns, 'transaction, S, Self> {
        let col = col.into_expr();
        optional(|row| {
            let col = row.and(col);
            row.then_dummy(T::from_expr(col))
        })
    }
}

impl<'transaction, S, From> FromExpr<'transaction, S, From> for () {
    fn from_expr<'columns>(
        _col: impl IntoExpr<'columns, S, Typ = From>,
    ) -> Dummy<'columns, 'transaction, S, Self> {
        ().into_dummy()
    }
}
