use std::{marker::PhantomData, ops::Deref, sync::Arc};

use ref_cast::RefCast;
use rusqlite::{Connection, Transaction};

use crate::{
    client::private_exec,
    exec::Execute,
    insert::{private_try_insert, Writable},
    private::FromRow,
    token::ThreadToken,
    Free, HasId,
};

/// For each opened database there exists one [WriteClient].
///
/// [WriteClient] dereferences to a [ReadClient] which can be cloned.
pub struct WriteClient<T> {
    pub(crate) snapshot: ReadClient<T>,
}

impl<T> Deref for WriteClient<T> {
    type Target = ReadClient<T>;

    fn deref(&self) -> &Self::Target {
        &self.snapshot
    }
}

pub struct ReadClient<S> {
    pub(crate) manager: Arc<r2d2_sqlite::SqliteConnectionManager>,
    pub(crate) conn: Connection,
    pub(crate) schema: PhantomData<S>,
}

impl<S> Clone for ReadClient<S> {
    fn clone(&self) -> Self {
        use r2d2::ManageConnection;
        Self {
            conn: self.manager.connect().unwrap(),
            manager: self.manager.clone(),
            schema: self.schema.clone(),
        }
    }
}

impl<S> ReadClient<S> {
    /// Take a read-only snapshot of the database.
    pub fn read<'a>(&'a self, _token: &'a mut ThreadToken) -> ReadTransaction<'a, S> {
        ReadTransaction {
            // this can not be a nested transaction, because we create it from the original connection.
            // we also know that it is not concurrent with any write transactions on the same connection.
            // (sqlite does not guarantee isolation for those)
            transaction: self.conn.unchecked_transaction().unwrap(),
            _p: PhantomData,
            _local: PhantomData,
        }
    }
}

impl<S> WriteClient<S> {
    /// Claim write access to the database.
    pub fn write<'a>(&'a mut self, _token: &'a mut ThreadToken) -> WriteTransaction<'a, S> {
        let connection = &mut self.snapshot.conn;
        WriteTransaction(ReadTransaction {
            transaction: connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap(),
            _p: PhantomData,
            _local: PhantomData,
        })
    }
}

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
pub struct WriteTransaction<'a, S>(ReadTransaction<'a, S>);

impl<'a, S> Deref for WriteTransaction<'a, S> {
    type Target = ReadTransaction<'a, S>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S> ReadTransaction<'_, S> {
    /// Execute a new query.
    pub fn exec<'s, F, R>(&'s self, f: F) -> R
    where
        F: for<'a> FnOnce(&'a mut Execute<'s, 'a, S>) -> R,
    {
        // Execution already happens in a transaction.
        // [Snapshot] and thus any [Latest] that it might be borrowed
        // from are borrowed immutably, so the rows can not change.
        private_exec(&self.transaction, f)
    }

    /// Retrieve a single row from the database.
    /// This is convenient but quite slow.
    pub fn get<'s, T>(&'s self, val: impl for<'x> FromRow<'x, 's, S, Out = T>) -> T {
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
    #[allow(clippy::needless_lifetimes)]
    pub fn try_insert<'s, T: HasId>(
        &'s mut self,
        val: impl Writable<T = T>,
    ) -> Option<Free<'s, T>> {
        private_try_insert(&self.0.transaction, val)
    }

    pub fn commit(self) {
        self.0.transaction.commit().unwrap()
    }

    // pub fn update(&mut self) {
    //     todo!()
    // }

    // pub fn delete(self) {
    //     todo!()
    // }
}
