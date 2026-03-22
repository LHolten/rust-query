use std::{cell::OnceCell, marker::PhantomData};

use sea_query::{ExprTrait, Nullable};

use crate::{
    Table, TableRow, Transaction,
    ast::{CONST_0, CONST_1},
    db::TableRowInner,
    schema::canonical,
    value::EqTyp,
};

/// The types that can be used inside [crate::Expr].
/// Some stuff like nested [Option] is not allowed.
pub trait DbTyp: Sized + 'static {
    type Prev;
    const NULLABLE: bool = false;
    const TYP: canonical::ColumnType;
    const FK: Option<(&'static str, &'static str)> = None;
    type Ext<'t>;
    type Sql: Nullable;

    type FromLazy<'x>;
    type Lazy<'t>: Sized;

    fn migrate(prev: Self::Prev) -> Self;
    fn from_lazy(lazy: &Self::FromLazy<'_>) -> Self;
    fn out_to_value(self) -> sea_query::Value;
    fn out_to_lazy<'t>(self) -> Self::Lazy<'t>;

    fn from_sql(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self>
    where
        Self: Sized;
}

/// Not all types are allowed to be stored.
/// Specificially `#[no_reference]` references.
pub trait StorableTyp: DbTyp {
    fn check(_col: sea_query::Alias) -> Option<sea_query::SimpleExpr>;
}

#[cfg(feature = "jiff-02")]
impl DbTyp for jiff::Timestamp {
    type Prev = Self;
    const TYP: canonical::ColumnType = canonical::ColumnType::Text;
    type Ext<'t> = ();
    type Sql = String;
    type FromLazy<'x> = Self;
    type Lazy<'t> = Self;

    fn migrate(prev: Self::Prev) -> Self {
        prev
    }
    fn from_lazy(lazy: &Self::FromLazy<'_>) -> Self {
        *lazy
    }
    fn out_to_value(self) -> sea_query::Value {
        // check that year is positive
        assert!(self >= jiff::Timestamp::from_second(-62167219200).unwrap());
        // Use space instead of `T` for date and time separator
        sea_query::Value::String(Some(self.strftime("%F %T%.f").to_string()))
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
    fn check(col: sea_query::Alias) -> Option<sea_query::SimpleExpr> {
        const CONCAT: sea_query::BinOper = sea_query::BinOper::Custom("||");
        let datetime =
            sea_query::Func::cust("datetime").arg(sea_query::Expr::col(col.clone()).binary(
                CONCAT,
                sea_query::Expr::Constant(sea_query::Value::String(Some("Z".to_owned()))),
            ));
        let ltrim = sea_query::Func::cust("ltrim")
            .arg(datetime)
            .arg(sea_query::Expr::Constant(sea_query::Value::String(Some(
                "-".to_owned(),
            ))));
        let substr = sea_query::Func::cust("substr")
            .arg(sea_query::Expr::col(col.clone()))
            .arg(sea_query::Expr::Constant(sea_query::Value::BigInt(Some(
                20,
            ))))
            .arg(sea_query::Expr::Constant(sea_query::Value::BigInt(Some(
                10,
            ))));
        let rtrim = sea_query::Func::cust("rtrim")
            .arg(substr)
            .arg(sea_query::Expr::Constant(sea_query::Value::String(Some(
                "0 ".to_owned(),
            ))));
        let concat = sea_query::Expr::from(ltrim).binary(CONCAT, rtrim);
        Some(sea_query::Expr::col(col).is(concat))
    }
}

#[cfg(feature = "jiff-02")]
impl DbTyp for jiff::civil::Date {
    type Prev = Self;
    const TYP: canonical::ColumnType = canonical::ColumnType::Text;
    type Ext<'t> = ();
    type Sql = String;
    type FromLazy<'x> = Self;
    type Lazy<'t> = Self;

    fn migrate(prev: Self::Prev) -> Self {
        prev
    }
    fn from_lazy(lazy: &Self::FromLazy<'_>) -> Self {
        *lazy
    }
    fn out_to_value(self) -> sea_query::Value {
        // check that year is positive
        assert!(
            self.year() >= 0,
            "only dates with a year that is greater than or equal to zero can be used"
        );
        sea_query::Value::String(Some(self.to_string()))
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
    fn check(col: sea_query::Alias) -> Option<sea_query::SimpleExpr> {
        let datetime = sea_query::Func::cust("date").arg(sea_query::Expr::col(col.clone()));
        let ltrim = sea_query::Func::cust("ltrim")
            .arg(datetime)
            .arg(sea_query::Expr::Constant(sea_query::Value::String(Some(
                "-".to_owned(),
            ))));
        Some(sea_query::Expr::col(col).is(ltrim))
    }
}

impl<T: Table> DbTyp for TableRow<T> {
    type Prev = TableRow<T::MigrateFrom>;
    const TYP: canonical::ColumnType = canonical::ColumnType::Integer;
    const FK: Option<(&'static str, &'static str)> = Some((T::NAME, T::ID));
    type Ext<'t> = T::Ext2<'t>;
    type Sql = i64;

    type FromLazy<'x> = crate::Lazy<'x, <T as crate::Table>::MigrateFrom>;
    fn migrate(prev: Self::Prev) -> Self {
        TableRow::migrate_row(prev)
    }
    fn from_lazy(lazy: &Self::FromLazy<'_>) -> Self {
        TableRow::migrate_row(lazy.table_row())
    }
    fn out_to_value(self) -> sea_query::Value {
        self.inner.idx.into()
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
    fn check(_col: sea_query::Alias) -> Option<sea_query::SimpleExpr> {
        None
    }
}

impl<T: EqTyp> DbTyp for Option<T> {
    type Prev = Option<T::Prev>;
    const TYP: canonical::ColumnType = T::TYP;
    const NULLABLE: bool = true;
    const FK: Option<(&'static str, &'static str)> = T::FK;
    type Ext<'t> = ();
    type Sql = T::Sql;

    type FromLazy<'x> = Option<T::FromLazy<'x>>;
    fn migrate(prev: Self::Prev) -> Self {
        prev.map(T::migrate)
    }
    fn from_lazy(lazy: &Self::FromLazy<'_>) -> Self {
        lazy.as_ref().map(T::from_lazy)
    }
    fn out_to_value(self) -> sea_query::Value {
        self.map(T::out_to_value)
            .unwrap_or(sea_query::Value::Bool(None))
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
    fn check(col: sea_query::Alias) -> Option<sea_query::SimpleExpr> {
        T::check(col)
    }
}

macro_rules! impl_typ {
    ($typ:ty, $can:expr, $map:expr) => {
        impl_typ!($typ, $can, $map, |_col| None);
    };
    ($typ:ty, $can:expr, $map:expr, $check:expr) => {
        impl DbTyp for $typ {
            type Prev = Self;
            const TYP: canonical::ColumnType = $can;
            type Ext<'t> = ();
            type Sql = Self;
            type FromLazy<'x> = Self;

            fn migrate(prev: Self) -> Self {
                prev
            }
            fn from_lazy(lazy: &Self::FromLazy<'_>) -> Self {
                lazy.clone()
            }
            fn out_to_value(self) -> sea_query::Value {
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
            fn check(col: sea_query::Alias) -> Option<sea_query::SimpleExpr> {
                let f: fn(col: sea_query::Alias) -> _ = $check;
                return f(col);
            }
        }
    };
}
impl_typ!(i64, canonical::ColumnType::Integer, |x| x.as_i64());
impl_typ!(String, canonical::ColumnType::Text, |x| x
    .as_str()
    .map(ToOwned::to_owned));
impl_typ!(
    bool,
    canonical::ColumnType::Integer,
    |x| x.as_i64().map(|x| x != 0),
    |col| Some(sea_query::Expr::col(col).is_in([CONST_0, CONST_1]))
);
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
            sea_query::Value::String(Some(good.to_owned()))
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
            sea_query::Value::String(Some(good.to_owned()))
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
