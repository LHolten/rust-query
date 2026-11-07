use std::{cell::OnceCell, marker::PhantomData};

use crate::{
    Table, TableRow, Transaction, db::TableRowInner, lower::list_writer::Alias, schema::canonical,
    value::EqTyp,
};

pub type LazyOf<'t, T> = <T as WrapTyp>::Wrap<<<T as DbTypOpt>::Actual as DbTyp>::Lazy<'t>>;
pub type FromLazyOf<'x, T> = <T as WrapTyp>::Wrap<<<T as DbTypOpt>::Actual as DbTyp>::FromLazy<'x>>;

pub fn from_lazy<T: DbTypOpt>(lazy: &FromLazyOf<'_, T>) -> T {
    match T::to_option(T::as_ref(lazy)) {
        Some(val) => T::some(T::Actual::from_lazy(val)),
        None => T::null::<T::Actual>().unwrap(),
    }
}

pub fn out_to_lazy<'t, T: DbTypOpt>(val: T) -> LazyOf<'t, T> {
    match T::to_option::<T::Actual>(val) {
        Some(val) => T::some(val.out_to_lazy()),
        None => T::null::<<T::Actual as DbTyp>::Lazy<'t>>().unwrap(),
    }
}

pub trait WrapTyp: Sized + 'static {
    /// Either `X` or `Option<X>`.
    type Wrap<X>;
    fn to_option<X>(val: Self::Wrap<X>) -> Option<X>;
    fn some<X>(val: X) -> Self::Wrap<X>;
    /// Returns [None] if not nullable.
    fn null<X>() -> Option<Self::Wrap<X>>;
    fn as_ref<X>(val: &Self::Wrap<X>) -> Self::Wrap<&X>;
}
impl<T: DbTyp> WrapTyp for T {
    type Wrap<X> = X;
    fn to_option<X>(val: X) -> Option<X> {
        Some(val)
    }
    fn some<X>(val: X) -> X {
        val
    }
    fn null<X>() -> Option<X> {
        None
    }
    fn as_ref<X>(val: &X) -> &X {
        val
    }
}
impl<T: DbTyp> WrapTyp for Option<T> {
    type Wrap<X> = Option<X>;
    fn to_option<X>(val: Option<X>) -> Option<X> {
        val
    }
    fn some<X>(val: X) -> Option<X> {
        Some(val)
    }
    fn null<X>() -> Option<Option<X>> {
        Some(None)
    }
    fn as_ref<X>(val: &Option<X>) -> Option<&X> {
        val.as_ref()
    }
}

pub trait DbTypOpt: WrapTyp<Wrap<Self::Actual> = Self> {
    type Actual: DbTyp;
}
impl<T: DbTyp> DbTypOpt for T {
    type Actual = Self;
}
impl<T: DbTyp> DbTypOpt for Option<T> {
    type Actual = T;
}

/// The types that can be used inside [crate::Expr].
/// Some stuff like nested [Option] is not allowed.
pub trait DbTyp: Sized + 'static {
    type Base: BaseTyp;

    type Ext<'t>;

    type FromLazy<'x>;
    type Lazy<'t>: Sized;

    fn from_lazy(lazy: &Self::FromLazy<'_>) -> Self;
    fn out_to_lazy<'t>(self) -> Self::Lazy<'t>;

    fn out_to_value(self) -> Self::Base;
    fn from_sql(value: <Self::Base as BaseTyp>::Ref<'_>) -> Self;
}

/// Not all types are allowed to be stored.
/// Specificially `#[no_reference]` references.
pub trait StorableTyp: DbTypOpt {
    const TYP: canonical::ColumnType;
    const FK: Option<(&'static str, &'static str)> = None;

    fn check(_col: &str) -> Option<String>;

    type Prev;
    fn migrate(prev: Self::Prev) -> Self;
}

pub trait BaseTyp: Sized {
    type Ref<'x>;
    fn base_from_sql(value: rusqlite::types::ValueRef<'_>) -> Self::Ref<'_>;
    fn base_to_value(val: Self) -> rusqlite::types::Value;
}
impl BaseTyp for i64 {
    type Ref<'x> = i64;
    fn base_from_sql(value: rusqlite::types::ValueRef<'_>) -> Self::Ref<'_> {
        value.as_i64().unwrap()
    }
    fn base_to_value(val: Self) -> rusqlite::types::Value {
        rusqlite::types::Value::Integer(val)
    }
}
impl BaseTyp for f64 {
    type Ref<'x> = f64;
    fn base_from_sql(value: rusqlite::types::ValueRef<'_>) -> Self::Ref<'_> {
        value.as_f64().unwrap()
    }
    fn base_to_value(val: Self) -> rusqlite::types::Value {
        rusqlite::types::Value::Real(val)
    }
}
impl BaseTyp for String {
    type Ref<'x> = &'x str;
    fn base_from_sql(value: rusqlite::types::ValueRef<'_>) -> Self::Ref<'_> {
        value.as_str().unwrap()
    }
    fn base_to_value(val: Self) -> rusqlite::types::Value {
        rusqlite::types::Value::Text(val)
    }
}
impl BaseTyp for Vec<u8> {
    type Ref<'x> = &'x [u8];
    fn base_from_sql(value: rusqlite::types::ValueRef<'_>) -> Self::Ref<'_> {
        value.as_blob().unwrap()
    }
    fn base_to_value(val: Self) -> rusqlite::types::Value {
        rusqlite::types::Value::Blob(val)
    }
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
    fn out_to_value(self) -> Self::Base {
        // check that year is positive
        assert!(self >= jiff::Timestamp::from_second(-62167219200).unwrap());
        // Use space instead of `T` for date and time separator
        self.strftime("%F %T%.f").to_string()
    }

    fn out_to_lazy<'t>(self) -> Self::Lazy<'t> {
        self
    }

    fn from_sql(value: <Self::Base as BaseTyp>::Ref<'_>) -> Self {
        use std::str::FromStr;

        let dt = jiff::civil::DateTime::from_str(value).unwrap();
        jiff::tz::TimeZone::UTC.to_timestamp(dt).unwrap()
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
    fn out_to_value(self) -> Self::Base {
        // check that year is positive
        assert!(
            self.year() >= 0,
            "only dates with a year that is greater than or equal to zero can be used"
        );
        self.to_string()
    }

    fn out_to_lazy<'t>(self) -> Self::Lazy<'t> {
        self
    }

    fn from_sql(value: <Self::Base as BaseTyp>::Ref<'_>) -> Self {
        use std::str::FromStr;

        jiff::civil::Date::from_str(value).unwrap()
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
    fn out_to_value(self) -> Self::Base {
        self.inner.idx
    }
    type Lazy<'t> = crate::Lazy<'t, T>;
    fn out_to_lazy<'t>(self) -> Self::Lazy<'t> {
        crate::Lazy {
            id: self,
            lazy: OnceCell::new(),
            txn: Transaction::new_ref(),
        }
    }

    fn from_sql(value: <Self::Base as BaseTyp>::Ref<'_>) -> Self {
        TableRow {
            _local: PhantomData,
            inner: TableRowInner {
                _p: PhantomData,
                idx: value,
            },
        }
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
    fn out_to_value(self) -> Self::Base {
        self.into()
    }
    type Lazy<'t> = Self;
    fn out_to_lazy<'t>(self) -> Self::Lazy<'t> {
        self
    }
    fn from_sql(val: <Self::Base as BaseTyp>::Ref<'_>) -> Self {
        val != 0
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
    ($typ:ty, $base:ty, $can:expr, $map:expr) => {
        impl DbTyp for $typ {
            type Base = $base;
            type Ext<'t> = ();
            type FromLazy<'x> = Self;

            fn from_lazy(lazy: &Self::FromLazy<'_>) -> Self {
                lazy.clone()
            }
            fn out_to_value(self) -> Self::Base {
                self.into()
            }
            type Lazy<'t> = Self;
            fn out_to_lazy<'t>(self) -> Self::Lazy<'t> {
                self
            }
            fn from_sql(val: <Self::Base as BaseTyp>::Ref<'_>) -> Self {
                let f: fn(<Self::Base as BaseTyp>::Ref<'_>) -> _ = $map;
                f(val)
            }
        }

        impl StorableTyp for $typ {
            const TYP: canonical::ColumnType = $can;

            fn check(_col: &str) -> Option<String> {
                None
            }

            type Prev = Self;
            fn migrate(prev: Self) -> Self {
                prev
            }
        }
    };
}
impl_typ!(i64, i64, canonical::ColumnType::Integer, |x| x);
impl_typ!(String, String, canonical::ColumnType::Text, |x| x
    .to_owned());

impl_typ!(Vec<u8>, Vec<u8>, canonical::ColumnType::Blob, |x| x
    .to_owned());
impl_typ!(f64, f64, canonical::ColumnType::Real, |x| x);

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
