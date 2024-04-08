use std::marker::PhantomData;

use elsa::FrozenVec;
use rusqlite::config::DbConfig;

use crate::{
    ast::{Joins, MySelect},
    value::MyAlias,
    Exec, Query,
};

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

    pub fn new_query<F, R>(&self, f: F) -> R
    where
        F: for<'a, 'names> FnOnce(&'names mut Exec<'names, 'a>) -> R,
    {
        self.inner.new_query(f)
    }
}

pub trait QueryBuilder {
    fn new_query<F, R>(&self, f: F) -> R
    where
        F: for<'a, 'names> FnOnce(&'names mut Exec<'names, 'a>) -> R;
}

impl QueryBuilder for rusqlite::Connection {
    fn new_query<F, R>(&self, f: F) -> R
    where
        F: for<'a, 'names> FnOnce(&'names mut Exec<'names, 'a>) -> R,
    {
        let ast = MySelect::default();
        let joins = Joins {
            table: MyAlias::new(),
            joined: FrozenVec::new(),
        };
        let q = Query {
            phantom: PhantomData,
            phantom2: PhantomData,
            ast: &ast,
            joins: &joins,
            client: self,
        };
        f(&mut Exec { q })
    }
}
