use std::{cell::OnceCell, ops::Deref};

#[cfg(doc)]
use crate::FromExpr;
use crate::{Table, TableRow, Transaction, value::SecretFromSql};

/// [Lazy] can be used to read any column of a table row and its parents.
/// Columns are loaded on demand, one row at at time.
///
/// As an example, if you have two tables `Post` and `User`:
/// ```
/// # #[rust_query::migration::schema(Schema)]
/// # pub mod vN {
/// #     pub struct Post {
/// #         pub author: User,
/// #     }
/// #     pub struct User {
/// #         pub name: String,
/// #     }
/// # }
/// # use rust_query::Lazy;
/// # use v0::*;
/// fn foo(post: Lazy<Post>) {
///     let user = &post.author; // If the `post` row was not retrieved yet, then it is retrieved now to read the `user` column.
///     let user_id = user.table_row(); // This doesn't access the database because the `user` id was already read from the `post` row.
///     let user_name = &user.name; // If the `user` row was not retrieved yet, then it is retrieved now to read the `name` column.
/// }
/// ```
///
/// Note that [Lazy] borrows the transaction immutably.
/// This means that it is not possible to keep a [Lazy] value when doing inserts or updates.
/// Here are some alternatives to solve this problem:
/// - [Copy]/[Clone] the columns that you need from the [Lazy] value before doing inserts and or updates.
/// - Another option is to use [Lazy::table_row] to retrieve an owned [TableRow].
///   This can then be used to create [crate::Expr] referencing the table columns for use in queries.
/// - If you need many columns in a struct, then consider [derive@crate::FromExpr].
pub struct Lazy<'transaction, T: Table> {
    pub(crate) id: TableRow<T>,
    pub(crate) lazy: OnceCell<Box<T::Lazy<'transaction>>>,
    pub(crate) txn: &'transaction Transaction<T::Schema>,
}

impl<'transaction, T: Table> Lazy<'transaction, T> {
    /// Get an owned [TableRow] out of this [Lazy] value.
    ///
    /// If you don't care about deleting the row then you probably want to
    /// immediately use [TableRow::into_expr](crate::FromExpr::from_expr) on the returned [TableRow].
    pub fn table_row(&self) -> TableRow<T> {
        self.id
    }
}

impl<'transaction, T: Table> Clone for Lazy<'transaction, T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            lazy: OnceCell::new(),
            txn: self.txn,
        }
    }
}

impl<'transaction, T: Table> Deref for Lazy<'transaction, T> {
    type Target = T::Lazy<'transaction>;

    fn deref(&self) -> &Self::Target {
        self.lazy
            .get_or_init(|| Box::new(T::get_lazy(self.txn, self.id)))
    }
}

impl<'transaction, T: Table> SecretFromSql for Lazy<'transaction, T> {
    fn from_sql(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        Ok(Self {
            id: TableRow::from_sql(value)?,
            lazy: OnceCell::new(),
            txn: Transaction::new_ref(),
        })
    }
}
