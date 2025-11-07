use crate::{IntoExpr, IntoSelect, Table, TableRow, dummy_impl::Select, optional};

use super::MyTyp;

/// Trait for values that can be retrieved from the database using one expression.
///
/// This is most likely the trait that you want to implement for your custom datatype.
/// Together with the [crate::IntoExpr] trait.
///
/// Note that this trait can also be implemented using the [derive@rust_query::FromExpr] derive macro.
pub trait FromExpr<S, From>: 'static + Sized {
    /// How to turn the expression into a [Select].
    fn from_expr<'columns>(
        col: impl IntoExpr<'columns, S, Typ = From>,
    ) -> Select<'columns, S, Self>;
}

macro_rules! from_expr {
    ($typ:ty) => {
        impl<S> FromExpr<S, $typ> for $typ {
            fn from_expr<'columns>(
                col: impl IntoExpr<'columns, S, Typ = $typ>,
            ) -> Select<'columns, S, Self> {
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

impl<T: Table> FromExpr<T::Schema, T> for TableRow<T> {
    fn from_expr<'columns>(
        col: impl IntoExpr<'columns, T::Schema, Typ = T>,
    ) -> Select<'columns, T::Schema, Self> {
        col.into_expr().into_select()
    }
}

impl<S, T, From: MyTyp> FromExpr<S, Option<From>> for Option<T>
where
    T: FromExpr<S, From>,
{
    fn from_expr<'columns>(
        col: impl IntoExpr<'columns, S, Typ = Option<From>>,
    ) -> Select<'columns, S, Self> {
        let col = col.into_expr();
        optional(|row| {
            let col = row.and(col);
            row.then_select(T::from_expr(col))
        })
    }
}

impl<S, From> FromExpr<S, From> for () {
    fn from_expr<'columns>(
        _col: impl IntoExpr<'columns, S, Typ = From>,
    ) -> Select<'columns, S, Self> {
        ().into_select()
    }
}
