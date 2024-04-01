use std::marker::PhantomData;

use elsa::FrozenVec;

use crate::{
    ast::{Joins, MySelect},
    value::MyAlias,
    Query,
};

pub struct Client {
    inner: rusqlite::Connection,
}

impl Client {
    pub fn open_in_memory() -> Self {
        let inner = rusqlite::Connection::open_in_memory().unwrap();
        Client { inner }
    }

    pub fn execute_batch(&self, sql: &str) {
        self.inner.execute_batch(sql).unwrap();
    }

    pub fn new_query<F, R>(&self, f: F) -> R
    where
        F: for<'a, 'names> FnOnce(&'names mut Query<'names, 'a>) -> R,
    {
        self.inner.new_query(f)
    }
}

pub trait QueryBuilder {
    fn new_query<F, R>(&self, f: F) -> R
    where
        F: for<'a, 'names> FnOnce(&'names mut Query<'names, 'a>) -> R;
}

impl QueryBuilder for rusqlite::Connection {
    fn new_query<F, R>(&self, f: F) -> R
    where
        F: for<'a, 'names> FnOnce(&'names mut Query<'names, 'a>) -> R,
    {
        let ast = MySelect::default();
        let joins = Joins {
            table: MyAlias::new(),
            joined: FrozenVec::new(),
        };
        let mut q = Query {
            phantom: PhantomData,
            phantom2: PhantomData,
            ast: &ast,
            joins: &joins,
            client: self,
        };
        f(&mut q)
    }
}
