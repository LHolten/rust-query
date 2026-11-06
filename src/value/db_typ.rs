use std::{cell::OnceCell, marker::PhantomData};

use crate::{
    Table, TableRow, Transaction, db::TableRowInner, lower::list_writer::Alias, schema::canonical,
    value::EqTyp,
};

/// The types that can be used inside [crate::Expr].
/// Some stuff like nested [Option] is not allowed.
pub trait DbTyp: Sized + 'static {
    type Base: BaseTypOpt;

    type Ext<'t>;

    type FromLazy<'x>;
    type Lazy<'t>: Sized;

    fn from_lazy(lazy: &Self::FromLazy<'_>) -> Self;
    fn out_to_value(self) -> rusqlite::types::Value;
    fn out_to_lazy<'t>(self) -> Self::Lazy<'t>;

    fn from_sql(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self>
    where
        Self: Sized;
}

/// Not all types are allowed to be stored.
/// Specificially `#[no_reference]` references.
pub trait StorableTyp: DbTyp {
    const TYP: canonical::ColumnType;
    const FK: Option<(&'static str, &'static str)> = None;

    fn check(_col: &str) -> Option<String>;

    type Prev;
    fn migrate(prev: Self::Prev) -> Self;
}

pub trait BaseTyp {}
impl BaseTyp for i64 {}
impl BaseTyp for f64 {}
impl BaseTyp for String {}
impl BaseTyp for Vec<u8> {}

pub trait BaseTypOpt {
    const NULLABLE: bool;
}
impl<T: BaseTyp> BaseTypOpt for T {
    const NULLABLE: bool = false;
}
impl<T: BaseTyp> BaseTypOpt for Option<T> {
    const NULLABLE: bool = true;
}

#[cfg(feature = "jiff-02")]
impl DbTyp for jiff::Timestamp {
    type Base = String;
    type Ext<'t> = ();
    type FromLazy<'x> = Self;
    type Lazy<'t> = Self;

    fn from_lazy(lazy: &Self::FromLazy<'_>) -> Self {
        *lazy
    }
    fn out_to_value(self) -> rusqlite::types::Value {
        // check that year is positive
        assert!(self >= jiff::Timestamp::from_second(-62167219200).unwrap());
        // Use space instead of `T` for date and time separator
        rusqlite::types::Value::Text(self.strftime("%F %T%.f").to_string())
    }

    fn out_to_lazy<'t>(self) -> Self::Lazy<'t> {
        self
    }

    fn from_sql(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self>
    where
        Self: Sized,
    {
        use rusqlite::types::FromSqlError;
        use std::str::FromStr;

        let dt = jiff::civil::DateTime::from_str(value.as_str()?).map_err(FromSqlError::other)?;
        jiff::tz::TimeZone::UTC
            .to_timestamp(dt)
            .map_err(FromSqlError::other)
    }
}

#[cfg(feature = "jiff-02")]
impl StorableTyp for jiff::Timestamp {
    const TYP: canonical::ColumnType = canonical::ColumnType::Text;
    fn check(col: &str) -> Option<String> {
        Some(format!(
            "{0} IS (ltrim(datetime({0} || 'Z'), '-') || rtrim(substr({0}, 20, 10), '0 '))",
            Alias(col)
        ))
    }

    type Prev = Self;
    fn migrate(prev: Self::Prev) -> Self {
        prev
    }
}

#[cfg(feature = "jiff-02")]
impl DbTyp for jiff::civil::Date {
    type Base = String;
    type Ext<'t> = ();
    type FromLazy<'x> = Self;
    type Lazy<'t> = Self;

    fn from_lazy(lazy: &Self::FromLazy<'_>) -> Self {
        *lazy
    }
    fn out_to_value(self) -> rusqlite::types::Value {
        // check that year is positive
        assert!(
            self.year() >= 0,
            "only dates with a year that is greater than or equal to zero can be used"
        );
        rusqlite::types::Value::Text(self.to_string())
    }

    fn out_to_lazy<'t>(self) -> Self::Lazy<'t> {
        self
    }

    fn from_sql(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self>
    where
        Self: Sized,
    {
        use rusqlite::types::FromSqlError;
        use std::str::FromStr;

        jiff::civil::Date::from_str(value.as_str()?).map_err(FromSqlError::other)
    }
}

#[cfg(feature = "jiff-02")]
impl StorableTyp for jiff::civil::Date {
    const TYP: canonical::ColumnType = canonical::ColumnType::Text;
    fn check(col: &str) -> Option<String> {
        Some(format!("{0} IS ltrim(date({0}), '-')", Alias(col)))
    }

    type Prev = Self;
    fn migrate(prev: Self::Prev) -> Self {
        prev
    }
}

impl<T: Table> DbTyp for TableRow<T> {
    type Base = i64;
    type Ext<'t> = T::Ext2<'t>;

    type FromLazy<'x> = crate::Lazy<'x, <T as crate::Table>::MigrateFrom>;
    fn from_lazy(lazy: &Self::FromLazy<'_>) -> Self {
        TableRow::migrate_row(lazy.table_row())
    }
    fn out_to_value(self) -> rusqlite::types::Value {
        rusqlite::types::Value::Integer(self.inner.idx)
    }
    type Lazy<'t> = crate::Lazy<'t, T>;
    fn out_to_lazy<'t>(self) -> Self::Lazy<'t> {
        crate::Lazy {
            id: self,
            lazy: OnceCell::new(),
            txn: Transaction::new_ref(),
        }
    }

    fn from_sql(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        Ok(TableRow {
            _local: PhantomData,
            inner: TableRowInner {
                _p: PhantomData,
                idx: value.as_i64()?,
            },
        })
    }
}

impl<T: Table<Referer = ()>> StorableTyp for TableRow<T> {
    const TYP: canonical::ColumnType = canonical::ColumnType::Integer;
    const FK: Option<(&'static str, &'static str)> = Some((T::NAME, T::ID));

    fn check(_col: &str) -> Option<String> {
        None
    }

    type Prev = TableRow<T::MigrateFrom>;
    fn migrate(prev: Self::Prev) -> Self {
        TableRow::migrate_row(prev)
    }
}

impl DbTyp for bool {
    type Base = i64;
    type Ext<'t> = ();
    type FromLazy<'x> = Self;
    fn from_lazy(lazy: &Self::FromLazy<'_>) -> Self {
        lazy.clone()
    }
    fn out_to_value(self) -> rusqlite::types::Value {
        self.into()
    }
    type Lazy<'t> = Self;
    fn out_to_lazy<'t>(self) -> Self::Lazy<'t> {
        self
    }
    fn from_sql(val: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        val.as_i64().map(|x| x != 0)
    }
}
impl StorableTyp for bool {
    const TYP: canonical::ColumnType = (canonical::ColumnType::Integer);
    fn check(col: &str) -> Option<String> {
        Some(format!("{} IN (0, 1)", Alias(col)))
    }
    type Prev = Self;
    fn migrate(prev: Self) -> Self {
        prev
    }
}

impl<T: EqTyp> DbTyp for Option<T> {
    type Base = Option<T::Base>;
    type Ext<'t> = ();

    type FromLazy<'x> = Option<T::FromLazy<'x>>;
    fn from_lazy(lazy: &Self::FromLazy<'_>) -> Self {
        lazy.as_ref().map(T::from_lazy)
    }
    fn out_to_value(self) -> rusqlite::types::Value {
        self.map(T::out_to_value)
            .unwrap_or(rusqlite::types::Value::Null)
    }
    type Lazy<'t> = Option<T::Lazy<'t>>;
    fn out_to_lazy<'t>(self) -> Self::Lazy<'t> {
        self.map(T::out_to_lazy)
    }

    fn from_sql(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        if value.data_type() == rusqlite::types::Type::Null {
            Ok(None)
        } else {
            Ok(Some(T::from_sql(value)?))
        }
    }
}

impl<T: EqTyp + StorableTyp> StorableTyp for Option<T> {
    const TYP: canonical::ColumnType = T::TYP;
    const FK: Option<(&'static str, &'static str)> = T::FK;

    fn check(col: &str) -> Option<String> {
        T::check(col)
    }

    type Prev = Option<T::Prev>;
    fn migrate(prev: Self::Prev) -> Self {
        prev.map(T::migrate)
    }
}

macro_rules! impl_typ {
    ($typ:ty, $can:expr, $map:expr) => {
        impl_typ!($typ, $can, $map, |_col| None);
    };
    ($typ:ty, $can:expr, $map:expr, $check:expr) => {
        impl DbTyp for $typ {
            type Base = $typ;
            type Ext<'t> = ();
            type FromLazy<'x> = Self;

            fn from_lazy(lazy: &Self::FromLazy<'_>) -> Self {
                lazy.clone()
            }
            fn out_to_value(self) -> rusqlite::types::Value {
                self.into()
            }
            type Lazy<'t> = Self;
            fn out_to_lazy<'t>(self) -> Self::Lazy<'t> {
                self
            }
            fn from_sql(
                val: rusqlite::types::ValueRef<'_>,
            ) -> rusqlite::types::FromSqlResult<Self> {
                let f: fn(rusqlite::types::ValueRef<'_>) -> _ = $map;
                f(val)
            }
        }

        impl StorableTyp for $typ {
            const TYP: canonical::ColumnType = $can;

            fn check(col: &str) -> Option<String> {
                let f: fn(col: &str) -> _ = $check;
                return f(col);
            }

            type Prev = Self;
            fn migrate(prev: Self) -> Self {
                prev
            }
        }
    };
}
impl_typ!(i64, canonical::ColumnType::Integer, |x| x.as_i64());
impl_typ!(String, canonical::ColumnType::Text, |x| x
    .as_str()
    .map(ToOwned::to_owned));

impl_typ!(Vec<u8>, canonical::ColumnType::Blob, |x| x
    .as_blob()
    .map(ToOwned::to_owned));
impl_typ!(f64, canonical::ColumnType::Real, |x| x.as_f64());

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

        let ts =
            jiff::Timestamp::from_sql(rusqlite::types::ValueRef::Text(good.as_bytes())).unwrap();
        assert_eq!(
            ts.out_to_value(),
            rusqlite::types::Value::Text(good.to_owned())
        )
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

        let ts =
            jiff::civil::Date::from_sql(rusqlite::types::ValueRef::Text(good.as_bytes())).unwrap();
        assert_eq!(
            ts.out_to_value(),
            rusqlite::types::Value::Text(good.to_owned())
        )
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
