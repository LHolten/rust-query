use std::{cell::OnceCell, marker::PhantomData};

use crate::{
    Table, TableRow, Transaction, db::TableRowInner, lower::ord_rc::OrdRc, schema::canonical,
};

pub type LazyOf<'t, T> = <T as WrapTyp>::Wrap<<<T as DbTypOpt>::Actual as DbTyp>::Lazy<'t>>;
pub type FromLazyOf<'x, T> =
    <T as WrapTyp>::Wrap<<<<T as DbTypOpt>::Actual as StorableTyp>::Prev as DbTyp>::Lazy<'x>>;
pub type PrevOf<T> = <T as WrapTyp>::Wrap<<<T as DbTypOpt>::Actual as StorableTyp>::Prev>;

pub fn migrate<T: DbTypOpt<Actual: StorableTyp>>(prev: PrevOf<T>) -> T {
    T::from_option::<T::Actual>(T::to_option(prev).map(StorableTyp::migrate)).unwrap()
}

pub fn from_lazy<T: DbTypOpt<Actual: StorableTyp>>(lazy: &FromLazyOf<'_, T>) -> T {
    T::from_option::<T::Actual>(T::to_option(T::as_ref(lazy)).map(StorableTyp::from_lazy)).unwrap()
}

pub fn out_to_lazy<'t, T: DbTypOpt>(val: T) -> LazyOf<'t, T> {
    T::from_option(T::to_option::<T::Actual>(val).map(DbTyp::out_to_lazy)).unwrap()
}

pub fn db_typ_to_expr<S, T: DbTyp>(t: T) -> crate::Expr<'static, S, T> {
    crate::Expr::adhoc(crate::lower::Expr::Parameter(OrdRc::new(t.out_to_value())))
}

pub trait WrapTyp: Sized + 'static {
    /// Either `X` or `Option<X>`.
    type Wrap<X>;
    fn to_option<X>(val: Self::Wrap<X>) -> Option<X>;
    fn from_option<X>(val: Option<X>) -> Option<Self::Wrap<X>>;
    fn as_ref<X>(val: &Self::Wrap<X>) -> Self::Wrap<&X>;
}
impl<T: DbTyp> WrapTyp for T {
    type Wrap<X> = X;
    fn to_option<X>(val: X) -> Option<X> {
        Some(val)
    }
    fn from_option<X>(val: Option<X>) -> Option<Self::Wrap<X>> {
        val
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
    fn from_option<X>(val: Option<X>) -> Option<Self::Wrap<X>> {
        Some(val)
    }
    fn as_ref<X>(val: &Option<X>) -> Option<&X> {
        val.as_ref()
    }
}

#[diagnostic::on_unimplemented(
    message = "Can not use `{Self}` as a column type",
    note = "column types can be wrapped in at most one `Option`"
)]
pub trait DbTypOpt: WrapTyp<Wrap<Self::Actual> = Self> {
    type Actual: DbTyp;
}
impl<T: DbTyp> DbTypOpt for T {
    type Actual = Self;
}
#[diagnostic::do_not_recommend]
impl<T: DbTyp> DbTypOpt for Option<T> {
    type Actual = T;
}

/// The types that can be used inside [crate::Expr].
/// Some stuff like nested [Option] is not allowed.
#[diagnostic::on_unimplemented(
    message = "`{Self}` can not be used here, expected a non-optional column type"
)]
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

    type Prev: DbTyp;
    fn migrate(prev: Self::Prev) -> Self;
    fn from_lazy(lazy: &<Self::Prev as DbTyp>::Lazy<'_>) -> Self;
}

pub trait BaseTyp: Into<rusqlite::types::Value> {
    const TYP: canonical::ColumnType;
    type Ref<'x>;
    fn base_from_sql(value: rusqlite::types::ValueRef<'_>) -> Self::Ref<'_>;
}
impl BaseTyp for i64 {
    const TYP: canonical::ColumnType = canonical::ColumnType::Integer;
    type Ref<'x> = i64;
    fn base_from_sql(value: rusqlite::types::ValueRef<'_>) -> Self::Ref<'_> {
        value.as_i64().unwrap()
    }
}
impl BaseTyp for f64 {
    const TYP: canonical::ColumnType = canonical::ColumnType::Real;
    type Ref<'x> = f64;
    fn base_from_sql(value: rusqlite::types::ValueRef<'_>) -> Self::Ref<'_> {
        value.as_f64().unwrap()
    }
}
impl BaseTyp for String {
    const TYP: canonical::ColumnType = canonical::ColumnType::Text;
    type Ref<'x> = &'x str;
    fn base_from_sql(value: rusqlite::types::ValueRef<'_>) -> Self::Ref<'_> {
        value.as_str().unwrap()
    }
}
impl BaseTyp for Vec<u8> {
    const TYP: canonical::ColumnType = canonical::ColumnType::Blob;
    type Ref<'x> = &'x [u8];
    fn base_from_sql(value: rusqlite::types::ValueRef<'_>) -> Self::Ref<'_> {
        value.as_blob().unwrap()
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

    fn from_lazy(lazy: &crate::Lazy<'_, T::MigrateFrom>) -> Self {
        TableRow::migrate_row(lazy.table_row())
    }
}

macro_rules! impl_typ {
    ($($typ:ty),*) => {$(
        crate::refinement_typ!($typ, $typ, |x| x, |x| x.to_owned());
    )*};
}
impl_typ!(i64, f64, String, Vec<u8>);
