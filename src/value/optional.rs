use std::marker::PhantomData;

use sea_query::Nullable;

use crate::{
    IntoSelect,
    dummy_impl::{Cached, Cacher, ColumnImpl, Prepared, Row, Select, SelectImpl},
};

use super::{DynTyped, Expr, IntoExpr, MyTyp, Typed};

/// This is a combinator function that allows constructing single row optional queries.
///
/// ```
/// # use rust_query::IntoExpr;
/// # let mut client = rust_query::private::doctest::get_client();
/// # let txn = rust_query::private::doctest::get_txn(&mut client);
/// # use rust_query::optional;
/// let res = txn.query_one(optional(|row| {
///     let x = row.and(Some("test"));
///     let y = row.and(Some(42));
///     row.then((x, y))
/// }));
/// assert_eq!(res, Some(("test".to_owned(), 42)));
/// ```
///
/// ```
/// # use rust_query::IntoExpr;
/// # let mut client = rust_query::private::doctest::get_client();
/// # let txn = rust_query::private::doctest::get_txn(&mut client);
/// # use rust_query::optional;
/// let res = txn.query_one(optional(|row| {
///     let x = row.and(Some("test"));
///     let y = row.and(None::<i64>);
///     row.then((x, y))
/// }));
/// assert_eq!(res, None);
/// ```
pub fn optional<'outer, S, R>(
    f: impl for<'inner> FnOnce(&mut Optional<'outer, 'inner, S>) -> R,
) -> R {
    let mut optional = Optional {
        nulls: Vec::new(),
        _p: PhantomData,
        _p2: PhantomData,
    };
    f(&mut optional)
}

/// This is the argument type used by the [optional] combinator.
///
/// Joining more optional columns can be done with the [Optional::and] method.
/// Finally it is possible to return selections or expressions using [Optional::then] and [Optional::then_expr].
pub struct Optional<'outer, 'inner, S> {
    nulls: Vec<DynTyped<bool>>,
    _p: PhantomData<&'inner &'outer ()>,
    _p2: PhantomData<S>,
}

impl<'outer, 'inner, S> Optional<'outer, 'inner, S> {
    /// Join an optional column to the current row.
    ///
    /// If the joined column is [None], then the whole [optional] combinator will return [None].
    #[doc(alias = "join")]
    pub fn and<T: 'static>(
        &mut self,
        col: impl IntoExpr<'inner, S, Typ = Option<T>>,
    ) -> Expr<'inner, S, T> {
        let column = col.into_expr();
        self.nulls.push(column.is_some().not().into_expr().inner);
        Expr::adhoc(move |b| column.inner.build_expr(b))
    }

    pub fn is_none(&self) -> Expr<'outer, S, bool> {
        let nulls = self.nulls.clone();
        Expr::adhoc(move |b| {
            nulls
                .iter()
                .map(|x| x.build_expr(b))
                .reduce(|a, b| a.or(b))
                .unwrap_or(false.into())
        })
    }

    /// Return a [bool] column indicating whether the current row exists.
    pub fn is_some(&self) -> Expr<'outer, S, bool> {
        self.is_none().not()
    }

    /// Return [Some] column if the current row exists and [None] column otherwise.
    pub fn then_expr<T: MyTyp<Sql: Nullable> + 'outer>(
        &self,
        col: impl IntoExpr<'inner, S, Typ = T>,
    ) -> Expr<'outer, S, Option<T>> {
        const NULL: sea_query::SimpleExpr =
            sea_query::SimpleExpr::Keyword(sea_query::Keyword::Null);

        let col = col.into_expr().inner;
        let nulls = self.nulls.clone();
        Expr::adhoc(move |b| {
            nulls.iter().fold(col.build_expr(b), |accum, e| {
                sea_query::Expr::case(e.build_expr(b), NULL)
                    .finally(accum)
                    .into()
            })
        })
    }

    /// Returns a [Select] with optional result.
    pub fn then<'transaction, Out: 'transaction>(
        &self,
        d: impl IntoSelect<'inner, 'transaction, S, Out = Out>,
    ) -> Select<'outer, 'transaction, S, Option<Out>> {
        Select::new(OptionalImpl {
            inner: d.into_select().inner,
            is_some: ColumnImpl {
                expr: self.is_some().into_expr().inner,
            },
        })
    }
}

pub struct OptionalImpl<X> {
    inner: X,
    is_some: ColumnImpl<bool>,
}

impl<'transaction, X: SelectImpl<'transaction>> SelectImpl<'transaction> for OptionalImpl<X> {
    type Out = Option<X::Out>;
    type Prepared = OptionalPrepared<X::Prepared>;

    fn prepare(self, cacher: &mut Cacher) -> Self::Prepared {
        OptionalPrepared {
            is_some: self.is_some.prepare(cacher),
            inner: self.inner.prepare(cacher),
        }
    }
}

pub struct OptionalPrepared<X> {
    inner: X,
    is_some: Cached<bool>,
}

impl<X: Prepared> Prepared for OptionalPrepared<X> {
    type Out = Option<X::Out>;

    fn call(&mut self, row: Row<'_>) -> Self::Out {
        if row.get(self.is_some) {
            Some(self.inner.call(row))
        } else {
            None
        }
    }
}
