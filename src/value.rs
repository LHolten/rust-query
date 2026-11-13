pub mod aggregate;
mod db_typ;
pub mod from_expr;
pub mod into_expr;
#[cfg(feature = "jiff-02")]
mod jiff_operations;
mod operations;
pub mod optional;
pub mod refinement_typ;

use std::{
    any::Any,
    cell::{Cell, OnceCell},
    fmt::Debug,
    marker::PhantomData,
    ops::Deref,
    rc::Rc,
};

use crate::{
    IntoExpr, Table,
    db::TableRow,
    lower,
    mutable::Mutable,
    private::IntoJoinable,
    scoped_transaction::{MutTemp, TransactionScope},
};
pub use db_typ::{
    BaseTyp, DbTyp, DbTypOpt, LazyOf, PrevOf, StorableTyp, WrapTyp, db_typ_to_expr, from_lazy,
    migrate, out_to_lazy,
};

pub trait NumTyp: OrdTyp + Copy {
    const ZERO: &str;
}

impl NumTyp for i64 {
    const ZERO: &str = "0";
}
impl NumTyp for f64 {
    const ZERO: &str = "0.0";
}

pub trait OrdTyp: DbTyp {}
impl OrdTyp for String {}
impl OrdTyp for Vec<u8> {}
impl OrdTyp for i64 {}
impl OrdTyp for f64 {}
impl OrdTyp for bool {}
#[cfg(feature = "jiff-02")]
impl OrdTyp for jiff::Timestamp {}
#[cfg(feature = "jiff-02")]
impl OrdTyp for jiff::civil::Date {}

pub trait BuffTyp: DbTyp {}
impl BuffTyp for String {}
impl BuffTyp for Vec<u8> {}

/// Should not be used outside this crate.
pub trait OptTable: DbTypOpt {
    type Schema;
    type Mutable<'t>;

    fn into_mutable<'t>(
        txn: &'t mut TransactionScope<Self::Schema>,
        val: Self,
    ) -> Self::Mutable<'t>;
}

impl<T: Table> OptTable for TableRow<T> {
    type Schema = T::Schema;
    type Mutable<'t> = Mutable<'t, T>;

    fn into_mutable<'t>(
        txn: &'t mut TransactionScope<Self::Schema>,
        inp: Self,
    ) -> Self::Mutable<'t> {
        txn.tmp = Cell::new(vec![MutTemp::new(inp)]);
        Mutable::new(&mut *Cell::get_mut(&mut txn.tmp)[0])
    }
}

impl<T: Table> OptTable for Option<TableRow<T>> {
    type Schema = T::Schema;
    type Mutable<'t> = Option<Mutable<'t, T>>;

    fn into_mutable<'t>(
        txn: &'t mut TransactionScope<Self::Schema>,
        val: Self,
    ) -> Self::Mutable<'t> {
        val.map(|x| TableRow::<T>::into_mutable(txn, x))
    }
}

/// This is an expression that can be used in queries.
///
/// - The lifetime parameter `'column` specifies which columns need to be in scope.
/// - The type parameter `S` specifies the expected schema of the query.
/// - And finally the type paramter `T` specifies the type of the expression.
///
/// [Expr] implements [Deref] to have column fields in case the expression has a table type.
pub struct Expr<'column, S, T: DbTypOpt> {
    pub(crate) _local: PhantomData<*const ()>,
    pub(crate) inner: Rc<lower::Expr>,
    pub(crate) _p: PhantomData<&'column ()>,
    pub(crate) _p2: PhantomData<(S, T)>,
    pub(crate) ext: OnceCell<Box<dyn Any>>,
    // is this expressions not null and a foreign key?
    pub(crate) not_null_key: bool,
}

#[cfg_attr(false, mutants::skip)]
impl<S, T: DbTyp> Debug for Expr<'_, S, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Expr of type {}", std::any::type_name::<T>())
    }
}

impl<'column, S, T: DbTyp> Expr<'column, S, T> {
    /// Extremely easy to use API. Should only be used by the macro to implement migrations.
    #[doc(hidden)]
    pub fn _migrate<OldS>(prev: impl IntoExpr<'column, OldS>) -> Self {
        let prev = prev.into_expr().inner;
        Self::new(prev)
    }
}

pub fn adhoc_expr<S, T: DbTyp>(f: lower::Expr) -> Expr<'static, S, T> {
    Expr::adhoc(f)
}

pub fn new_column<'x, S, C: DbTypOpt, T: Table>(
    table: impl IntoExpr<'x, S, Typ = TableRow<T>>,
    name: &'static str,
) -> Expr<'x, S, C> {
    let table = table.into_expr();
    Expr::new_inner(
        table.inner.col(T::NAME, name, T::ID, table.not_null_key),
        table.not_null_key && !C::from_option::<()>(None).is_some(),
    )
}

pub fn unique_from_joinable<'inner, T: Table>(
    j: impl IntoJoinable<'inner, T::Schema, Typ = TableRow<T>>,
) -> Expr<'inner, T::Schema, Option<TableRow<T>>> {
    let joinable = j.into_joinable();
    let unique = Rc::new(lower::Unique {
        table: joinable.table.name,
        conds: joinable.conds,
        guaranteed: false,
    });
    Expr::adhoc(lower::Expr::RowIndex(lower::RowLike::Unique(unique), T::ID))
}

impl<S, T: DbTypOpt> Expr<'_, S, T> {
    pub(crate) fn adhoc(e: lower::Expr) -> Self {
        Self::new(Rc::new(e))
    }

    pub(crate) fn new(val: Rc<lower::Expr>) -> Self {
        Self::new_inner(val, false)
    }

    pub(crate) fn new_inner(val: Rc<lower::Expr>, not_null_key: bool) -> Self {
        Self {
            _local: PhantomData,
            inner: val,
            _p: PhantomData,
            _p2: PhantomData,
            ext: OnceCell::new(),
            not_null_key,
        }
    }
}

impl<S, T: DbTypOpt> Clone for Expr<'_, S, T> {
    fn clone(&self) -> Self {
        Self {
            _local: PhantomData,
            inner: self.inner.clone(),
            _p: self._p,
            _p2: self._p2,
            ext: OnceCell::new(),
            not_null_key: self.not_null_key,
        }
    }
}

impl<'t, T: Table> Deref for Expr<'t, T::Schema, TableRow<T>> {
    type Target = T::Ext2<'t>;

    fn deref(&self) -> &Self::Target {
        let ext = self.ext.get_or_init(|| {
            let expr = Expr {
                _local: PhantomData,
                inner: self.inner.clone(),
                _p: PhantomData::<&'static ()>,
                _p2: PhantomData,
                ext: OnceCell::new(),
                not_null_key: self.not_null_key,
            };
            Box::new(T::build_ext2(&expr))
        });
        T::covariant_ext(ext.downcast_ref().unwrap())
    }
}
