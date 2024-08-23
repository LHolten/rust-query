use std::{
    cell::Cell,
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use sea_query::SqliteQueryBuilder;
use sea_query_rusqlite::RusqliteBinder;

use crate::{
    from_row::{Cacher, FromRow, Row},
    query::Query,
};

/// This is the top level query type and dereferences to [Query].
/// It has methods for turning queries into vectors and for inserting in the database.
pub struct Execute<'outer, 'inner> {
    pub(crate) phantom: PhantomData<&'outer ()>,
    pub(crate) q: Query<'inner>,
    pub(crate) conn: &'inner rusqlite::Connection,
}

impl<'outer, 'inner> Deref for Execute<'outer, 'inner> {
    type Target = Query<'inner>;

    fn deref(&self) -> &Self::Target {
        &self.q
    }
}

impl<'outer, 'inner> DerefMut for Execute<'outer, 'inner> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.q
    }
}

impl<'outer, 'inner> Execute<'outer, 'inner> {
    /// Turn a database query into a rust [Vec] of results.
    pub fn into_vec<D>(&'inner self, dummy: D) -> Vec<D::Out>
    where
        D: FromRow<'inner, 'outer>,
    {
        let mut f = dummy.prepare(Cacher {
            _p: PhantomData,
            ast: self.ast,
        });

        let select = self.ast.simple();
        let (sql, values) = select.build_rusqlite(SqliteQueryBuilder);
        if SHOW_SQL.get() {
            println!("{sql}");
            println!("{values:?}");
        }

        let mut statement = self.conn.prepare(&sql).unwrap();
        let mut rows = statement.query(&*values.as_params()).unwrap();

        let mut out = vec![];
        while let Some(row) = rows.next().unwrap() {
            let row = Row {
                _p: PhantomData,
                _p2: PhantomData,
                row,
            };
            out.push(f(row));
        }
        out
    }
}

thread_local! {
    static SHOW_SQL: Cell<bool> = Cell::new(false);
}

pub fn show_sql<R>(f: impl FnOnce() -> R) -> R {
    let old = SHOW_SQL.get();
    SHOW_SQL.set(true);
    let res = f();
    SHOW_SQL.set(old);
    res
}
