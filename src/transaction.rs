use std::{any::Any, cell::Cell, marker::PhantomData, ops::Deref, rc::Rc, sync::Arc};

use ref_cast::RefCast;
use rusqlite::{Connection, Transaction};

use crate::{
    client::{private_exec, Client},
    exec::Execute,
    insert::{private_try_insert, Writable},
    private::FromRow,
    Free, HasId,
};

/// Only one [ThreadToken] exists in each thread.
/// It can thus not be send across threads.
pub struct ThreadToken {
    _p: PhantomData<*const ()>,
    pub(crate) stuff: Rc<dyn Any>,
}

thread_local! {
    static EXISTS: Cell<bool> = const { Cell::new(true) };
}

impl ThreadToken {
    /// Retrieve the [ThreadToken] if it was not retrieved yet
    pub fn acquire() -> Option<Self> {
        EXISTS.replace(false).then_some(ThreadToken {
            _p: PhantomData,
            stuff: Rc::new(()),
        })
    }
}

impl Drop for ThreadToken {
    fn drop(&mut self) {
        EXISTS.set(true)
    }
}

/// For each opened database there exists one [LatestToken].
pub struct LatestToken<T> {
    pub(crate) snapshot: SnapshotToken<T>,
}

impl<T> Deref for LatestToken<T> {
    type Target = SnapshotToken<T>;

    fn deref(&self) -> &Self::Target {
        &self.snapshot
    }
}

pub struct SnapshotToken<S> {
    pub(crate) client: Arc<Client>,
    pub(crate) conn: Connection,
    pub(crate) schema: PhantomData<S>,
}

impl<S> Clone for SnapshotToken<S> {
    fn clone(&self) -> Self {
        use r2d2::ManageConnection;
        let conn = self.client.manager.connect().unwrap();
        Self {
            client: self.client.clone(),
            conn,
            schema: self.schema.clone(),
        }
    }
}

impl<S> SnapshotToken<S> {
    /// Take a read-only snapshot of the database.
    pub fn snapshot<'a>(&'a mut self, _token: &'a mut ThreadToken) -> Snapshot<'a, S> {
        Snapshot {
            transaction: self.conn.transaction().unwrap(),
            _p: PhantomData,
            _local: PhantomData,
        }
    }
}

impl<S> LatestToken<S> {
    /// Claim write access to the database.
    pub fn latest<'a>(&'a mut self, _token: &'a mut ThreadToken) -> Latest<'a, S> {
        let connection = &mut self.snapshot.conn;
        Latest(Snapshot {
            transaction: connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap(),
            _p: PhantomData,
            _local: PhantomData,
        })
    }
}

/// There can be at most one [Snapshot] or [Latest] in each thread.
/// This is why these types are both !Send.
/// All rows in this snapshot live for at most `'a`.
#[derive(RefCast)]
#[repr(transparent)]
pub struct Snapshot<'a, S> {
    transaction: Transaction<'a>,
    _p: PhantomData<&'a S>,
    _local: PhantomData<*const ()>,
}

/// Same as [Snapshot], but allows inserting new rows
pub struct Latest<'a, S>(Snapshot<'a, S>);

impl<'a, S> Deref for Latest<'a, S> {
    type Target = Snapshot<'a, S>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S> Snapshot<'_, S> {
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

impl<S> Latest<'_, S> {
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

    // pub fn update(&mut self) {
    //     todo!()
    // }

    // pub fn delete(self) {
    //     todo!()
    // }
}
