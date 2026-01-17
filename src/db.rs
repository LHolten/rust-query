use std::{fmt::Debug, marker::PhantomData};

use crate::{Expr, IntoExpr, Table};

/// Row reference that can be used in any query in the same transaction.
///
/// [TableRow] is restricted to a single thread to prevent it from being used in a different transaction.
pub struct TableRow<T: Table> {
    pub(crate) _local: PhantomData<*const ()>,
    pub(crate) inner: TableRowInner<T>,
}

impl<T: Table> Eq for TableRow<T> {}

impl<T: Table> PartialOrd for TableRow<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T: Table> Ord for TableRow<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.inner.idx.cmp(&other.inner.idx)
    }
}

pub(crate) struct TableRowInner<T> {
    pub(crate) _p: PhantomData<T>,
    pub(crate) idx: i64,
}

impl<T: Table> PartialEq for TableRow<T> {
    fn eq(&self, other: &Self) -> bool {
        self.inner.idx == other.inner.idx
    }
}

impl<T: Table> Debug for TableRow<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "db_{}", self.inner.idx)
    }
}

impl<T: Table> Clone for TableRow<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Table> Copy for TableRow<T> {}

impl<T> Clone for TableRowInner<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for TableRowInner<T> {}

impl<T: Table> From<TableRow<T>> for sea_query::Value {
    fn from(value: TableRow<T>) -> Self {
        value.inner.idx.into()
    }
}

// works for any schema?
impl<'column, S, T: Table> IntoExpr<'column, S> for TableRow<T> {
    type Typ = T;
    fn into_expr(self) -> Expr<'static, S, Self::Typ> {
        let idx = self.inner.idx;

        Expr::adhoc_promise(
            move |_| sea_query::Expr::val(idx),
            false, // table row is proof of existence
        )
    }
}

/// This makes it possible to use TableRow as a parameter in
/// rusqlite queries and statements.
impl<T: Table> rusqlite::ToSql for TableRow<T> {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        self.inner.idx.to_sql()
    }
}
