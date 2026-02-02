use std::{cell::OnceCell, marker::PhantomData};

use sea_query::Nullable;

use crate::{Table, TableRow, Transaction, db::TableRowInner, schema::canonical, value::EqTyp};

pub trait DbTyp: 'static {
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

    fn from_sql(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self>;
}

impl<T: Table + ?Sized> DbTyp for TableRow<T> {
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

macro_rules! impl_typ {
    ($typ:ty, $can:expr, $var:pat => $map:expr) => {
        impl MyTyp for $typ {
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
                $var: rusqlite::types::ValueRef<'_>,
            ) -> rusqlite::types::FromSqlResult<Self> {
                $map
            }
        }
    };
}
impl_typ!(i64, canonical::ColumnType::Integer, x => x.as_i64());
impl_typ!(String, canonical::ColumnType::Text, x => x.as_str().map(str::to_owned));
impl_typ!(bool, canonical::ColumnType::Integer, x => x.as_i64().map(|x|x != 0));
impl_typ!(Vec<u8>, canonical::ColumnType::Blob, x => x.as_blob());
impl_typ!(f64, canonical::ColumnType::Real, x => x.as_f64());
