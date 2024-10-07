use std::{marker::PhantomData, ops::Deref};

use ref_cast::RefCast;

use crate::{
    client::private_exec,
    exec::Query,
    insert::{private_try_insert, Writable},
    private::Dummy,
    token::ThreadToken,
    Table, TableRow,
};

/// The primary interface to the database.
/// It allows creating read and write transactions from multiple threads.
/// It is also safe to create multiple [Database] instances for the same database (from one or multiple processes).
///
/// Sqlite is configured to be in [WAL mode](https://www.sqlite.org/wal.html).
/// The effect of this mode is that there can be any number of readers with one concurrent writer.
/// What is nice about this is that a [Transaction] can always be made immediately.
/// Making a [TransactionMut] has to wait until all other [TransactionMut]s are finished.
///
/// Sqlite is also configured with [`synchronous=NORMAL`](https://www.sqlite.org/pragma.html#pragma_synchronous). This gives better performance by fsyncing less.
/// The database will not lose transactions due to application crashes, but it might due to system crashes or power loss.
///
/// # Creating transactions
/// Creating a transaction requires access to a [ThreadToken].
/// This makes it impossible to create two transactions on the same thread, making it impossible to accidentally share a [TableRow] outside of the transaction that it was created in.
///
pub struct Database<S> {
    pub(crate) manager: r2d2_sqlite::SqliteConnectionManager,
    pub(crate) schema: PhantomData<S>,
}

impl<S> Database<S> {
    /// Create a [Transaction]. This operation always completes immediately as it does not need to wait on other transactions.
    pub fn read<'a>(&'a self, token: &'a mut ThreadToken) -> Transaction<'a, S> {
        use r2d2::ManageConnection;
        let conn = token.conn.insert(self.manager.connect().unwrap());
        Transaction {
            // this can not be a nested transaction, because we create it from the original connection.
            // we also know that it is not concurrent with any write transactions on the same connection.
            // (sqlite does not guarantee isolation for those)
            transaction: conn.unchecked_transaction().unwrap(),
            _p: PhantomData,
            _local: PhantomData,
        }
    }

    /// Create a [TransactionMut].
    /// This operation needs to wait for all other [TransactionMut]s for this database to be finished.
    ///
    /// The implementation uses the [unlock_notify](https://sqlite.org/unlock_notify.html) feature of sqlite.
    /// This makes it work across processes.
    pub fn write_lock<'a>(&'a self, token: &'a mut ThreadToken) -> TransactionMut<'a, S> {
        use r2d2::ManageConnection;
        let conn = token.conn.insert(self.manager.connect().unwrap());
        TransactionMut {
            inner: Transaction {
                transaction: conn
                    .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                    .unwrap(),
                _p: PhantomData,
                _local: PhantomData,
            },
        }
    }
}

/// [Transaction] can be used to query the database.
///
/// From the perspective of a [Transaction] each [TransactionMut] is fully applied or not at all.
/// Futhermore, the effects of [TransactionMut]s have a global order.
/// So if we have mutations `A` and then `B`, it is impossible for a [Transaction] to see the effect of `B` without seeing the effect of `A`.
///
/// All [TableRow] references retrieved from the database live for at most `'a`.
/// This makes these references effectively local to this [Transaction].
#[derive(RefCast)]
#[repr(transparent)]
pub struct Transaction<'a, S> {
    pub(crate) transaction: rusqlite::Transaction<'a>,
    pub(crate) _p: PhantomData<&'a S>,
    pub(crate) _local: PhantomData<ThreadToken>,
}

/// Same as [Transaction], but allows inserting new rows.
///
/// [TransactionMut] always uses the latest version of the database, with the effects of all previous [TransactionMut]s applied.
///
/// To make mutations to the database permanent you need to use [TransactionMut::commit].
/// This is to make sure that if a function panics while holding a mutable transaction, it will roll back those changes.
pub struct TransactionMut<'a, S> {
    inner: Transaction<'a, S>,
}

impl<'a, S> Deref for TransactionMut<'a, S> {
    type Target = Transaction<'a, S>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'t, S> Transaction<'t, S> {
    /// Execute a query with multiple results.
    ///
    /// Please take a look at the documentation of [Query] for how to use it.
    pub fn query<F, R>(&self, f: F) -> R
    where
        F: for<'a> FnOnce(&'a mut Query<'t, 'a, S>) -> R,
    {
        // Execution already happens in a [Transaction].
        // and thus any [TransactionMut] that it might be borrowed
        // from are borrowed immutably, so the rows can not change.
        private_exec(&self.transaction, f)
    }

    /// Retrieve a single result from the database.
    ///
    /// Instead of using [Self::query_one] in a loop, it is better to
    /// call [Self::query] and return all results at once.
    pub fn query_one<O>(&self, val: impl Dummy<'static, 't, S, Out = O>) -> O
    where
        S: 'static,
    {
        // Theoretically this doesn't even need to be in a transaction.
        // We already have one though, so we must use it.
        let mut res = private_exec(&self.transaction, |e| {
            // Cast the static lifetime to any lifetime necessary, this is fine because we know the static lifetime
            // can not be guaranteed by a query scope.
            e.into_vec_private(val)
        });
        res.pop().unwrap()
    }
}

impl<S> TransactionMut<'_, S> {
    /// Try inserting a value into the database.
    ///
    /// Returns a reference to the new inserted value or `None` if there is a conflict.
    /// Conflicts can occur due too unique constraints in the schema.
    ///
    /// The method takes a mutable reference so that it can not be interleaved with a multi row query.
    pub fn try_insert<'s, T: Table>(
        &mut self,
        val: impl Writable<T = T>,
    ) -> Option<TableRow<'s, T>> {
        private_try_insert(&self.inner.transaction, val)
    }

    /// Make the changes made in this [TransactionMut] permanent.
    ///
    /// If the [TransactionMut] is dropped without calling this function, then the changes are rolled back.
    pub fn commit(self) {
        self.inner.transaction.commit().unwrap()
    }

    // pub fn update(&mut self) {
    //     todo!()
    // }

    // pub fn delete(self) {
    //     todo!()
    // }
}
