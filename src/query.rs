use std::{
    cell::Cell,
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use sea_query::SqliteQueryBuilder;
use sea_query_rusqlite::RusqliteBinder;

use crate::{
    dummy_impl::{Cacher, IntoSelect, Prepared, Row, SelectImpl},
    rows::Rows,
};

/// This is the top level query type and dereferences to [Rows].
/// Most importantly it can turn the query result into a [Vec].
pub struct Query<'outer, 'inner, S> {
    pub(crate) phantom: PhantomData<&'inner &'outer ()>,
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
    /// Types that implement [crate::IntoExpr], will also implement [IntoSelect].
    /// Tuples of two values also implement [IntoSelect]. If you want to return more
    /// than two values, then you should use a struct that derives [rust_query_macros::Select].
    ///
    /// The order of rows that is returned is unstable. This means that the order may change between any two
    /// executions of the exact same query. If a specific order (or even a consistent order) is required,
    /// then you have to use something like [std::slice::sort].
    pub fn into_vec<O>(&self, select: impl IntoSelect<'inner, 'outer, S, Out = O>) -> Vec<O> {
        self.into_vec_private(select)
    }

    pub(crate) fn into_vec_private<'x, D>(&self, dummy: D) -> Vec<D::Out>
    where
        D: IntoSelect<'x, 'outer, S>,
    {
        let mut cacher = Cacher::new();
        let mut prepared = dummy.into_select().inner.prepare(&mut cacher);

        let cached = self.ast.cache(cacher.columns);

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
            out.push(prepared.call(Row::new(row, &cached)));
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
