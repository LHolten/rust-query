use std::{marker::PhantomData, ops::Deref};

use crate::{ast::MySelect, Exec, Query};

/// This is a wrapper for [rusqlite::Connection].
/// It's main purpose is to remove the need to depend on rusqlite in the future.
pub struct Client<S> {
    pub(crate) inner: rusqlite::Connection,
    pub(crate) schema: S,
}

impl<S> Deref for Client<S> {
    type Target = S;

    fn deref(&self) -> &Self::Target {
        &self.schema
    }
}

impl<S> Client<S> {
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
