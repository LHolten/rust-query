use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
    rc::Rc,
};

use sea_query::{Asterisk, ExprTrait, Func, SelectStatement};

use crate::{
    Expr,
    alias::MyAlias,
    rows::Rows,
    value::{EqTyp, IntoExpr, MyTableRef, MyTyp, NumTyp, Typed, ValueBuilder},
};

use super::DynTypedExpr;

/// This is the argument type used for [aggregate].
pub struct Aggregate<'outer, 'inner, S> {
    pub(crate) query: Rows<'inner, S>,
    _p: PhantomData<&'inner &'outer ()>,
}

impl<'inner, S> Deref for Aggregate<'_, 'inner, S> {
    type Target = Rows<'inner, S>;

    fn deref(&self) -> &Self::Target {
        &self.query
    }
}

impl<S> DerefMut for Aggregate<'_, '_, S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.query
    }
}

impl<'outer, 'inner, S: 'static> Aggregate<'outer, 'inner, S> {
    /// This must be used with an aggregating expression.
    /// otherwise there is a change that there are multiple rows.
    fn select<T>(
        &self,
        expr: impl 'static + Fn(&mut ValueBuilder) -> sea_query::Expr,
    ) -> Aggr<S, Option<T>> {
        let expr = DynTypedExpr::new(expr);
        let mut builder = self.query.ast.clone().full();
        let (select, mut fields) = builder.build_select(vec![expr], Vec::new());

        let conds = builder.forwarded.into_iter().map(|(x, _)| x).collect();

        Aggr {
            _p2: PhantomData,
            select: Rc::new(select),
            field: {
                debug_assert_eq!(fields.len(), 1);
                fields.swap_remove(0)
            },
            conds,
        }
    }

    /// Return the average value in a column, this is [None] if there are zero rows.
    pub fn avg(&self, val: impl IntoExpr<'inner, S, Typ = f64>) -> Expr<'outer, S, Option<f64>> {
        let val = val.into_expr().inner;
        Expr::new(self.select(move |b| Func::avg(val.build_expr(b)).into()))
    }

    /// Return the maximum value in a column, this is [None] if there are zero rows.
    pub fn max<T>(&self, val: impl IntoExpr<'inner, S, Typ = T>) -> Expr<'outer, S, Option<T>>
    where
        T: EqTyp,
    {
        let val = val.into_expr().inner;
        Expr::new(self.select(move |b| Func::max(val.build_expr(b)).into()))
    }

    /// Return the minimum value in a column, this is [None] if there are zero rows.
    pub fn min<T>(&self, val: impl IntoExpr<'inner, S, Typ = T>) -> Expr<'outer, S, Option<T>>
    where
        T: EqTyp,
    {
        let val = val.into_expr().inner;
        Expr::new(self.select(move |b| Func::min(val.build_expr(b)).into()))
    }

    /// Return the sum of a column.
    pub fn sum<T>(&self, val: impl IntoExpr<'inner, S, Typ = T>) -> Expr<'outer, S, T>
    where
        T: NumTyp,
    {
        let val = val.into_expr().inner;
        let val = self.select::<T>(move |b| Func::sum(val.build_expr(b)).into());

        Expr::adhoc(move |b| {
            sea_query::Expr::expr(val.build_expr(b)).if_null(sea_query::Expr::Constant(T::ZERO))
        })
    }

    /// Return the number of distinct values in a column.
    pub fn count_distinct<T: EqTyp + 'static>(
        &self,
        val: impl IntoExpr<'inner, S, Typ = T>,
    ) -> Expr<'outer, S, i64> {
        let val = val.into_expr().inner;
        let val = self.select::<i64>(move |b| Func::count_distinct(val.build_expr(b)).into());
        Expr::adhoc(move |b| {
            sea_query::Expr::expr(val.build_expr(b)).if_null(sea_query::Expr::Constant(0i64.into()))
        })
    }

    /// Return whether there are any rows.
    pub fn exists(&self) -> Expr<'outer, S, bool> {
        let val = self.select::<i64>(|_| Func::count(sea_query::Expr::col(Asterisk)).into());
        Expr::adhoc(move |b| sea_query::Expr::expr(val.build_expr(b)).is_not_null())
    }
}

pub struct Aggr<S, T> {
    pub(crate) _p2: PhantomData<(S, T)>,
    pub(crate) select: Rc<SelectStatement>,
    pub(crate) conds: Vec<MyTableRef>,
    pub(crate) field: MyAlias,
}

impl<S, T> Clone for Aggr<S, T> {
    fn clone(&self) -> Self {
        Self {
            _p2: PhantomData,
            select: self.select.clone(),
            conds: self.conds.clone(),
            field: self.field,
        }
    }
}

impl<S, T: MyTyp> Typed for Aggr<S, T> {
    type Typ = T;
    fn build_expr(&self, b: &mut ValueBuilder) -> sea_query::Expr {
        sea_query::Expr::col((self.build_table(b), self.field)).into()
    }
}

impl<S, T> Aggr<S, T> {
    fn build_table(&self, b: &mut ValueBuilder) -> MyAlias {
        b.get_aggr(self.select.clone(), self.conds.clone())
    }
}

/// Perform an aggregate that returns a single result for each of the current rows.
///
/// You can filter the rows in the aggregate based on values from the outer query.
/// That is the only way to get a different aggregate for each outer row.
///
/// ```
/// # use rust_query::aggregate;
/// # use rust_query::private::doctest::*;
/// # rust_query::private::doctest::get_txn(|txn| {
/// let res = txn.query_one(aggregate(|rows| {
///     let user = rows.join(User);
///     rows.count_distinct(user)
/// }));
/// assert_eq!(res, 1, "there is one user in the database");
/// # });
/// ```
pub fn aggregate<'outer, S, F, R>(f: F) -> R
where
    F: for<'inner> FnOnce(&mut Aggregate<'outer, 'inner, S>) -> R,
{
    let inner = Rows {
        phantom: PhantomData,
        ast: Default::default(),
        _p: PhantomData,
    };
    let mut group = Aggregate {
        query: inner,
        _p: PhantomData,
    };
    f(&mut group)
}
