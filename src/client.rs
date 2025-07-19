use std::cell::Cell;

use self_cell::MutBorrow;

use crate::{Database, Transaction, TransactionMut, transaction::OwnedTransaction};

/// The primary interface to the database.
///
/// Only one [LocalClient] can exist in each thread and transactions need to mutably borrow a [LocalClient].
/// This makes it impossible to have access to two transactions from one thread.
///
/// The only way to have concurrent read transactions is to have them on different threads.
/// Write transactions never run in parallell with each other, but they do run in parallel with read transactions.
pub struct LocalClient {
    _p: std::marker::PhantomData<*const ()>,
}

impl LocalClient {
    /// Create a [Transaction]. This operation always completes immediately as it does not need to wait on other transactions.
    ///
    /// This function will panic if the schema was modified compared to when the [Database] value
    /// was created. This can happen for example by running another instance of your program with
    /// additional migrations.
    pub fn transaction<S>(&mut self, db: &Database<S>) -> Transaction<S> {
        use r2d2::ManageConnection;
        // TODO: could check here if the existing connection is good to use.
        let conn = db.manager.connect().unwrap();
        let owned = OwnedTransaction::new(MutBorrow::new(conn), |conn| {
            Some(conn.borrow_mut().transaction().unwrap())
        });
        Transaction::new_checked(owned, db.schema_version)
    }

    /// Create a [TransactionMut].
    /// This operation needs to wait for all other [TransactionMut]s for this database to be finished.
    ///
    /// The implementation uses the [unlock_notify](https://sqlite.org/unlock_notify.html) feature of sqlite.
    /// This makes it work across processes.
    ///
    /// Note: you can create a deadlock if you are holding on to another lock while trying to
    /// get a mutable transaction!
    ///
    /// This function will panic if the schema was modified compared to when the [Database] value
    /// was created. This can happen for example by running another instance of your program with
    /// additional migrations.
    pub fn transaction_mut<S>(&mut self, db: &Database<S>) -> TransactionMut<S> {
        use r2d2::ManageConnection;
        // TODO: could check here if the existing connection is good to use.
        // TODO: make sure that when reusing a connection, the foreign keys are checked (migration doesn't)
        // .pragma_update(None, "foreign_keys", "ON").unwrap();
        let conn = db.manager.connect().unwrap();
        let owned = OwnedTransaction::new(MutBorrow::new(conn), |conn| {
            Some(
                conn.borrow_mut()
                    .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                    .unwrap(),
            )
        });
        TransactionMut {
            inner: Transaction::new_checked(owned, db.schema_version),
        }
    }
}

thread_local! {
    static EXISTS: Cell<bool> = const { Cell::new(true) };
}

impl LocalClient {
    fn new() -> Self {
        LocalClient {
            _p: std::marker::PhantomData,
        }
    }

    /// Create a [LocalClient] if it was not created yet on this thread.
    ///
    /// Async tasks often share their thread and can thus not use this method.
    /// Instead you should use your equivalent of `spawn_blocking` or `block_in_place`.
    /// These functions guarantee that you have a unique thread and thus allow [LocalClient::try_new].
    ///
    /// Note that using `spawn_blocking` for sqlite is actually a good practice.
    /// Sqlite queries can be expensive, it might need to read from disk which is slow.
    /// Doing so on all async runtime threads would prevent other tasks from executing.
    pub fn try_new() -> Option<Self> {
        EXISTS.replace(false).then(LocalClient::new)
    }
}

impl Drop for LocalClient {
    /// Dropping a [LocalClient] allows retrieving it with [LocalClient::try_new] again.
    fn drop(&mut self) {
        EXISTS.set(true)
    }
}
