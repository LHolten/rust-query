use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
    rc::Rc,
};

use sea_query::Func;

use crate::{
    Expr, IntoExpr,
    ast::CONST_0,
    rows::Rows,
    value::{AdHoc, EqTyp, NumTyp, ValueBuilder},
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
    /// otherwise there is a chance that there are multiple rows.
    fn select<T: EqTyp>(
        &self,
        expr: impl 'static + Fn(&mut ValueBuilder) -> sea_query::Expr,
    ) -> Rc<AdHoc<dyn Fn(&mut ValueBuilder) -> sea_query::Expr, Option<T>>> {
        let expr = DynTypedExpr::new(expr);
        let mut builder = self.query.ast.clone().full();
        let (select, mut fields) = builder.build_select(vec![expr], Vec::new());

        let conds: Vec<_> = builder.forwarded.into_iter().map(|(x, _)| x).collect();

        let select = Rc::new(select);
        let field = {
            debug_assert_eq!(fields.len(), 1);
            fields.swap_remove(0)
        };

        Expr::<S, _>::adhoc(move |b| {
            sea_query::Expr::col((b.get_aggr(select.clone(), conds.clone()), field))
        })
        .inner
    }

    /// Return the average value in a column, this is [None] if there are zero rows.
    ///
    /// ```
    /// # #[rust_query::migration::schema(M)]
    /// # pub mod vN {
    /// #     pub struct Val {
    /// #         pub x: f64,
    /// #     }
    /// # }
    /// # use v0::*;
    /// # use rust_query::aggregate;
    /// # rust_query::Database::new(rust_query::migration::Config::open_in_memory()).transaction_mut_ok(|txn| {
    /// for x in [1.0, 2.0, 3.0] {
    ///     txn.insert_ok(Val { x });
    /// }
    /// let (avg1, avg2) = txn.query_one(aggregate(|rows| {
    ///     let val = rows.join(Val);
    ///     let avg1 = rows.avg(&val.x);
    ///     rows.filter(false); // remove all rows
    ///     let avg2 = rows.avg(&val.x);
    ///     (avg1, avg2)
    /// }));
    /// assert_eq!(avg1, Some(2.0));
    /// assert_eq!(avg2, None);
    /// # });
    /// ```
    pub fn avg(&self, val: impl IntoExpr<'inner, S, Typ = f64>) -> Expr<'outer, S, Option<f64>> {
        let val = val.into_expr().inner;
        Expr::new(self.select(move |b| Func::avg(val.build_expr(b)).into()))
    }

    /// Return the maximum value in a column, this is [None] if there are zero rows.
    ///
    /// ```
    /// # #[rust_query::migration::schema(M)]
    /// # pub mod vN {
    /// #     pub struct Val {
    /// #         pub x: i64,
    /// #     }
    /// # }
    /// # use v0::*;
    /// # use rust_query::aggregate;
    /// # rust_query::Database::new(rust_query::migration::Config::open_in_memory()).transaction_mut_ok(|txn| {
    /// for x in [-100, 10, 42] {
    ///     txn.insert_ok(Val { x });
    /// }
    /// let (max1, max2) = txn.query_one(aggregate(|rows| {
    ///     let val = rows.join(Val);
    ///     let max1 = rows.max(&val.x);
    ///     rows.filter(false); // remove all rows
    ///     let max2 = rows.max(&val.x);
    ///     (max1, max2)
    /// }));
    /// assert_eq!(max1, Some(42));
    /// assert_eq!(max2, None);
    /// # });
    /// ```
    pub fn max<T>(&self, val: impl IntoExpr<'inner, S, Typ = T>) -> Expr<'outer, S, Option<T>>
    where
        T: EqTyp,
    {
        let val = val.into_expr().inner;
        Expr::new(self.select(move |b| Func::max(val.build_expr(b)).into()))
    }

    /// Return the minimum value in a column, this is [None] if there are zero rows.
    ///
    /// ```
    /// # #[rust_query::migration::schema(M)]
    /// # pub mod vN {
    /// #     pub struct Val {
    /// #         pub x: i64,
    /// #     }
    /// # }
    /// # use v0::*;
    /// # use rust_query::aggregate;
    /// # rust_query::Database::new(rust_query::migration::Config::open_in_memory()).transaction_mut_ok(|txn| {
    /// for x in [-100, 10, 42] {
    ///     txn.insert_ok(Val { x });
    /// }
    /// let (min1, min2) = txn.query_one(aggregate(|rows| {
    ///     let val = rows.join(Val);
    ///     let min1 = rows.min(&val.x);
    ///     rows.filter(false); // remove all rows
    ///     let min2 = rows.min(&val.x);
    ///     (min1, min2)
    /// }));
    /// assert_eq!(min1, Some(-100));
    /// assert_eq!(min2, None);
    /// # });
    /// ```
    pub fn min<T>(&self, val: impl IntoExpr<'inner, S, Typ = T>) -> Expr<'outer, S, Option<T>>
    where
        T: EqTyp,
    {
        let val = val.into_expr().inner;
        Expr::new(self.select(move |b| Func::min(val.build_expr(b)).into()))
    }

    /// Return the sum of a column.
    ///
    /// ```
    /// # #[rust_query::migration::schema(M)]
    /// # pub mod vN {
    /// #     pub struct Val {
    /// #         pub x: i64,
    /// #     }
    /// # }
    /// # use v0::*;
    /// # use rust_query::aggregate;
    /// # rust_query::Database::new(rust_query::migration::Config::open_in_memory()).transaction_mut_ok(|txn| {
    /// for x in [-100, 10, 42] {
    ///     txn.insert_ok(Val { x });
    /// }
    /// let (sum1, sum2) = txn.query_one(aggregate(|rows| {
    ///     let val = rows.join(Val);
    ///     let sum1 = rows.sum(&val.x);
    ///     rows.filter(false); // remove all rows
    ///     let sum2 = rows.sum(&val.x);
    ///     (sum1, sum2)
    /// }));
    /// assert_eq!(sum1, -48);
    /// assert_eq!(sum2, 0);
    /// # });
    /// ```
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
    ///
    /// ```
    /// # #[rust_query::migration::schema(M)]
    /// # pub mod vN {
    /// #     pub struct Val {
    /// #         pub x: i64,
    /// #     }
    /// # }
    /// # use v0::*;
    /// # use rust_query::aggregate;
    /// # rust_query::Database::new(rust_query::migration::Config::open_in_memory()).transaction_mut_ok(|txn| {
    /// for x in [-100, 10, 42, 10] {
    ///     txn.insert_ok(Val { x });
    /// }
    /// let (count1, count2) = txn.query_one(aggregate(|rows| {
    ///     let val = rows.join(Val);
    ///     let count1 = rows.count_distinct(&val.x);
    ///     rows.filter(false); // remove all rows
    ///     let count2 = rows.count_distinct(&val.x);
    ///     (count1, count2)
    /// }));
    /// assert_eq!(count1, 3);
    /// assert_eq!(count2, 0);
    /// # });
    /// ```
    pub fn count_distinct<T: EqTyp + 'static>(
        &self,
        val: impl IntoExpr<'inner, S, Typ = T>,
    ) -> Expr<'outer, S, i64> {
        let val = val.into_expr().inner;
        let val = self.select::<i64>(move |b| Func::count_distinct(val.build_expr(b)).into());
        Expr::adhoc(move |b| {
            // technically the `if_null` here is only required for correlated sub queries
            sea_query::Expr::expr(val.build_expr(b)).if_null(sea_query::Expr::Constant(0i64.into()))
        })
    }

    /// Return whether there are any rows.
    ///
    /// ```
    /// # #[rust_query::migration::schema(M)]
    /// # pub mod vN {
    /// #     pub struct Val {
    /// #         pub x: i64,
    /// #     }
    /// # }
    /// # use v0::*;
    /// # use rust_query::aggregate;
    /// # rust_query::Database::new(rust_query::migration::Config::open_in_memory()).transaction_mut_ok(|txn| {
    /// for x in [10, 42, 10] {
    ///     txn.insert_ok(Val { x });
    /// }
    /// let (e1, e2) = txn.query_one(aggregate(|rows| {
    ///     rows.join(Val);
    ///     let e1 = rows.exists();
    ///     rows.filter(false); // removes all rows
    ///     let e2 = rows.exists();
    ///     (e1, e2)
    /// }));
    /// assert_eq!(e1, true);
    /// assert_eq!(e2, false);
    /// # });
    /// ```
    pub fn exists(&self) -> Expr<'outer, S, bool> {
        let zero_expr = Expr::<_, i64>::adhoc(|_| CONST_0);
        self.count_distinct(zero_expr.clone()).neq(zero_expr)
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
