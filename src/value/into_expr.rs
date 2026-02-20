use sea_query::Nullable;

use crate::{
    Expr, Lazy, Table, TableRow,
    value::{DbTyp, EqTyp},
};

/// Trait for all values that can be used as expressions in queries.
///
/// There is a hierarchy of types that can be used to build queries.
/// - [TableRow], [i64], [f64], [bool], `&[u8]`, `&str`:
///   These are the base types for building expressions. They all
///   implement [IntoExpr] and are [Copy]. Note that [TableRow] is special
///   because it refers to a table row that is guaranteed to exist.
/// - [Expr] is the type that all [IntoExpr] values can be converted into.
///   It has a lot of methods to combine expressions into more complicated expressions.
///   Next to those, it implements [std::ops::Deref], if the expression is a table expression.
///   This can be used to get access to the columns of the table, which can themselves be table expressions.
///   Note that combinators like [crate::optional] and [crate::aggregate] also have [Expr] as return type.
///
/// Note that while [Expr] implements [IntoExpr], you may want to use `&Expr` instead.
/// Using a reference lets you reuse [Expr] without calling [Clone] explicitly.
pub trait IntoExpr<'column, S> {
    /// The type of the expression.
    type Typ: DbTyp;

    /// Turn this value into an [Expr].
    fn into_expr(self) -> Expr<'column, S, Self::Typ>;
}

impl<'column, S, T: IntoExpr<'column, S, Typ = X>, X: EqTyp> IntoExpr<'column, S> for Option<T> {
    type Typ = Option<X>;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        let this = self.map(|x| x.into_expr().inner);
        Expr::adhoc(move |b| {
            this.as_ref()
                .map(|x| (x.func)(b))
                .unwrap_or(<X::Sql as Nullable>::null().into())
        })
    }
}

impl<'column, S> IntoExpr<'column, S> for String {
    type Typ = String;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        Expr::adhoc(move |_| sea_query::Expr::from(self.clone()))
    }
}

impl<'column, S> IntoExpr<'column, S> for &str {
    type Typ = String;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        self.to_owned().into_expr()
    }
}

impl<'column, S> IntoExpr<'column, S> for Vec<u8> {
    type Typ = Vec<u8>;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        Expr::adhoc(move |_| sea_query::Expr::from(self.clone()))
    }
}

impl<'column, S> IntoExpr<'column, S> for &[u8] {
    type Typ = Vec<u8>;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        self.to_owned().into_expr()
    }
}

impl<'column, S> IntoExpr<'column, S> for bool {
    type Typ = bool;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        Expr::adhoc(move |_| sea_query::Expr::from(self))
    }
}

impl<'column, S> IntoExpr<'column, S> for i64 {
    type Typ = i64;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        Expr::adhoc(move |_| sea_query::Expr::from(self))
    }
}
impl<'column, S> IntoExpr<'column, S> for f64 {
    type Typ = f64;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        Expr::adhoc(move |_| sea_query::Expr::from(self))
    }
}

impl<'column, S, T> IntoExpr<'column, S> for &T
where
    T: IntoExpr<'column, S> + Clone,
{
    type Typ = T::Typ;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        T::into_expr(self.clone())
    }
}

impl<'column, T: Table> IntoExpr<'column, T::Schema> for TableRow<T> {
    type Typ = Self;
    fn into_expr(self) -> Expr<'static, T::Schema, Self::Typ> {
        let idx = self.inner.idx;

        Expr::adhoc_promise(
            move |_| sea_query::Expr::val(idx),
            false, // table row is proof of existence
        )
    }
}

impl<'column, T: Table> IntoExpr<'column, T::Schema> for Lazy<'_, T> {
    type Typ = TableRow<T>;

    fn into_expr(self) -> crate::Expr<'column, T::Schema, Self::Typ> {
        self.id.into_expr()
    }
}

impl<'column, S, T: DbTyp> IntoExpr<'column, S> for Expr<'column, S, T> {
    type Typ = T;
    fn into_expr(self) -> Expr<'column, S, Self::Typ> {
        self
    }
}
