use std::{marker::PhantomData, rc::Rc};

use sea_query::Nullable;

use crate::{
    IntoSelect,
    dummy_impl::{Cached, Cacher, ColumnImpl, Prepared, Row, Select, SelectImpl},
};

use super::{
    DynTyped, Expr, IntoExpr, MyTyp,
    operations::{Assume, NullIf, Or},
};

/// This is a combinator function that allows constructing single row optional queries.
///
/// For more information refer to [Optional];
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
/// Finally it is possible to return either columns or dummies using [Optional::then] and [Optional::then_select].
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
        Expr::new(Assume(column.inner))
    }

    /// Return a [bool] column indicating whether the current row exists.
    pub fn is_some(&self) -> Expr<'outer, S, bool> {
        let any_null = self
            .nulls
            .iter()
            .cloned()
            .reduce(|a, b| DynTyped(Rc::new(Or(a, b))));
        // TODO: make this not double wrap the `DynTyped`
        any_null.map_or(Expr::new(true), |x| Expr::new(x).not())
    }

    /// Return [Some] column if the current row exists and [None] column otherwise.
    pub fn then<T: MyTyp<Sql: Nullable> + 'outer>(
        &self,
        col: impl IntoExpr<'inner, S, Typ = T>,
    ) -> Expr<'outer, S, Option<T>> {
        let res = Expr::new(Some(col.into_expr().inner));
        self.nulls
            .iter()
            .rfold(res, |accum, e| Expr::new(NullIf(e.clone(), accum.inner)))
    }

    /// Returns a [Select] with optional result.
    pub fn then_select<'transaction, Out: 'transaction>(
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
