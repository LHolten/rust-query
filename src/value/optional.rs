use std::{marker::PhantomData, rc::Rc};

use sea_query::Nullable;

use crate::{
    dummy_impl::{Cached, Cacher, ColumnImpl, Dummy, DummyImpl, Prepared, Row},
    IntoDummy,
};

use super::{
    operations::{Assume, NullIf, Or},
    DynTyped, Expr, IntoColumn, MyTyp,
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
/// Finally it is possible to return either columns or dummies using [Optional::then] and [Optional::then_dummy].
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
        col: impl IntoColumn<'inner, S, Typ = Option<T>>,
    ) -> Expr<'inner, S, T> {
        let column = col.into_column();
        self.nulls.push(column.is_some().not().into_column().inner);
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
        col: impl IntoColumn<'inner, S, Typ = T>,
    ) -> Expr<'outer, S, Option<T>> {
        let res = Expr::new(Some(col.into_column().inner));
        self.nulls
            .iter()
            .rfold(res, |accum, e| Expr::new(NullIf(e.clone(), accum.inner)))
    }

    /// Returns an optional dummy that can be used as the result of the query.
    pub fn then_dummy<'transaction, Out: 'transaction>(
        &self,
        d: impl IntoDummy<'inner, 'transaction, S, Out = Out>,
    ) -> Dummy<'outer, 'transaction, S, Option<Out>> {
        Dummy::new(OptionalImpl {
            inner: d.into_dummy().inner,
            is_some: ColumnImpl {
                expr: self.is_some().into_column().inner,
            },
        })
    }
}

pub struct OptionalImpl<X> {
    inner: X,
    is_some: ColumnImpl<bool>,
}

impl<'transaction, X: DummyImpl<'transaction>> DummyImpl<'transaction> for OptionalImpl<X> {
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
