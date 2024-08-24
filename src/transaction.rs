use std::{cell::Cell, marker::PhantomData, ops::Deref, ptr::null, sync::Arc};

use rusqlite::Transaction;

use crate::{
    client::{private_exec, Client},
    exec::Execute,
    insert::{private_try_insert, Writable},
    migrate::Schema,
    private::FromRow,
    Free, HasId,
};

/// Only one [ThreadToken] exists in each thread.
/// It can thus not be send across threads.
pub struct ThreadToken<T>(*const T);

thread_local! {
    static EXISTS: Cell<bool> = const { Cell::new(true) };
}

impl ThreadToken<()> {
    /// Retrieve the [ThreadToken] if it was not retrieved yet
    pub fn acquire() -> Option<Self> {
        EXISTS.replace(false).then_some(ThreadToken(null()))
    }

    pub fn release(self) {
        EXISTS.set(true)
    }

    /// Change which schema is usable in the current thread.
    pub fn schema<S: Schema>(self) -> (ThreadToken<S>, S) {
        todo!()
    }
}

impl<T> ThreadToken<T> {
    /// Change which schema is usable in the current thread.
    pub fn finish(self, _: T) -> ThreadToken<()> {
        ThreadToken(null())
    }
}

pub struct DbClient<T> {
    pub latest: LatestToken<T>,
    pub snapshot: SnapshotToken<T>,
}

/// For each opened database there exists one [LatestToken].
pub struct LatestToken<T>(pub(crate) SnapshotToken<T>);

impl<T> Deref for LatestToken<T> {
    type Target = SnapshotToken<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// For each opened database there exist one [SnapshotToken].
pub struct SnapshotToken<T> {
    pub(crate) client: Arc<Client>,
    pub(crate) schema: T,
}

impl<T> SnapshotToken<T> {
    /// Take a read-only snapshot of the database.
    pub fn snapshot<'a>(&'a self, token: &'a mut ThreadToken<T>) -> Snapshot<'a> {
        todo!()
    }
}

impl<T> LatestToken<T> {
    /// Claim write access to the database.
    pub fn latest<'a>(&'a mut self, token: &'a mut ThreadToken<T>) -> Latest<'a> {
        todo!()
    }
}

/// There can be at most one [Snapshot] for [Latest] in each thread.
/// This is why these types are both !Send.
/// All rows in this snapshot live for at least `'a`.
#[derive(Clone, Copy)]
pub struct Snapshot<'a>(&'a Transaction<'a>, PhantomData<&'a mut ThreadToken<()>>);
pub struct Latest<'a>(Snapshot<'a>);

impl<'a> Deref for Latest<'a> {
    type Target = Snapshot<'a>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Snapshot<'_> {
    /// Execute a new query.
    pub fn exec<'s, F, R>(&'s self, f: F) -> R
    where
        F: for<'a> FnOnce(&'a mut Execute<'s, 'a>) -> R,
    {
        // Execution already happens in a transaction.
        // [Snapshot] and thus any [Latest] that it might be borrowed
        // from are borrowed immutably, so the rows can not change.
        private_exec(&self.0, f)
    }

    /// Retrieve a single row from the database.
    /// This is convenient but quite slow.
    pub fn get<'s, T>(&'s self, val: impl for<'x> FromRow<'x, 's, Out = T>) -> T {
        // Theoretically this doesn't even need to be in a transaction.
        // We already have one though, so we must use it.
        let mut res = private_exec(&self.0, |e| e.into_vec(val));
        res.pop().unwrap()
    }
}

impl Latest<'_> {
    /// Try inserting a value into the database.
    /// Returns a reference to the new inserted value or `None` if there is a conflict.
    /// Takes a mutable reference so that it can not be interleaved with a multi row query.
    #[allow(clippy::needless_lifetimes)]
    pub fn try_insert<'s, T: HasId>(
        &'s mut self,
        val: impl Writable<T = T>,
    ) -> Option<Free<'s, T>> {
        private_try_insert(&self.0 .0, val)
    }

    // pub fn update(&mut self) {
    //     todo!()
    // }

    // pub fn delete(self) {
    //     todo!()
    // }
}
