use std::{marker::PhantomData, rc::Rc};

use sea_query::Nullable;

use crate::{
    dummy::{Cached, Cacher, Prepared, PubDummy, Row},
    Dummy,
};

use super::{
    operations::{Assume, NullIf, Or},
    Column, DynTyped, IntoColumn, MyTyp,
};

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

pub struct Optional<'outer, 'inner, S> {
    nulls: Vec<DynTyped<bool>>,
    _p: PhantomData<&'inner &'outer ()>,
    _p2: PhantomData<S>,
}

impl<'outer, 'inner, S> Optional<'outer, 'inner, S> {
    /// This method exists for now because `Column` is currently invariant in its lifetime
    pub fn lower<T: 'static>(
        &self,
        col: impl IntoColumn<'outer, S, Typ = T>,
    ) -> Column<'inner, S, T> {
        Column::new(col.into_column().inner)
    }

    /// Could be renamed to `join`
    pub fn and<T: 'static>(
        &mut self,
        col: impl IntoColumn<'inner, S, Typ = Option<T>>,
    ) -> Column<'inner, S, T> {
        let column = col.into_column();
        self.nulls.push(column.is_some().not().into_column().inner);
        Column::new(Assume(column.inner))
    }

    /// Could be renamed `map`
    pub fn then<T: MyTyp<Sql: Nullable> + 'outer>(
        &self,
        col: impl IntoColumn<'inner, S, Typ = T>,
    ) -> Column<'outer, S, Option<T>> {
        let res = Column::new(Some(col.into_column().inner));
        self.nulls
            .iter()
            .rfold(res, |accum, e| Column::new(NullIf(e.clone(), accum.inner)))
    }

    pub fn is_some(&self) -> Column<'outer, S, bool> {
        let any_null = self
            .nulls
            .iter()
            .cloned()
            .reduce(|a, b| DynTyped(Rc::new(Or(a, b))));
        // TODO: make this not double wrap the `DynTyped`
        any_null.map_or(Column::new(true), |x| Column::new(x).not())
    }

    pub fn then_dummy<'transaction, P>(
        &self,
        d: impl Dummy<'inner, 'transaction, S, Prepared<'static> = P>,
    ) -> PubDummy<'outer, S, OptionalPrepared<P>> {
        let d = PubDummy::new(d);
        let mut cacher = Cacher {
            _p: PhantomData,
            _p2: PhantomData,
            columns: d.columns,
        };
        let is_some = cacher.cache(self.is_some());
        PubDummy {
            columns: cacher.columns,
            inner: OptionalPrepared {
                inner: d.inner,
                is_some,
            },
            _p: PhantomData,
            _p2: PhantomData,
        }
    }
}

pub struct OptionalPrepared<X> {
    inner: X,
    is_some: Cached<'static, bool>,
}

impl<'transaction, X> Prepared<'static, 'transaction> for OptionalPrepared<X>
where
    X: Prepared<'static, 'transaction>,
{
    type Out = Option<X::Out>;

    fn call(&mut self, row: Row<'_, 'static, 'transaction>) -> Self::Out {
        if row.get(self.is_some) {
            Some(self.inner.call(row))
        } else {
            None
        }
    }
}
