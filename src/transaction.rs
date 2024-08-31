use std::{marker::PhantomData, ops::Deref};

use ref_cast::RefCast;
use rusqlite::Transaction;

use crate::{
    client::private_exec,
    exec::Query,
    insert::{private_try_insert, Writable},
    private::FromRow,
    token::ThreadToken,
    Free, HasId,
};

/// The primary interface to the database.
/// It allows creating read and write transactions from multiple threads.
pub struct Database<S> {
    pub(crate) manager: r2d2_sqlite::SqliteConnectionManager,
    pub(crate) schema: PhantomData<S>,
}

impl<S> Database<S> {
    /// Take a read-only snapshot of the database.
    ///
    /// This does not block because sqlite in WAL mode allows reading while writing.
    pub fn read<'a>(&'a self, token: &'a mut ThreadToken) -> ReadTransaction<'a, S> {
        use r2d2::ManageConnection;
        let conn = token.conn.insert(self.manager.connect().unwrap());
        ReadTransaction {
            // this can not be a nested transaction, because we create it from the original connection.
            // we also know that it is not concurrent with any write transactions on the same connection.
            // (sqlite does not guarantee isolation for those)
            transaction: conn.unchecked_transaction().unwrap(),
            _p: PhantomData,
            _local: PhantomData,
        }
    }

    /// Claim write access to the database.
    /// This will block until it can acquire a write transaction.
    ///
    /// This function uses <https://sqlite.org/unlock_notify.html> to wait.
    pub fn write_lock<'a>(&'a self, token: &'a mut ThreadToken) -> WriteTransaction<'a, S> {
        use r2d2::ManageConnection;
        let conn = token.conn.insert(self.manager.connect().unwrap());
        WriteTransaction {
            inner: ReadTransaction {
                transaction: conn
                    .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                    .unwrap(),
                _p: PhantomData,
                _local: PhantomData,
            },
        }
    }
}

/// [ReadTransaction] allows querying the database.
///
/// There can be at most one [ReadTransaction] or [WriteTransaction] in each thread.
/// This is why these types are both `!Send`.
///
/// All [Free] row id references in this snapshot live for at most `'a`.
#[derive(RefCast)]
#[repr(transparent)]
pub struct ReadTransaction<'a, S> {
    transaction: Transaction<'a>,
    _p: PhantomData<&'a S>,
    _local: PhantomData<ThreadToken>,
}

/// Same as [ReadTransaction], but also allows inserting new rows.
pub struct WriteTransaction<'a, S> {
    inner: ReadTransaction<'a, S>,
}

impl<'a, S> Deref for WriteTransaction<'a, S> {
    type Target = ReadTransaction<'a, S>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'t, S> ReadTransaction<'t, S> {
    /// Execute a new query.
    pub fn query<F, R>(&self, f: F) -> R
    where
        F: for<'a> FnOnce(&'a mut Query<'t, 'a, S>) -> R,
    {
        // Execution already happens in a transaction.
        // [Snapshot] and thus any [Latest] that it might be borrowed
        // from are borrowed immutably, so the rows can not change.
        private_exec(&self.transaction, f)
    }

    /// Retrieve a single row from the database.
    /// This is convenient but quite slow.
    pub fn get<T>(&self, val: impl for<'x> FromRow<'x, 't, S, Out = T>) -> T {
        // Theoretically this doesn't even need to be in a transaction.
        // We already have one though, so we must use it.
        let mut res = private_exec(&self.transaction, |e| e.into_vec(val));
        res.pop().unwrap()
    }
}

impl<S> WriteTransaction<'_, S> {
    /// Try inserting a value into the database.
    /// Returns a reference to the new inserted value or `None` if there is a conflict.
    /// Takes a mutable reference so that it can not be interleaved with a multi row query.
    pub fn try_insert<'s, T: HasId>(&mut self, val: impl Writable<T = T>) -> Option<Free<'s, T>> {
        private_try_insert(&self.inner.transaction, val)
    }

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
