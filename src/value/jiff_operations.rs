use jiff::fmt::temporal;
use sea_query::ExprTrait;

use crate::{Expr, IntoExpr};

fn const_str(val: &str) -> sea_query::Value {
    sea_query::Value::String(Some(val.to_owned()))
}
fn concat(a: impl Into<sea_query::Expr>, b: impl Into<sea_query::Expr>) -> sea_query::Expr {
    a.into().binary(sea_query::BinOper::Custom("||"), b)
}

impl<'column, S> Expr<'column, S, jiff::Timestamp> {
    /// Number of whole seconds since the unix epoch.
    ///
    /// Fractional seconds are truncated (matching jiff behaviour).
    /// Before 1970 seconds are rounded up and after 1970 seconds are rounded down.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// use jiff::Timestamp;
    /// assert_eq!(txn.query_one(Timestamp::from_millisecond(123713).unwrap().into_expr().to_second()), 123);
    /// assert_eq!(txn.query_one(Timestamp::from_millisecond(-123713).unwrap().into_expr().to_second()), -123);
    /// # });
    /// ```
    pub fn to_second(&self) -> Expr<'column, S, i64> {
        let this = self.inner.clone();
        Expr::adhoc(move |b| {
            sea_query::Func::cust("timestamp_to_second")
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
                .arg(const_str("unixepoch"))
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
            sea_query::Func::cust("timestamp_add_nanosecond")
                .arg(this.build_expr(b))
                .arg(nanos.build_expr(b))
                .into()
        })
    }

    /// Convert a timestamp to a date using the specified timezone.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # use std::str::FromStr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// use jiff::Timestamp;
    /// use jiff::tz::TimeZone;
    /// use jiff::civil::Date;
    /// let ts = Timestamp::from_second(123713).unwrap();
    /// let tz = TimeZone::get("Europe/Amsterdam").unwrap();
    /// assert_eq!(txn.query_one(ts.into_expr().to_date_in_tz(&tz)), Date::from_str("1970-01-02").unwrap());
    /// # });
    /// ```
    pub fn to_date_in_tz(&self, tz: &jiff::tz::TimeZone) -> Expr<'column, S, jiff::civil::Date> {
        static PRINTER: temporal::DateTimePrinter = temporal::DateTimePrinter::new();
        let time_zone = PRINTER.time_zone_to_string(tz).unwrap();
        let this = self.inner.clone();

        Expr::adhoc(move |b| {
            sea_query::Func::cust("timestamp_to_date")
                .arg(this.build_expr(b))
                .arg(time_zone.clone())
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
                .arg(const_str("%Y"))
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
                .arg(const_str("%m"))
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
                .arg(const_str("%d"))
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
                .arg(concat(days.build_expr(b), const_str(" day")))
                .into()
        })
    }

    /// Give the first day of the month.
    ///
    /// - `idx == 0` the first day of this month.
    /// - `idx == 1` the first day of next month.
    /// - `idx == -1` the first day of previous month.
    /// - etc.
    ///
    /// ```
    /// # use rust_query::IntoExpr;
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// use jiff::civil::date;
    /// assert_eq!(txn.query_one(date(2300, 3, 6).into_expr().first_of_month(0)), date(2300, 3, 1));
    /// assert_eq!(txn.query_one(date(2300, 3, 6).into_expr().first_of_month(1)), date(2300, 4, 1));
    /// assert_eq!(txn.query_one(date(2300, 3, 6).into_expr().first_of_month(12)), date(2301, 3, 1));
    /// assert_eq!(txn.query_one(date(2300, 3, 6).into_expr().first_of_month(-1)), date(2300, 2, 1));
    /// # });
    /// ```
    pub fn first_of_month(&self, idx: impl IntoExpr<'column, S, Typ = i64>) -> Self {
        let this = self.inner.clone();
        let idx = idx.into_expr().inner;

        Expr::adhoc(move |b| {
            sea_query::Func::cust("date")
                .arg(this.build_expr(b))
                .arg(const_str("start of month"))
                .arg(concat(idx.build_expr(b), const_str(" month")))
                .into()
        })
    }
}
