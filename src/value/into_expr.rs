use std::rc::Rc;

use crate::{
    Expr, Lazy, Table, TableRow,
    lower::{self, ord_rc::OrdRc},
    value::{DbTyp, db_typ::DbTypOpt},
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
    type Typ: DbTypOpt;

    /// Turn this value into an [Expr].
    fn into_expr(&self) -> Expr<'column, S, Self::Typ>;
}

impl<'column, S, T: IntoExpr<'column, S, Typ = X>, X: DbTyp> IntoExpr<'column, S> for Option<T> {
    type Typ = Option<X>;
    fn into_expr(&self) -> Expr<'column, S, Self::Typ> {
        let this = self.as_ref().map(|x| x.into_expr().inner);

        // TODO: check if the original expression is a parameter and then upgrade the parameter to be nullable instead
        Expr::new(this.unwrap_or_else(|| Rc::new(lower::Expr::Constant("NULL"))))
    }
}

impl<'column, S> IntoExpr<'column, S> for str {
    type Typ = String;
    fn into_expr(&self) -> Expr<'column, S, Self::Typ> {
        self.to_owned().into_expr()
    }
}

impl<'column, S> IntoExpr<'column, S> for [u8] {
    type Typ = Vec<u8>;
    fn into_expr(&self) -> Expr<'column, S, Self::Typ> {
        self.to_owned().into_expr()
    }
}

impl<'column, S, T> IntoExpr<'column, S> for &T
where
    T: ?Sized + IntoExpr<'column, S>,
{
    type Typ = T::Typ;
    fn into_expr(&self) -> Expr<'column, S, Self::Typ> {
        T::into_expr(self)
    }
}

impl<'column, T: Table> IntoExpr<'column, T::Schema> for TableRow<T> {
    type Typ = Self;
    fn into_expr(&self) -> Expr<'static, T::Schema, Self::Typ> {
        let idx = self.inner.idx;

        Expr::adhoc(lower::Expr::Parameter(OrdRc::new(idx)))
    }
}

impl<'column, T: Table> IntoExpr<'column, T::Schema> for Lazy<'_, T> {
    type Typ = TableRow<T>;

    fn into_expr(&self) -> crate::Expr<'column, T::Schema, Self::Typ> {
        self.id.into_expr()
    }
}

impl<'column, S, T: DbTypOpt> IntoExpr<'column, S> for Expr<'column, S, T> {
    type Typ = T;
    fn into_expr(&self) -> Expr<'column, S, Self::Typ> {
        self.clone()
    }
}
