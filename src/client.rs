use std::marker::PhantomData;

use rusqlite::config::DbConfig;

use crate::{ast::MySelect, Exec, Query};

/// This is a wrapper for [rusqlite::Connection].
/// It's main purpose is to remove the need to depend on rusqlite in the future.
/// Right now it is mostly used in the tests.
pub struct Client {
    inner: rusqlite::Connection,
}

impl Client {
    /// Create a new client with recommended settings.
    pub fn open_in_memory() -> Self {
        let inner = rusqlite::Connection::open_in_memory().unwrap();
        inner.pragma_update(None, "journal_mode", "WAL").unwrap();
        inner.pragma_update(None, "synchronous", "NORMAL").unwrap();
        inner.pragma_update(None, "foreign_keys", "ON").unwrap();
        inner
            .set_db_config(DbConfig::SQLITE_DBCONFIG_DQS_DDL, false)
            .unwrap();
        inner
            .set_db_config(DbConfig::SQLITE_DBCONFIG_DQS_DML, false)
            .unwrap();

        Client { inner }
    }

    /// Execute a raw sql statement, useful for loading a schema.
    pub fn execute_batch(&self, sql: &str) {
        self.inner.execute_batch(sql).unwrap();
    }

    /// Execute a new query.
    pub fn new_query<'s, F, R>(&'s self, f: F) -> R
    where
        F: for<'a> FnOnce(&'a mut Exec<'s, 'a>) -> R,
    {
        self.inner.new_query(f)
    }
}

/// Extension trait to use this library with [rusqlite::Connection] directly.
pub trait QueryBuilder {
    fn new_query<'s, F, R>(&'s self, f: F) -> R
    where
        F: for<'a> FnOnce(&'a mut Exec<'s, 'a>) -> R;
}

impl QueryBuilder for rusqlite::Connection {
    fn new_query<'s, F, R>(&'s self, f: F) -> R
    where
        F: for<'a> FnOnce(&'a mut Exec<'s, 'a>) -> R,
    {
        let ast = MySelect::default();
        let q = Query {
            phantom: PhantomData,
            ast: &ast,
            client: self,
        };
        f(&mut Exec {
            q,
            phantom: PhantomData,
        })
    }
}
