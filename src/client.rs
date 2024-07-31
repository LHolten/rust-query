use std::{
    cell::OnceCell,
    marker::PhantomData,
    sync::{Condvar, Mutex},
};

use rusqlite::Transaction;

use crate::{
    ast::MySelect,
    exec::Execute,
    insert::{private_try_insert, Writable},
    query::Query,
    value::{Covariant, MyTyp},
    HasId, Just,
};

pub struct Client {
    manager: r2d2_sqlite::SqliteConnectionManager,
    cvar: Condvar,
    updates: Mutex<bool>,
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
    /// Execute a new query.
    pub fn exec<'s, F, R>(&'s self, f: F) -> R
    where
        F: for<'a> FnOnce(&'a mut Execute<'s, 'a>) -> R,
    {
        use r2d2::ManageConnection;
        // Select needs it's own connection for isolation, because
        // sqlite does not provide isolation within a connection.
        let mut conn = self.manager.connect().unwrap();
        // Additionally, because rows have dynamic columns,
        // we need to be able to do multiple selects on the same snapshot.
        // For this we need a transaction.
        let transaction = conn.transaction().unwrap();
        private_exec(&transaction, f)
    }

    /// Retrieve a single value from the database.
    /// This is convenient but quite slow.
    pub fn get<'s, T: MyTyp>(&'s self, val: impl Covariant<'s, Typ = T>) -> T::Out<'s> {
        // TODO: would not need it's own connection if I made rows have fixed columns
        self.exec(|e| e.into_vec(move |row| row.get(val.clone().weaken())))
            .pop()
            .unwrap()
    }

    /// Try inserting a value into the database.
    /// Returns a reference to the new inserted value or `None` if there is a conflict.
    pub fn try_insert<'s, T: HasId>(
        &'s self,
        val: impl Writable<'s, T = T>,
    ) -> Option<Just<'s, T>> {
        use r2d2::ManageConnection;
        let res = CONN.with(|conn| {
            let conn = conn.get_or_init(|| self.manager.connect().unwrap());
            private_try_insert(conn, val)
        });
        *self.updates.lock().unwrap() = true;
        self.cvar.notify_all();
        res
    }

    /// Wait for any changes to the database.
    pub fn wait(&self) {
        let updates = self.updates.lock().unwrap();
        *self.cvar.wait_while(updates, |&mut x| !x).unwrap() = false;
    }
}

/// Extension trait to use this library with [rusqlite::Connection] directly.
pub(crate) trait QueryBuilder {
    fn new_query<'s, F, R>(&'s self, f: F) -> R
    where
        F: for<'a> FnOnce(&'a mut Execute<'s, 'a>) -> R;
}

impl QueryBuilder for rusqlite::Transaction<'_> {
    fn new_query<'s, F, R>(&'s self, f: F) -> R
    where
        F: for<'a> FnOnce(&'a mut Execute<'s, 'a>) -> R,
    {
        private_exec(self, f)
    }
}

/// For internal use only as it does not have required bounds
pub(crate) fn private_exec<'s, F, R>(conn: &Transaction, f: F) -> R
where
    F: for<'a> FnOnce(&'a mut Execute<'s, 'a>) -> R,
{
    let mut ast = MySelect::default();
    let q = Query {
        phantom: PhantomData,
        ast: &mut ast,
        conn,
    };
    f(&mut Execute {
        q,
        phantom: PhantomData,
    })
}
