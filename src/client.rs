use std::marker::PhantomData;

use crate::{ast::MySelect, exec::Query, query::Rows};

/// Extension trait to use this library with [rusqlite::Connection] directly.
pub(crate) trait QueryBuilder {
    fn new_query<'s, F, R, S>(&'s self, f: F) -> R
    where
        F: for<'a> FnOnce(&'a mut Query<'s, 'a, S>) -> R;
}

impl QueryBuilder for rusqlite::Transaction<'_> {
    fn new_query<'s, F, R, S>(&'s self, f: F) -> R
    where
        F: for<'a> FnOnce(&'a mut Query<'s, 'a, S>) -> R,
    {
        private_exec(self, f)
    }
}

/// For internal use only as it does not have required bounds
pub(crate) fn private_exec<'s, F, R, S>(conn: &rusqlite::Connection, f: F) -> R
where
    F: for<'a> FnOnce(&'a mut Query<'s, 'a, S>) -> R,
{
    let mut ast = MySelect::default();
    let q = Rows {
        phantom: PhantomData,
        ast: &mut ast,
    };
    f(&mut Query {
        q,
        phantom: PhantomData,
        conn,
    })
}
