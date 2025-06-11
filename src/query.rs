use std::{
    cell::Cell,
    fmt::Debug,
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use rusqlite::Connection;
use sea_query::SqliteQueryBuilder;
use sea_query_rusqlite::{RusqliteBinder, RusqliteValues};
use self_cell::{MutBorrow, self_cell};

use crate::{
    alias::MyAlias,
    dummy_impl::{Cacher, DynPrepared, IntoSelect, Prepared, Row, SelectImpl},
    rows::Rows,
};

/// This is the type used by the [crate::Transaction::query] method.
pub struct Query<'outer, 'inner, S> {
    pub(crate) phantom: PhantomData<&'inner &'outer ()>,
    pub(crate) q: Rows<'inner, S>,
    pub(crate) conn: &'inner rusqlite::Connection,
}

impl<'inner, S> Deref for Query<'_, 'inner, S> {
    type Target = Rows<'inner, S>;

    fn deref(&self) -> &Self::Target {
        &self.q
    }
}

impl<S> DerefMut for Query<'_, '_, S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.q
    }
}

type Stmt<'x> = rusqlite::CachedStatement<'x>;
type RRows<'a> = rusqlite::Rows<'a>;

self_cell!(
    struct OwnedRows<'x> {
        owner: MutBorrow<Stmt<'x>>,

        #[covariant]
        dependent: RRows,
    }
);

pub struct Iter<'inner, O> {
    inner: OwnedRows<'inner>,
    prepared: DynPrepared<'inner, O>,
    cached: Vec<MyAlias>,
}

impl<O> Iterator for Iter<'_, O> {
    type Item = O;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.with_dependent_mut(|_, rows| {
            let row = rows.next().unwrap()?;
            Some(self.prepared.call(Row::new(row, &self.cached)))
        })
    }
}

impl<'outer, 'inner, S> Query<'outer, 'inner, S> {
    /// Turn a database query into a [Vec] of results.
    ///
    /// The order of rows that is returned is unstable. This means that the order may change between any two
    /// executions of the exact same query. If a specific order (or even a consistent order) is required,
    /// then you have to use something like [slice::sort].
    pub fn into_vec<O>(&self, select: impl IntoSelect<'inner, 'outer, S, Out = O>) -> Vec<O> {
        self.into_iter(select).collect()
    }

    /// Turn a database query into an [Iter] of results.
    ///
    /// The order of rows that is returned is unstable. This means that the order may change between any two
    /// executions of the exact same query. If a specific order (or even a consistent order) is required,
    /// then you have to use something like [slice::sort].
    pub fn into_iter<O>(
        &self,
        select: impl IntoSelect<'inner, 'outer, S, Out = O>,
    ) -> Iter<'inner, O> {
        let mut cacher = Cacher::new();
        let prepared = select.into_select().inner.prepare(&mut cacher);

        let (select, cached) = self.ast.clone().full().simple(cacher.columns);
        let (sql, values) = select.build_rusqlite(SqliteQueryBuilder);
        if SHOW_SQL.get() {
            println!("{sql}");
            println!("{values:?}");
        }
        if GET_PLAN.get() {
            let node = get_node(&self.conn, &values, &sql);
            PLAN.set(Some(node));
        }

        let statement = MutBorrow::new(self.conn.prepare_cached(&sql).unwrap());

        Iter {
            inner: OwnedRows::new(statement, |stmt| {
                stmt.borrow_mut().query(&*values.as_params()).unwrap()
            }),
            prepared,
            cached,
        }
    }
}

thread_local! {
    static SHOW_SQL: Cell<bool> = const { Cell::new(false) };
    static GET_PLAN: Cell<bool> = const { Cell::new(false) };
    static PLAN: Cell<Option<Node>> = const { Cell::new(None) };
}

pub fn show_sql<R>(f: impl FnOnce() -> R) -> R {
    let old = SHOW_SQL.get();
    SHOW_SQL.set(true);
    let res = f();
    SHOW_SQL.set(old);
    res
}

pub fn get_plan<R>(f: impl FnOnce() -> R) -> (R, Node) {
    let old = GET_PLAN.get();
    GET_PLAN.set(true);
    let res = f();
    GET_PLAN.set(old);
    (res, PLAN.take().unwrap())
}

fn get_node(conn: &Connection, values: &RusqliteValues, sql: &str) -> Node {
    let mut prepared = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
    let rows = prepared
        .query_map(&*values.as_params(), |row| {
            Ok((
                row.get_unwrap("parent"),
                Node {
                    id: row.get_unwrap("id"),
                    detail: row.get_unwrap("detail"),
                    children: vec![],
                },
            ))
        })
        .unwrap();
    let mut out = Node {
        id: 0,
        detail: "QUERY PLAN".to_owned(),
        children: vec![],
    };
    rows.for_each(|res| {
        let (id, node) = res.unwrap();
        out.get_mut(id).children.push(node);
    });

    out
}

pub struct Node {
    id: i64,
    detail: String,
    children: Vec<Node>,
}

impl Node {
    fn get_mut(&mut self, id: i64) -> &mut Node {
        if self.id == id {
            return self;
        }
        self.children.last_mut().unwrap().get_mut(id)
    }
}

impl Debug for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.detail)?;
        if !self.children.is_empty() {
            f.write_str(" ")?;
            f.debug_list().entries(&self.children).finish()?;
        }
        Ok(())
    }
}
