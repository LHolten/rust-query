use crate::{IntoExpr, IntoSelect, Table, TableRow, dummy_impl::Select, optional};

use super::MyTyp;

/// Trait for values that can be retrieved from the database using one expression.
///
/// This is most likely the trait that you want to implement for your custom datatype.
/// Together with the [crate::IntoExpr] trait.
///
/// Note that this trait can also be implemented using the [derive@rust_query::FromExpr] derive macro.
pub trait FromExpr<'transaction, S, From>: 'transaction + Sized {
    /// How to turn a column reference into a [Select].
    fn from_expr<'columns>(
        col: impl IntoExpr<'columns, S, Typ = From>,
    ) -> Select<'columns, 'transaction, S, Self>;
}

macro_rules! from_expr {
    ($typ:ty) => {
        impl<'transaction, S> FromExpr<'transaction, S, $typ> for $typ {
            fn from_expr<'columns>(
                col: impl IntoExpr<'columns, S, Typ = $typ>,
            ) -> Select<'columns, 'transaction, S, Self> {
                col.into_expr().into_select()
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
    ) -> Select<'columns, 'transaction, T::Schema, Self> {
        col.into_expr().into_select()
    }
}

impl<'transaction, S, T, From: MyTyp> FromExpr<'transaction, S, Option<From>> for Option<T>
where
    T: FromExpr<'transaction, S, From>,
{
    fn from_expr<'columns>(
        col: impl IntoExpr<'columns, S, Typ = Option<From>>,
    ) -> Select<'columns, 'transaction, S, Self> {
        let col = col.into_expr();
        optional(|row| {
            let col = row.and(col);
            row.then(T::from_expr(col))
        })
    }
}

impl<'transaction, S, From> FromExpr<'transaction, S, From> for () {
    fn from_expr<'columns>(
        _col: impl IntoExpr<'columns, S, Typ = From>,
    ) -> Select<'columns, 'transaction, S, Self> {
        ().into_select()
    }
}
