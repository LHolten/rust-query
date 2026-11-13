#[doc(hidden)]
#[macro_export]
macro_rules! refinement_typ {
    ($(#[$attr:meta])* $typ:ty, $base:ty, $out:expr, $from:expr, $check:expr $(,)?) => {
        impl $crate::private::DbTyp for $typ {
            type Base = $base;
            type Lazy<'t> = Self;

            fn out_to_lazy<'t>(self) -> Self::Lazy<'t> {
                self
            }
            fn out_to_value(self) -> Self::Base {
                let f: fn(Self) -> Self::Base = $out;
                f(self)
            }
            fn from_sql(val: <Self::Base as $crate::private::BaseTyp>::Ref<'_>) -> Self {
                let f: fn(<Self::Base as $crate::private::BaseTyp>::Ref<'_>) -> Self = $from;
                f(val)
            }
        }

        impl $crate::private::StorableTyp for $typ {
            type Prev = Self;

            fn check(col: &str) -> Option<String> {
                let f: fn($crate::private::Alias) -> String = $check;
                Some(f($crate::private::Alias(col)))
            }
            fn migrate(prev: Self::Prev) -> Self {
                prev
            }
            fn from_lazy(lazy: &Self) -> Self {
                lazy.clone()
            }
        }

        $(#[$attr])*
        impl<'column, S> $crate::IntoExpr<'column, S> for $typ {
            type Typ = Self;

            fn into_expr(&self) -> $crate::Expr<'column, S, Self::Typ> {
                $crate::private::db_typ_to_expr(self.clone())
            }
        }
    };
}

#[cfg(feature = "jiff-02")]
refinement_typ!(
    /// Note that timestamps before `0000-01-01 00:00:00` can not be used in an expression.
    /// The reason is that datetimes are represented in sqlite as strings and for negative
    /// years the sorting order is wrong.
    ///
    /// The conversion is lossless, everything up to nanoseconds is preserved.
    jiff::Timestamp,
    String,
    |ts| {
        // check that year is positive
        assert!(ts >= jiff::Timestamp::from_second(-62167219200).unwrap());
        // Use space instead of `T` for date and time separator
        ts.strftime("%F %T%.f").to_string()
    },
    |value| {
        use std::str::FromStr;

        let dt = jiff::civil::DateTime::from_str(value).unwrap();
        jiff::tz::TimeZone::UTC.to_timestamp(dt).unwrap()
    },
    |col| format!(
        "{col} IS (ltrim(datetime({col} || 'Z'), '-') || rtrim(substr({col}, 20, 10), '0 '))"
    ),
);

#[cfg(feature = "jiff-02")]
refinement_typ!(
    /// Note that dates before `0000-01-01` can not be used in an expression.
    /// The reason is that dates are represented in sqlite as strings and for negative
    /// years the sorting order is wrong.
    jiff::civil::Date,
    String,
    |date| {
        // check that year is positive
        assert!(
            date.year() >= 0,
            "only dates with a year that is greater than or equal to zero can be used"
        );
        date.to_string()
    },
    |value| {
        use std::str::FromStr;

        jiff::civil::Date::from_str(value).unwrap()
    },
    |col| format!("{col} IS ltrim(date({col}), '-')"),
);

refinement_typ!(bool, i64, |val| val.into(), |val| val != 0, |col| format!(
    "{col} IN (0, 1)"
),);

#[test]
#[cfg(feature = "jiff-02")]
fn jiff_check_constraint() {
    use crate::{Database, migration::Config, value::DbTyp};

    #[crate::migration::schema(Schema)]
    pub mod vN {
        pub struct Thing {
            pub created_at: jiff::Timestamp,
        }
    }
    use v0::*;

    let db = Database::<Schema>::new(Config::open_in_memory());
    let mut conn = db.rusqlite_connection();
    let txn = conn.transaction().unwrap();

    let good = [
        "2000-01-01 10:20:30",
        "2000-01-01 10:20:31",
        "2000-01-01 10:20:31.1",
        "2000-01-01 10:20:31.000000001",
    ];

    let bad = [
        "2000-01-01 10:20:30Z",
        "2000-01-01 10:20:31Z",
        "2000-01-01 10:20:31+00:01",
        "2000-01-01 10:20:31+00:00",
        "2000-01-01 10:20:31 ",
        "-2000-01-01 10:20:30",
        "-2000-01-01 10:20:31",
        "2000-01-01 10:20:30.",
        "2000-01-01 10:20:30.0",
        "2000-01-01 10:20:30. 1",
        "2000-01-01 10:20:30.10",
        "2000-01-01 10:20:31.0000000001", // sub-nanosecond
    ];

    for good in good {
        txn.execute("INSERT INTO thing (created_at) VALUES ($1)", [good])
            .expect(&format!("{good}\n"));

        let ts = jiff::Timestamp::from_sql(good);
        assert_eq!(ts.out_to_value(), good.to_owned())
    }

    for bad in bad {
        let err = txn
            .execute("INSERT INTO thing (created_at) VALUES ($1)", [bad])
            .expect_err(&format!("{bad}"));
        assert_eq!(
            err.sqlite_error().unwrap().extended_code,
            rusqlite::ffi::SQLITE_CONSTRAINT_CHECK
        );
    }

    txn.commit().unwrap();
}

#[test]
#[cfg(feature = "jiff-02")]
fn jiff_check_constraint_date() {
    use crate::{Database, migration::Config, value::DbTyp};

    #[crate::migration::schema(Schema)]
    pub mod vN {
        pub struct Thing {
            pub created_at: jiff::civil::Date,
        }
    }
    use v0::*;

    let db = Database::<Schema>::new(Config::open_in_memory());
    let mut conn = db.rusqlite_connection();
    let txn = conn.transaction().unwrap();

    let good = ["2000-01-01", "0000-01-30", "9999-12-31"];

    let bad = [
        "2000-01-01 10:20:30",
        "2000-01-01 ",
        "2000-01-01Z",
        "2000-01-01+00:01",
        "2000-01-01+00:00",
        "-2000-01-01",
    ];

    for good in good {
        txn.execute("INSERT INTO thing (created_at) VALUES ($1)", [good])
            .expect(&format!("{good}\n"));

        let ts = jiff::civil::Date::from_sql(good);
        assert_eq!(ts.out_to_value(), good.to_owned())
    }

    for bad in bad {
        let err = txn
            .execute("INSERT INTO thing (created_at) VALUES ($1)", [bad])
            .expect_err(&format!("{bad}"));
        assert_eq!(
            err.sqlite_error().unwrap().extended_code,
            rusqlite::ffi::SQLITE_CONSTRAINT_CHECK
        );
    }

    txn.commit().unwrap();
}
