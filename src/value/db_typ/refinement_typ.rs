use crate::{
    lower::list_writer::Alias,
    value::{BaseTyp, DbTyp, StorableTyp},
};

pub trait RefinementTyp: Clone + 'static {
    type RBase: BaseTyp;

    fn r_out_to_value(self) -> Self::RBase;
    fn r_from_sql(value: <Self::RBase as BaseTyp>::Ref<'_>) -> Self;

    fn r_check(_col: Alias) -> String;
}

impl<T: RefinementTyp> DbTyp for T {
    type Base = T::RBase;
    type Lazy<'t> = T;

    fn out_to_lazy<'t>(self) -> Self::Lazy<'t> {
        self
    }
    fn out_to_value(self) -> Self::Base {
        Self::r_out_to_value(self)
    }
    fn from_sql(value: <Self::Base as BaseTyp>::Ref<'_>) -> Self {
        Self::r_from_sql(value)
    }
}

impl<T: RefinementTyp> StorableTyp for T {
    type Prev = Self;
    type FromLazy<'x> = Self;

    fn check(col: &str) -> Option<String> {
        Some(Self::r_check(Alias(col)))
    }
    fn migrate(prev: Self::Prev) -> Self {
        prev
    }
    fn from_lazy(lazy: &Self::FromLazy<'_>) -> Self {
        lazy.clone()
    }
}

#[cfg(feature = "jiff-02")]
impl RefinementTyp for jiff::Timestamp {
    type RBase = String;

    fn r_out_to_value(self) -> Self::RBase {
        // check that year is positive
        assert!(self >= jiff::Timestamp::from_second(-62167219200).unwrap());
        // Use space instead of `T` for date and time separator
        self.strftime("%F %T%.f").to_string()
    }

    fn r_from_sql(value: <Self::RBase as BaseTyp>::Ref<'_>) -> Self {
        use std::str::FromStr;

        let dt = jiff::civil::DateTime::from_str(value).unwrap();
        jiff::tz::TimeZone::UTC.to_timestamp(dt).unwrap()
    }

    fn r_check(col: Alias) -> String {
        format!(
            "{col} IS (ltrim(datetime({col} || 'Z'), '-') || rtrim(substr({col}, 20, 10), '0 '))"
        )
    }
}

#[cfg(feature = "jiff-02")]
impl RefinementTyp for jiff::civil::Date {
    type RBase = String;

    fn r_out_to_value(self) -> Self::RBase {
        // check that year is positive
        assert!(
            self.year() >= 0,
            "only dates with a year that is greater than or equal to zero can be used"
        );
        self.to_string()
    }

    fn r_from_sql(value: <Self::RBase as BaseTyp>::Ref<'_>) -> Self {
        use std::str::FromStr;

        jiff::civil::Date::from_str(value).unwrap()
    }

    fn r_check(col: Alias) -> String {
        format!("{col} IS ltrim(date({col}), '-')")
    }
}

impl RefinementTyp for bool {
    type RBase = i64;
    fn r_out_to_value(self) -> Self::RBase {
        self.into()
    }
    fn r_from_sql(val: <Self::RBase as BaseTyp>::Ref<'_>) -> Self {
        val != 0
    }
    fn r_check(col: Alias) -> String {
        format!("{col} IN (0, 1)")
    }
}

#[test]
#[cfg(feature = "jiff-02")]
fn jiff_check_constraint() {
    use crate::{Database, migration::Config};

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
    use crate::{Database, migration::Config};

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
