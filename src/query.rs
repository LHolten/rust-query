use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
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
pub struct Query<'inner, S> {
    pub(crate) phantom: PhantomData<&'inner ()>,
    pub(crate) q: Rows<'inner, S>,
    pub(crate) conn: &'inner rusqlite::Connection,
}

impl<'inner, S> Deref for Query<'inner, S> {
    type Target = Rows<'inner, S>;

    fn deref(&self) -> &Self::Target {
        &self.q
    }
}

impl<S> DerefMut for Query<'_, S> {
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

/// Lazy iterator over rows from a query.
///
/// This is currently invariant in `'inner` due to [MutBorrow].
/// Would be nice to relax this variance in the future.
pub struct Iter<'inner, O> {
    inner: OwnedRows<'inner>,
    prepared: DynPrepared<O>,
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

impl<'inner, S> Query<'inner, S> {
    /// Turn a database query into a [Vec] of results.
    ///
    /// The order of rows that is returned is unstable. This means that the order may change between any two
    /// executions of the exact same query. If a specific order (or even a consistent order) is required,
    /// then you have to use something like [slice::sort].
    pub fn into_vec<O>(&self, select: impl IntoSelect<'inner, S, Out = O>) -> Vec<O> {
        self.into_iter(select).collect()
    }

    /// Turn a database query into an iterator of results.
    ///
    /// The order of rows that is returned is unstable. This means that the order may change between any two
    /// executions of the exact same query. If a specific order (or even a consistent order) is required,
    /// then you have to use something like [slice::sort].
    pub fn into_iter<O>(&self, select: impl IntoSelect<'inner, S, Out = O>) -> Iter<'inner, O> {
        let mut cacher = Cacher::new();
        let prepared = select.into_select().inner.prepare(&mut cacher);

        let (select, cached) = self.ast.clone().full().simple(cacher.columns);
        let (sql, values) = select.build_rusqlite(SqliteQueryBuilder);
        if COLLECT.get() {
            SQL_AND_PLAN.with_borrow_mut(|map| {
                map.entry(sql.clone())
                    .or_insert_with(|| get_node(self.conn, &values, &sql));
            });
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
    static COLLECT: Cell<bool> = const { Cell::new(false) };
    static SQL_AND_PLAN: RefCell<BTreeMap<String, Node>> = const { RefCell::new(BTreeMap::new()) };
}

pub fn get_plan<R>(f: impl FnOnce() -> R) -> (R, BTreeMap<String, Node>) {
    let old = COLLECT.get();
    COLLECT.set(true);
    let res = f();
    COLLECT.set(old);
    (res, SQL_AND_PLAN.take())
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
