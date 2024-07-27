use std::marker::PhantomData;

use ref_cast::RefCast;

use crate::{
    ast::MySelect,
    exec::Execute,
    query::Query,
    value::{Covariant, MyTyp},
    Value,
};

/// This is a wrapper for [rusqlite::Connection].
/// It's main purpose is to remove the need to depend on rusqlite in the future.
#[derive(Debug, RefCast)]
#[repr(transparent)]
pub struct Client {
    pub(crate) inner: rusqlite::Connection,
}

#[derive(Clone)]
struct Weaken<'t, T> {
    inner: T,
    _p: PhantomData<&'t ()>,
}

impl<'t, 'a: 't, T: Covariant<'a>> Value<'t> for Weaken<'a, T> {
    type Typ = T::Typ;

    fn build_expr(&self, b: crate::value::ValueBuilder) -> sea_query::SimpleExpr {
        self.inner.build_expr(b)
    }
}

impl Client {
    /// Execute a new query.
    pub fn exec<'s, F, R>(&'s self, f: F) -> R
    where
        F: for<'a> FnOnce(&'a mut Execute<'s, 'a>) -> R,
    {
        self.inner.new_query(f)
    }

    /// Retrieve a single value from the database.
    /// This is convenient but quite slow.
    pub fn get<'s, T: MyTyp>(&'s self, val: impl Covariant<'s, Typ = T>) -> T::Out<'s> {
        let weak = Weaken {
            inner: val,
            _p: PhantomData,
        };
        self.exec(|e| e.into_vec(move |row| row.get(weak.clone())))
            .pop()
            .unwrap()
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
        let mut ast = MySelect::default();
        let q = Query {
            phantom: PhantomData,
            ast: &mut ast,
            client: self,
        };
        f(&mut Execute {
            q,
            phantom: PhantomData,
        })
    }
}
