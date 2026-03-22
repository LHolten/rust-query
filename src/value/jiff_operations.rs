use sea_query::ExprTrait;

use crate::{Expr, IntoExpr};

impl<'column, S> Expr<'column, S, jiff::Timestamp> {
    /// Number of whole seconds since the unix epoch.
    ///
    /// Fractional seconds are truncated (matching jiff behaviour).
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// use jiff::Timestamp;
    /// assert_eq!(txn.query_one(Timestamp::from_millisecond(123713).unwrap().into_expr().as_second()), 123);
    /// assert_eq!(txn.query_one(Timestamp::from_millisecond(-123713).unwrap().into_expr().as_second()), -123);
    /// # });
    /// ```
    pub fn as_second(&self) -> Expr<'column, S, i64> {
        let this = self.inner.clone();
        Expr::adhoc(move |b| {
            sea_query::Func::cust("timestamp_as_second")
                .arg(this.build_expr(b))
                .into()
        })
    }

    /// New timestamp from seconds since the unix epoch.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// use jiff::Timestamp;
    /// use rust_query::Expr;
    /// assert_eq!(txn.query_one(Expr::from_second(4322)), Timestamp::from_second(4322).unwrap());
    /// assert_eq!(txn.query_one(Expr::from_second(-4322)), Timestamp::from_second(-4322).unwrap());
    /// # });
    /// ```
    pub fn from_second(val: impl IntoExpr<'column, S, Typ = i64>) -> Self {
        let this = val.into_expr().inner;
        Expr::adhoc(move |b| {
            sea_query::Func::cust("datetime")
                .arg(this.build_expr(b))
                .arg(sea_query::Expr::Constant(sea_query::Value::String(Some(
                    "unixepoch".to_owned(),
                ))))
                .into()
        })
    }

    /// The fractional component of the timestamp in seconds.
    ///
    /// Negative for timestamps before 1970 (matching jiff behaviour).
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// use jiff::Timestamp;
    /// assert_eq!(txn.query_one(Timestamp::from_millisecond(123713).unwrap().into_expr().subsec_nanosecond()), 713000000);
    /// assert_eq!(txn.query_one(Timestamp::from_millisecond(-123713).unwrap().into_expr().subsec_nanosecond()), -713000000);
    /// # });
    /// ```
    pub fn subsec_nanosecond(&self) -> Expr<'column, S, i64> {
        let this = self.inner.clone();
        Expr::adhoc(move |b| {
            sea_query::Func::cust("timestamp_subsec_nanosecond")
                .arg(this.build_expr(b))
                .into()
        })
    }

    /// Add a number of nanoseconds to a timestamp
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// use jiff::Timestamp;
    /// use rust_query::Expr;
    /// let ts = Timestamp::from_millisecond(123713).unwrap();
    /// assert_eq!(txn.query_one(Expr::from_second(ts.as_second()).add_nanosecond(ts.subsec_nanosecond() as i64)), ts);
    /// let ts = Timestamp::from_millisecond(-123713).unwrap();
    /// assert_eq!(txn.query_one(Expr::from_second(ts.as_second()).add_nanosecond(ts.subsec_nanosecond() as i64)), ts);
    /// # });
    /// ```
    pub fn add_nanosecond(&self, nanos: impl IntoExpr<'column, S, Typ = i64>) -> Self {
        let this = self.inner.clone();
        let nanos = nanos.into_expr().inner;
        Expr::adhoc(move |b| {
            sea_query::Func::cust("timestamp_add_nanos")
                .arg(this.build_expr(b))
                .arg(nanos.build_expr(b))
                .into()
        })
    }
}

impl<'column, S> Expr<'column, S, jiff::civil::Date> {
    /// The year in the range `0..=9999`.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// use jiff::civil::date;
    /// assert_eq!(txn.query_one(date(2300, 3, 6).into_expr().year()), 2300);
    /// # });
    /// ```
    pub fn year(&self) -> Expr<'column, S, i64> {
        let this = self.inner.clone();
        Expr::adhoc(move |b| {
            sea_query::Func::cust("strftime")
                .arg(sea_query::Expr::Constant(sea_query::Value::String(Some(
                    "%Y".to_owned(),
                ))))
                .arg(this.build_expr(b))
                .cast_as("INTEGER")
        })
    }

    /// The month of the year in the range `1..=12`.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// use jiff::civil::date;
    /// assert_eq!(txn.query_one(date(2300, 3, 6).into_expr().month()), 3);
    /// # });
    /// ```
    pub fn month(&self) -> Expr<'column, S, i64> {
        let this = self.inner.clone();
        Expr::adhoc(move |b| {
            sea_query::Func::cust("strftime")
                .arg(sea_query::Expr::Constant(sea_query::Value::String(Some(
                    "%m".to_owned(),
                ))))
                .arg(this.build_expr(b))
                .cast_as("INTEGER")
        })
    }

    /// The day of the month in the range `1..=31`.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// use jiff::civil::date;
    /// assert_eq!(txn.query_one(date(2300, 3, 6).into_expr().day()), 6);
    /// # });
    /// ```
    pub fn day(&self) -> Expr<'column, S, i64> {
        let this = self.inner.clone();
        Expr::adhoc(move |b| {
            sea_query::Func::cust("strftime")
                .arg(sea_query::Expr::Constant(sea_query::Value::String(Some(
                    "%d".to_owned(),
                ))))
                .arg(this.build_expr(b))
                .cast_as("INTEGER")
        })
    }

    /// Add a number of days to this date.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// use jiff::civil::date;
    /// assert_eq!(txn.query_one(date(2300, 3, 6).into_expr().add_day(30)), date(2300, 4, 5));
    /// # });
    /// ```
    pub fn add_day(&self, days: impl IntoExpr<'column, S, Typ = i64>) -> Self {
        let this = self.inner.clone();
        let days = days.into_expr().inner;
        Expr::adhoc(move |b| {
            sea_query::Func::cust("date")
                .arg(this.build_expr(b))
                .arg(days.build_expr(b).binary(
                    sea_query::BinOper::Custom("||"),
                    sea_query::Expr::Constant(sea_query::Value::String(Some(" day".to_owned()))),
                ))
                .into()
        })
    }

    pub fn date(
        year: impl IntoExpr<'column, S, Typ = i64>,
        month: impl IntoExpr<'column, S, Typ = i64>,
        day: impl IntoExpr<'column, S, Typ = i64>,
    ) -> Self {
        let year = year.into_expr().inner;
        let month = month.into_expr().inner;
        let day = day.into_expr().inner;

        todo!()
    }
}
