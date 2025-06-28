use std::{cell::OnceCell, fmt::Debug, marker::PhantomData, ops::Deref};

use sea_query::{Alias, SimpleExpr};

use crate::{
    Expr, IntoExpr, LocalClient, Table,
    alias::MyAlias,
    value::{MyTableRef, Typed, ValueBuilder},
};

/// Table reference that is the result of a join.
/// It can only be used in the query where it was created.
/// Invariant in `'t`.
pub(crate) struct Join<T> {
    pub(crate) table_idx: MyTableRef,
    pub(crate) _p: PhantomData<T>,
}

impl<T> Join<T> {
    pub(crate) fn new(table_idx: MyTableRef) -> Self {
        Self {
            table_idx,
            _p: PhantomData,
        }
    }
}

impl<T: Table> Typed for Join<T> {
    type Typ = T;
    fn build_expr(&self, b: &mut ValueBuilder) -> SimpleExpr {
        sea_query::Expr::col((self.build_table(b), Alias::new(T::ID))).into()
    }
    fn build_table(&self, b: &mut ValueBuilder) -> MyAlias {
        b.get_table::<T>(self.table_idx.clone())
    }
}

/// Row reference that can be used in any query in the same transaction.
///
/// [TableRow] is covariant in `'t` and restricted to a single thread to prevent it from being used in a different transaction.
pub struct TableRow<'t, T: Table> {
    pub(crate) _p: PhantomData<&'t ()>,
    pub(crate) _local: PhantomData<LocalClient>,
    pub(crate) inner: TableRowInner<T>,
    pub(crate) ext: OnceCell<Box<T::Ext2<'t>>>,
}

impl<T: Table> TableRow<'_, T> {
    pub(crate) fn new(idx: i64) -> Self {
        Self {
            _p: PhantomData,
            _local: PhantomData,
            inner: TableRowInner {
                _p: PhantomData,
                idx,
            },
            ext: OnceCell::new(),
        }
    }
}

impl<T: Table> Eq for TableRow<'_, T> {}

impl<T: Table> PartialOrd for TableRow<'_, T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T: Table> Ord for TableRow<'_, T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.inner.idx.cmp(&other.inner.idx)
    }
}

pub(crate) struct TableRowInner<T> {
    pub(crate) _p: PhantomData<T>,
    pub(crate) idx: i64,
}

impl<T: Table> PartialEq for TableRow<'_, T> {
    fn eq(&self, other: &Self) -> bool {
        self.inner.idx == other.inner.idx
    }
}

impl<T: Table> Debug for TableRow<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "db_{}", self.inner.idx)
    }
}

impl<T: Table> Clone for TableRow<'_, T> {
    fn clone(&self) -> Self {
        Self {
            _p: PhantomData,
            _local: PhantomData,
            inner: self.inner,
            ext: OnceCell::new(),
        }
    }
}

impl<T> Clone for TableRowInner<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for TableRowInner<T> {}

impl<'t, T: Table> Deref for TableRow<'t, T> {
    type Target = T::Ext2<'t>;

    fn deref(&self) -> &Self::Target {
        todo!()
    }
}

impl<T: Table> From<TableRow<'_, T>> for sea_query::Value {
    fn from(value: TableRow<T>) -> Self {
        value.inner.idx.into()
    }
}

impl<T: Table> Typed for TableRowInner<T> {
    type Typ = T;
    fn build_expr(&self, _: &mut ValueBuilder) -> SimpleExpr {
        sea_query::Expr::val(self.idx).into()
    }
}

impl<'t, S, T: Table> IntoExpr<'t, S> for TableRow<'t, T> {
    type Typ = T;
    fn into_expr(self) -> Expr<'t, S, Self::Typ> {
        Expr::new(self.inner)
    }
}

/// This makes it possible to use TableRow as a parameter in
/// rusqlite queries and statements.
impl<T: Table> rusqlite::ToSql for TableRow<'_, T> {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        self.inner.idx.to_sql()
    }
}
