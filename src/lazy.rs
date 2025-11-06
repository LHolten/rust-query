use std::{cell::OnceCell, ops::Deref};

use crate::{IntoExpr, Table, TableRow, Transaction, value::SecretFromSql};

// this wrapper exists to make `id` immutable
pub struct Lazy<'transaction, T: Table>(pub(crate) LazyInner<'transaction, T>);

impl<'transaction, T: Table> Clone for Lazy<'transaction, T> {
    fn clone(&self) -> Self {
        Self(LazyInner {
            id: self.id,
            lazy: OnceCell::new(),
            txn: self.txn,
        })
    }
}

// TODO: works for any schema for some reason (just like TableRow)
impl<'transaction, T: Table, S> IntoExpr<'static, S> for Lazy<'transaction, T> {
    type Typ = T;

    fn into_expr(self) -> crate::Expr<'static, S, Self::Typ> {
        self.id.into_expr()
    }
}

impl<'transaction, T: Table> Deref for Lazy<'transaction, T> {
    type Target = LazyInner<'transaction, T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct LazyInner<'transaction, T: Table> {
    pub id: TableRow<T>,
    pub(crate) lazy: OnceCell<Box<T::Lazy<'transaction>>>,
    pub(crate) txn: &'transaction Transaction<T::Schema>,
}

impl<'transaction, T: Table> Deref for LazyInner<'transaction, T> {
    type Target = T::Lazy<'transaction>;

    fn deref(&self) -> &Self::Target {
        self.lazy
            .get_or_init(|| Box::new(T::get_lazy(self.txn, self.id)))
    }
}

impl<'transaction, T: Table> SecretFromSql for Lazy<'transaction, T> {
    fn from_sql(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        Ok(Self(LazyInner {
            id: TableRow::from_sql(value)?,
            lazy: OnceCell::new(),
            txn: Transaction::new_ref(),
        }))
    }
}
