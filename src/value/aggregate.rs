use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
    rc::Rc,
};

use crate::{
    Expr, IntoExpr, lower,
    rows::Rows,
    value::{EqTyp, NumTyp},
};

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
    fn select_func(&self, agg_func: &'static str, val: Rc<lower::Expr>) -> Rc<lower::Expr> {
        let expr = Rc::new(lower::Expr::Func(agg_func, Box::new([val])));
        // freezing the same rows multiple times should result in the same frozen rows
        // which are later deduplicated because frozen rows are used as a key.
        // TODO: maybe this can be made more efficient.
        Rc::new(lower::Expr::AggrIndex(self.ast.as_ref().clone(), expr))
    }

    /// Return the average value in a column, this is [None] if there are zero rows.
    ///
    /// ```
    /// # use rust_query::private::doctest_aggregate::*;
    /// # get_txn(|txn| {
    /// for x in [1, 2, 3] {
    ///     txn.insert_ok(Val { x });
    /// }
    /// let (avg1, avg2) = txn.query_one(aggregate(|rows| {
    ///     let val = rows.join(Val);
    ///     let avg1 = rows.avg(val.x.to_f64());
    ///     rows.filter(false); // remove all rows
    ///     let avg2 = rows.avg(val.x.to_f64());
    ///     (avg1, avg2)
    /// }));
    /// assert_eq!(avg1, Some(2.0));
    /// assert_eq!(avg2, None);
    /// # });
    /// ```
    pub fn avg(&self, val: impl IntoExpr<'inner, S, Typ = f64>) -> Expr<'outer, S, Option<f64>> {
        let val = val.into_expr().inner;
        Expr::new(self.select_func("avg", val))
    }

    /// Return the maximum value in a column, this is [None] if there are zero rows.
    ///
    /// ```
    /// # use rust_query::private::doctest_aggregate::*;
    /// # get_txn(|txn| {
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
        Expr::new(self.select_func("max", val))
    }

    /// Return the minimum value in a column, this is [None] if there are zero rows.
    ///
    /// ```
    /// # use rust_query::private::doctest_aggregate::*;
    /// # get_txn(|txn| {
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
        Expr::new(self.select_func("min", val))
    }

    /// Return the sum of a column.
    ///
    /// ```
    /// # use rust_query::private::doctest_aggregate::*;
    /// # get_txn(|txn| {
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
        let val = self.select_func("sum", val);

        Expr::adhoc(lower::Expr::Func(
            "IFNULL",
            Box::new([val, Rc::new(lower::Expr::Constant(T::ZERO))]),
        ))
    }

    /// Return the number of distinct values in a column.
    ///
    /// ```
    /// # use rust_query::private::doctest_aggregate::*;
    /// # get_txn(|txn| {
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
        let val = self.select_func("COUNT", Rc::new(lower::Expr::Prefix("DISTINCT ", val)));
        // technically the `if_null` here is only required for correlated sub queries
        Expr::adhoc(lower::Expr::Func(
            "IFNULL",
            Box::new([val, Rc::new(lower::Expr::Constant(i64::ZERO))]),
        ))
    }

    /// Return whether there are any rows.
    ///
    /// ```
    /// # use rust_query::private::doctest_aggregate::*;
    /// # get_txn(|txn| {
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
        let zero_expr = Expr::<_, i64>::adhoc(lower::CONST_0);
        self.count_distinct(zero_expr.clone()).neq(zero_expr)
    }
}

/// Perform an aggregate that returns a single result for each of the current rows.
///
/// One can filter the rows in the aggregate based on values from the outer query.
/// See the documentation for [Aggregate] for more information.
///
/// ```
/// # use rust_query::migration::{schema, Config};
/// # use rust_query::{Database, aggregate};
/// #[schema(Site)]
/// pub mod vN {
///     pub struct Review {
///         #[index]
///         pub book: rust_query::TableRow<Book>,
///         pub rating: f64,
///     }
///     pub struct Book {
///         pub name: String
///     }
/// }
/// use v0::*;
///
/// Database::new(Config::open_in_memory()).transaction(|txn| {
///     let books = txn.query(|rows| {
///         let book = rows.join(Book);
///         let rating = aggregate(|aggr| {
///             let review = aggr.join(Review.book(&book));
///             // books without reviews will get a rating of 0.0
///             aggr.avg(&review.rating).unwrap_or(0.0)
///         });
///         // top 10 highest rated books
///         rows.order_by()
///             .desc(rating)
///             .into_iter(book)
///             .take(10)
///     });
/// });
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
