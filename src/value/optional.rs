use std::{marker::PhantomData, rc::Rc};

use sea_query::Nullable;

use crate::{
    dummy_impl::{Cached, Cacher, Package, Prepared, Row},
    Dummy,
};

use super::{
    operations::{Assume, NullIf, Or},
    Column, DynTyped, DynTypedExpr, IntoColumn, MyTyp,
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
    ) -> Column<'inner, S, T> {
        let column = col.into_column();
        self.nulls.push(column.is_some().not().into_column().inner);
        Column::new(Assume(column.inner))
    }

    /// Return [Some] column if the current row exists and [None] column otherwise.
    pub fn then<T: MyTyp<Sql: Nullable> + 'outer>(
        &self,
        col: impl IntoColumn<'inner, S, Typ = T>,
    ) -> Column<'outer, S, Option<T>> {
        let res = Column::new(Some(col.into_column().inner));
        self.nulls
            .iter()
            .rfold(res, |accum, e| Column::new(NullIf(e.clone(), accum.inner)))
    }

    /// Return a [bool] column indicating whether the current row exists.
    pub fn is_some(&self) -> Column<'outer, S, bool> {
        let any_null = self
            .nulls
            .iter()
            .cloned()
            .reduce(|a, b| DynTyped(Rc::new(Or(a, b))));
        // TODO: make this not double wrap the `DynTyped`
        any_null.map_or(Column::new(true), |x| Column::new(x).not())
    }

    /// Returns an optional dummy that can be used as the result of the query.
    pub fn then_dummy<'transaction, P>(
        &self,
        d: impl Dummy<'inner, 'transaction, S, Prepared = P>,
    ) -> OptionalDummy<'outer, S, P> {
        let mut cacher = Cacher::new();
        OptionalDummy {
            inner: d.prepare(&mut cacher).inner,
            is_some: cacher.cache(self.is_some().inner),
            columns: cacher.columns,
            _p: PhantomData,
            _p2: PhantomData,
        }
    }
}

/// Erases the `'i` lifetime
pub struct OptionalDummy<'columns, S, X> {
    pub(crate) columns: Vec<DynTypedExpr>,
    pub(crate) inner: X,
    pub(crate) is_some: Cached<bool>,
    pub(crate) _p: PhantomData<fn(&'columns ()) -> &'columns ()>,
    pub(crate) _p2: PhantomData<S>,
}

impl<'columns, 'transaction, S, X: Prepared<'transaction>> Dummy<'columns, 'transaction, S>
    for OptionalDummy<'columns, S, X>
{
    type Out = Option<X::Out>;
    type Prepared = OptionalPrepared<X>;

    fn prepare<'i>(self, cacher: &'i mut Cacher<'columns, S>) -> Package<'i, Self::Prepared> {
        let mut diff = None;
        self.columns.into_iter().enumerate().for_each(|(old, x)| {
            let new = cacher.cache_erased(x);
            let _diff = new - old;
            debug_assert!(diff.is_none_or(|it| it == _diff));
            diff = Some(_diff);
        });
        let diff = diff.unwrap_or_default();
        Package::new(OptionalPrepared {
            offset: diff,
            inner: self.inner,
            is_some: self.is_some,
        })
    }
}

pub struct OptionalPrepared<X> {
    offset: usize,
    inner: X,
    is_some: Cached<bool>,
}

impl<'i, 'a, X: Prepared<'a>> Prepared<'a> for OptionalPrepared<X> {
    type Out = Option<X::Out>;

    fn call(&mut self, row: Row<'_>) -> Self::Out {
        let row = Row::new(row.row, &row.fields[self.offset..]);
        if row.get(self.is_some) {
            Some(self.inner.call(row))
        } else {
            None
        }
    }
}
