use std::{
    cell::Cell,
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use sea_query::SqliteQueryBuilder;
use sea_query_rusqlite::RusqliteBinder;

use crate::{
    dummy::{Dummy, DynDummy},
    rows::Rows,
};

/// This is the top level query type and dereferences to [Rows].
/// Most importantly it can turn the query result into a [Vec].
pub struct Query<'outer, 'inner, S> {
    pub(crate) phantom: PhantomData<&'outer ()>,
    pub(crate) q: Rows<'inner, S>,
    pub(crate) conn: &'inner rusqlite::Connection,
}

impl<'outer, 'inner, S> Deref for Query<'outer, 'inner, S> {
    type Target = Rows<'inner, S>;

    fn deref(&self) -> &Self::Target {
        &self.q
    }
}

impl<'outer, 'inner, S> DerefMut for Query<'outer, 'inner, S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.q
    }
}

impl<'outer, 'inner, S> Query<'outer, 'inner, S> {
    /// Turn a database query into a rust [Vec] of results.
    ///
    /// Types that implement [crate::IntoColumn], will also implement [Dummy].
    /// Tuples of two values also implement [Dummy]. If you want to return more
    /// than two values, then you should use a struct that derives [crate::FromDummy].
    pub fn into_vec<'e, D>(&'inner self, dummy: D) -> Vec<D::Out>
    where
        D: Dummy<'inner, 'e, 'outer, S>,
        'outer: 'e,
    {
        self.into_vec_private(dummy)
    }

    pub(crate) fn into_vec_private<'x, 'l, D>(&'inner self, dummy: D) -> Vec<D::Out>
    where
        D: Dummy<'x, 'l, 'outer, S>,
        S: 'x,
        'outer: 'l,
    {
        let mut d = DynDummy::new(dummy);
        let cached = self.ast.cache(d.columns);

        let select = self.ast.simple();
        let (sql, values) = select.build_rusqlite(SqliteQueryBuilder);
        if SHOW_SQL.get() {
            println!("{sql}");
            println!("{values:?}");
        }

        let mut statement = self.conn.prepare_cached(&sql).unwrap();
        let mut rows = statement.query(&*values.as_params()).unwrap();

        let mut out = vec![];
        while let Some(row) = rows.next().unwrap() {
            out.push((d.func)(row, &cached));
        }
        out
    }
}

thread_local! {
    static SHOW_SQL: Cell<bool> = const { Cell::new(false) };
}

pub fn show_sql<R>(f: impl FnOnce() -> R) -> R {
    let old = SHOW_SQL.get();
    SHOW_SQL.set(true);
    let res = f();
    SHOW_SQL.set(old);
    res
}
