use std::marker::PhantomData;

use crate::{ast::MySelect, exec::Execute, query::Query};

/// This is a wrapper for [rusqlite::Connection].
/// It's main purpose is to remove the need to depend on rusqlite in the future.
#[derive(Debug)]
pub struct Client {
    pub(crate) inner: rusqlite::Connection,
}

impl Client {
    /// Execute a new query.
    pub fn exec<'s, F, R>(&'s self, f: F) -> R
    where
        F: for<'a> FnOnce(&'a mut Execute<'s, 'a>) -> R,
    {
        self.inner.new_query(f)
    }
}

/// Extension trait to use this library with [rusqlite::Connection] directly.
pub(crate) trait QueryBuilder {
    fn new_query<'s, F, R>(&'s self, f: F) -> R
    where
        F: for<'a> FnOnce(&'a mut Execute<'s, 'a>) -> R;
}

impl QueryBuilder for rusqlite::Connection {
    fn new_query<'s, F, R>(&'s self, f: F) -> R
    where
        F: for<'a> FnOnce(&'a mut Execute<'s, 'a>) -> R,
    {
        let ast = MySelect::default();
        let q = Query {
            phantom: PhantomData,
            ast: &ast,
            client: self,
        };
        f(&mut Execute {
            q,
            phantom: PhantomData,
        })
    }
}
