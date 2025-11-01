use std::{cell::OnceCell, ops::Deref};

use crate::{FromExpr, IntoExpr, IntoSelect, Table, TableRow, Transaction, value::SecretFromSql};

// this wrapper exists to make `id` immutable
pub struct Lazy<'transaction, T: Table>(LazyInner<'transaction, T>);

// TODO: works for any schema for some reason (just like TableRow)
impl<'transaction, T: Table, S> IntoExpr<'static, S> for Lazy<'transaction, T> {
    type Typ = T;

    fn into_expr(self) -> crate::Expr<'static, S, Self::Typ> {
        self.id.into_expr()
    }
}

impl<'transaction, T: Table> FromExpr<T::Schema, T> for Lazy<'static, T> {
    fn from_expr<'columns>(
        col: impl crate::IntoExpr<'columns, T::Schema, Typ = T>,
    ) -> crate::Select<'columns, T::Schema, Self> {
        col.into_expr().into_select()
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
    lazy: OnceCell<Box<T::Row>>,
    txn: &'transaction Transaction<T::Schema>,
}

impl<'transaction, T: Table> Deref for LazyInner<'transaction, T>
where
    T::Row: FromExpr<T::Schema, T>,
{
    type Target = T::Row;

    fn deref(&self) -> &Self::Target {
        use crate::FromExpr;
        self.lazy
            .get_or_init(|| Box::new(self.txn.query_one(T::Row::from_expr(self.id))))
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
