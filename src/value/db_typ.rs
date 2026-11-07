use std::{cell::OnceCell, marker::PhantomData};

use crate::{Table, TableRow, Transaction, db::TableRowInner, schema::canonical};

pub type LazyOf<'t, T> = <T as WrapTyp>::Wrap<<<T as DbTypOpt>::Actual as DbTyp>::Lazy<'t>>;
pub type FromLazyOf<'x, T> =
    <T as WrapTyp>::Wrap<<<T as DbTypOpt>::Actual as StorableTyp>::FromLazy<'x>>;
pub type PrevOf<T> = <T as WrapTyp>::Wrap<<<T as DbTypOpt>::Actual as StorableTyp>::Prev>;

pub fn migrate<T: StorableTypOpt>(prev: PrevOf<T>) -> T {
    match T::to_option(prev) {
        Some(val) => T::some(T::Actual::migrate(val)),
        None => T::null::<T::Actual>().unwrap(),
    }
}

pub fn from_lazy<T: StorableTypOpt>(lazy: &FromLazyOf<'_, T>) -> T {
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

pub trait StorableTypOpt: DbTypOpt<Actual: StorableTyp> {}
impl<T: DbTypOpt<Actual: StorableTyp>> StorableTypOpt for T {}

mod refinement_typ;

/// The types that can be used inside [crate::Expr].
/// Some stuff like nested [Option] is not allowed.
pub trait DbTyp: Sized + 'static {
    type Base: BaseTyp;

    type Lazy<'t>: Sized;

    fn out_to_lazy<'t>(self) -> Self::Lazy<'t>;

    fn out_to_value(self) -> Self::Base;
    fn from_sql(value: <Self::Base as BaseTyp>::Ref<'_>) -> Self;
}

/// Not all types are allowed to be stored.
/// Specificially `#[no_reference]` references.
pub trait StorableTyp: DbTyp {
    const FK: Option<(&'static str, &'static str)> = None;

    fn check(_col: &str) -> Option<String>;

    type Prev;
    fn migrate(prev: Self::Prev) -> Self;
    type FromLazy<'x>;
    fn from_lazy(lazy: &Self::FromLazy<'_>) -> Self;
}

pub trait BaseTyp: Sized {
    const TYP: canonical::ColumnType;
    type Ref<'x>;
    fn base_from_sql(value: rusqlite::types::ValueRef<'_>) -> Self::Ref<'_>;
    fn base_to_value(val: Self) -> rusqlite::types::Value;
}
impl BaseTyp for i64 {
    const TYP: canonical::ColumnType = canonical::ColumnType::Integer;
    type Ref<'x> = i64;
    fn base_from_sql(value: rusqlite::types::ValueRef<'_>) -> Self::Ref<'_> {
        value.as_i64().unwrap()
    }
    fn base_to_value(val: Self) -> rusqlite::types::Value {
        rusqlite::types::Value::Integer(val)
    }
}
impl BaseTyp for f64 {
    const TYP: canonical::ColumnType = canonical::ColumnType::Real;
    type Ref<'x> = f64;
    fn base_from_sql(value: rusqlite::types::ValueRef<'_>) -> Self::Ref<'_> {
        value.as_f64().unwrap()
    }
    fn base_to_value(val: Self) -> rusqlite::types::Value {
        rusqlite::types::Value::Real(val)
    }
}
impl BaseTyp for String {
    const TYP: canonical::ColumnType = canonical::ColumnType::Text;
    type Ref<'x> = &'x str;
    fn base_from_sql(value: rusqlite::types::ValueRef<'_>) -> Self::Ref<'_> {
        value.as_str().unwrap()
    }
    fn base_to_value(val: Self) -> rusqlite::types::Value {
        rusqlite::types::Value::Text(val)
    }
}
impl BaseTyp for Vec<u8> {
    const TYP: canonical::ColumnType = canonical::ColumnType::Blob;
    type Ref<'x> = &'x [u8];
    fn base_from_sql(value: rusqlite::types::ValueRef<'_>) -> Self::Ref<'_> {
        value.as_blob().unwrap()
    }
    fn base_to_value(val: Self) -> rusqlite::types::Value {
        rusqlite::types::Value::Blob(val)
    }
}

impl<T: Table> DbTyp for TableRow<T> {
    type Base = i64;

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
    const FK: Option<(&'static str, &'static str)> = Some((T::NAME, T::ID));

    fn check(_col: &str) -> Option<String> {
        None
    }

    type Prev = TableRow<T::MigrateFrom>;
    fn migrate(prev: Self::Prev) -> Self {
        TableRow::migrate_row(prev)
    }

    type FromLazy<'x> = crate::Lazy<'x, <T as crate::Table>::MigrateFrom>;
    fn from_lazy(lazy: &Self::FromLazy<'_>) -> Self {
        TableRow::migrate_row(lazy.table_row())
    }
}

macro_rules! impl_typ {
    ($typ:ty, $base:ty, $can:expr, $map:expr) => {
        impl DbTyp for $typ {
            type Base = $base;
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
            fn check(_col: &str) -> Option<String> {
                None
            }

            type Prev = Self;
            fn migrate(prev: Self) -> Self {
                prev
            }

            type FromLazy<'x> = Self;
            fn from_lazy(lazy: &Self::FromLazy<'_>) -> Self {
                lazy.clone()
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
