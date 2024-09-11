use std::{marker::PhantomData, ops::Deref};

use ref_cast::RefCast;
use rusqlite::Transaction;

use crate::{
    client::private_exec,
    exec::Query,
    insert::{private_try_insert, Writable},
    private::Dummy,
    token::ThreadToken,
    Free, HasId,
};

/// The primary interface to the database.
/// It allows creating read and write transactions from multiple threads.
/// It is also safe to create multiple [Database] instances for the same database (from one or multiple processes).
///
/// Sqlite is configured to be in [WAL mode](https://www.sqlite.org/wal.html). The effect of this mode is that there can be any number of readers with one concurrent writer.
/// What is nice about this is that a [ReadTransaction] can always be made immediately.
/// Making a [WriteTransaction] has to wait until all other [WriteTransaction]s are finished.
///
/// From the perspective of a [ReadTransaction] each [WriteTransaction] is fully applied or not at all.
/// Futhermore, [WriteTransaction]s have a global order.
/// So if we have mutations A and then B, it is impossible to see the effect of B without seeing the effect of A.
///
/// Sqlite is also configured with [`synchronous=NORMAL`](https://www.sqlite.org/pragma.html#pragma_synchronous). This gives better performance by fsyncing less.
/// The database will not lose transactions due to application crashes, but it might due to system crashes or power loss.
pub struct Database<S> {
    pub(crate) manager: r2d2_sqlite::SqliteConnectionManager,
    pub(crate) schema: PhantomData<S>,
}

impl<S> Database<S> {
    /// Create a [ReadTransaction]. This operation always completes immediately as it does not need to wait on other transactions.
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

    /// Create a [WriteTransaction].
    /// This operation needs to wait for all other [WriteTransaction]s for this database to be finished.
    ///
    /// The implementation uses the [unlock_notify](https://sqlite.org/unlock_notify.html) feature of sqlite.
    /// This makes it work across processes.
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
    pub fn query_one<T>(&self, val: impl for<'x> Dummy<'x, 't, S, Out = T>) -> T {
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
