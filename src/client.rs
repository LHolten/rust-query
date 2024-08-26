use std::{
    cell::OnceCell,
    marker::PhantomData,
    sync::{Condvar, Mutex},
};

use crate::{ast::MySelect, exec::Execute, query::Query};

pub struct Client {
    pub(crate) manager: r2d2_sqlite::SqliteConnectionManager,
    pub(crate) cvar: Condvar,
    pub(crate) updates: Mutex<bool>,
}

thread_local! {
    static CONN: OnceCell<rusqlite::Connection> = const { OnceCell::new() }
}

impl Client {
    pub(crate) fn new(manager: r2d2_sqlite::SqliteConnectionManager) -> Self {
        Self {
            manager,
            cvar: Condvar::new(),
            updates: Mutex::new(true),
        }
    }
}

impl Client {
    /// Wait for any changes to the database.
    pub fn wait(&self) {
        let updates = self.updates.lock().unwrap();
        *self.cvar.wait_while(updates, |&mut x| !x).unwrap() = false;
    }
}

/// Extension trait to use this library with [rusqlite::Connection] directly.
pub(crate) trait QueryBuilder {
    fn new_query<'s, F, R, S>(&'s self, f: F) -> R
    where
        F: for<'a> FnOnce(&'a mut Execute<'s, 'a, S>) -> R;
}

impl QueryBuilder for rusqlite::Transaction<'_> {
    fn new_query<'s, F, R, S>(&'s self, f: F) -> R
    where
        F: for<'a> FnOnce(&'a mut Execute<'s, 'a, S>) -> R,
    {
        private_exec(self, f)
    }
}

/// For internal use only as it does not have required bounds
pub(crate) fn private_exec<'s, F, R, S>(conn: &rusqlite::Connection, f: F) -> R
where
    F: for<'a> FnOnce(&'a mut Execute<'s, 'a, S>) -> R,
{
    let mut ast = MySelect::default();
    let q = Query {
        phantom: PhantomData,
        ast: &mut ast,
    };
    f(&mut Execute {
        q,
        phantom: PhantomData,
        conn,
    })
}
