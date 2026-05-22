pub mod aggregate;
mod db_typ;
pub mod from_expr;
pub mod into_expr;
#[cfg(feature = "jiff-02")]
mod jiff_operations;
mod operations;
pub mod optional;

use std::{cell::OnceCell, fmt::Debug, marker::PhantomData, ops::Deref, rc::Rc};

use crate::{
    IntoExpr, IntoSelect, Select, Table, db::TableRow, lower, mutable::Mutable,
    private::IntoJoinable,
};
pub use db_typ::{DbTyp, StorableTyp};

pub trait NumTyp: OrdTyp + Clone + Copy {
    const ZERO: &str;
}

impl NumTyp for i64 {
    const ZERO: &str = "0";
}
impl NumTyp for f64 {
    const ZERO: &str = "0.0";
}

pub trait OrdTyp: EqTyp {}
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

#[diagnostic::on_unimplemented(
    message = "Columns with type `{Self}` can not be checked for equality",
    note = "`EqTyp` is also implemented for all table types"
)]
pub trait EqTyp: DbTyp {}

impl EqTyp for String {}
impl EqTyp for Vec<u8> {}
impl EqTyp for i64 {}
impl EqTyp for f64 {}
impl EqTyp for bool {}
#[cfg(feature = "jiff-02")]
impl EqTyp for jiff::Timestamp {}
#[cfg(feature = "jiff-02")]
impl EqTyp for jiff::civil::Date {}
#[diagnostic::do_not_recommend]
impl<T: Table> EqTyp for TableRow<T> {}

pub trait OptTable: DbTyp {
    type Schema;
    type Select;
    type Mutable<'t>;
    fn select_opt_mutable(
        val: Expr<'_, Self::Schema, Self>,
    ) -> Select<'_, Self::Schema, Self::Select>;

    fn into_mutable<'t>(val: Self::Select) -> Self::Mutable<'t>;
}

impl<T: Table> OptTable for TableRow<T> {
    type Schema = T::Schema;
    type Select = (T::Select, TableRow<T>);
    type Mutable<'t> = Mutable<'t, T>;
    fn select_opt_mutable(
        val: Expr<'_, Self::Schema, Self>,
    ) -> Select<'_, Self::Schema, Self::Select> {
        (T::into_select(val.clone()), val).into_select()
    }

    fn into_mutable<'t>((inner, row_id): Self::Select) -> Self::Mutable<'t> {
        Mutable::new(T::select_mutable(inner), row_id)
    }
}

impl<T: Table> OptTable for Option<TableRow<T>> {
    type Schema = T::Schema;
    type Select = Option<(T::Select, TableRow<T>)>;
    type Mutable<'t> = Option<Mutable<'t, T>>;
    fn select_opt_mutable(
        val: Expr<'_, Self::Schema, Self>,
    ) -> Select<'_, Self::Schema, Self::Select> {
        crate::optional(|row| {
            let val = row.and(val);
            row.then_select((T::into_select(val.clone()), val))
        })
    }

    fn into_mutable<'t>(val: Self::Select) -> Self::Mutable<'t> {
        val.map(TableRow::<T>::into_mutable)
    }
}

/// This is an expression that can be used in queries.
///
/// - The lifetime parameter `'column` specifies which columns need to be in scope.
/// - The type parameter `S` specifies the expected schema of the query.
/// - And finally the type paramter `T` specifies the type of the expression.
///
/// [Expr] implements [Deref] to have column fields in case the expression has a table type.
pub struct Expr<'column, S, T: DbTyp> {
    pub(crate) _local: PhantomData<*const ()>,
    pub(crate) inner: Rc<lower::Expr>,
    pub(crate) _p: PhantomData<&'column ()>,
    pub(crate) _p2: PhantomData<S>,
    pub(crate) ext: OnceCell<Box<T::Ext<'static>>>,
}

#[cfg_attr(test, mutants::skip)]
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

pub fn new_column<'x, S, C: DbTyp, T: Table>(
    table: impl IntoExpr<'x, S, Typ = TableRow<T>>,
    name: &'static str,
) -> Expr<'x, S, C> {
    let table = table.into_expr().inner;
    let unique = Rc::new(lower::Unique {
        table: lower::JoinableTable::Table(T::NAME, T::ID),
        conds: vec![(T::ID, table)],
    });
    Expr::adhoc(lower::Expr::RowIndex(lower::RowLike::Unique(unique), name))
}

pub fn unique_from_joinable<'inner, T: Table>(
    j: impl IntoJoinable<'inner, T::Schema, Typ = TableRow<T>>,
) -> Expr<'inner, T::Schema, Option<TableRow<T>>> {
    let joinable = j.into_joinable();
    let unique = Rc::new(lower::Unique {
        table: joinable.table,
        conds: joinable.conds,
    });
    Expr::adhoc(lower::Expr::RowIndex(lower::RowLike::Unique(unique), T::ID))
}

pub struct AdHoc<F: ?Sized, T: ?Sized> {
    maybe_optional: bool,
    _p: PhantomData<T>,
    func: F,
}

impl<S, T: DbTyp> Expr<'_, S, T> {
    pub(crate) fn adhoc(e: lower::Expr) -> Self {
        Self::new(Rc::new(e))
    }

    pub(crate) fn new(val: Rc<lower::Expr>) -> Self {
        Self {
            _local: PhantomData,
            inner: val,
            _p: PhantomData,
            _p2: PhantomData,
            ext: OnceCell::new(),
        }
    }
}

impl<S, T: DbTyp> Clone for Expr<'_, S, T> {
    fn clone(&self) -> Self {
        Self {
            _local: PhantomData,
            inner: self.inner.clone(),
            _p: self._p,
            _p2: self._p2,
            ext: OnceCell::new(),
        }
    }
}

impl<'t, T: Table> Deref for Expr<'t, T::Schema, TableRow<T>> {
    type Target = T::Ext2<'t>;

    fn deref(&self) -> &Self::Target {
        T::covariant_ext(self.ext.get_or_init(|| {
            let expr = Expr {
                _local: PhantomData,
                inner: self.inner.clone(),
                _p: PhantomData::<&'static ()>,
                _p2: PhantomData,
                ext: OnceCell::new(),
            };
            Box::new(T::build_ext2(&expr))
        }))
    }
}
