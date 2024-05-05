use std::marker::PhantomData;

use rusqlite::config::DbConfig;

use crate::{ast::MySelect, Exec, Query};

pub struct Client {
    inner: rusqlite::Connection,
}

impl Client {
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

    pub fn execute_batch(&self, sql: &str) {
        self.inner.execute_batch(sql).unwrap();
    }

    pub fn new_query<'s, F, R>(&'s self, f: F) -> R
    where
        F: for<'a> FnOnce(&'a mut Exec<'s, 'a>) -> R,
    {
        self.inner.new_query(f)
    }
}

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
