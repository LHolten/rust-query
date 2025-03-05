use crate::{IntoDummy, Table, TableRow, dummy_impl::Dummy, optional};

use super::{Expr, MyTyp};

/// Trait for values that can be retrieved from the database using one reference column.
///
/// This is most likely the trait that you want to implement for your custom datatype.
/// Together with the [crate::IntoExpr] trait.
///
/// Note that this trait can also be implemented using [rust_query_macros::Dummy] by
/// adding the `#[rust_query(From = Thing)]` helper attribute.
pub trait FromExpr<'transaction, S, From>: 'transaction + Sized {
    /// How to turn a column reference into a [Dummy].
    fn from_expr<'columns>(col: Expr<'columns, S, From>) -> Dummy<'columns, 'transaction, S, Self>;
}

macro_rules! from_expr {
    ($typ:ty) => {
        impl<'transaction, S> FromExpr<'transaction, S, $typ> for $typ {
            fn from_expr<'columns>(
                col: Expr<'columns, S, $typ>,
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
        col: Expr<'columns, T::Schema, T>,
    ) -> Dummy<'columns, 'transaction, T::Schema, Self> {
        col.into_dummy()
    }
}

impl<'transaction, S, T, From: MyTyp> FromExpr<'transaction, S, Option<From>> for Option<T>
where
    T: FromExpr<'transaction, S, From>,
{
    fn from_expr<'columns>(
        col: Expr<'columns, S, Option<From>>,
    ) -> Dummy<'columns, 'transaction, S, Self> {
        optional(|row| {
            let col = row.and(col);
            row.then_dummy(T::from_expr(col))
        })
    }
}

impl<'transaction, S, From> FromExpr<'transaction, S, From> for () {
    fn from_expr<'columns>(
        _col: Expr<'columns, S, From>,
    ) -> Dummy<'columns, 'transaction, S, Self> {
        ().into_dummy()
    }
}
