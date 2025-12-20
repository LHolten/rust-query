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
    IntoExpr,
    alias::MyAlias,
    rows::Rows,
    select::{Cacher, DynPrepared, IntoSelect, Prepared, Row, SelectImpl},
    transaction::TXN,
    value::{DynTypedExpr, OrdTyp},
};

/// This is the type used by the [crate::Transaction::query] method.
pub struct Query<'t, 'inner, S> {
    pub(crate) phantom: PhantomData<&'t ()>,
    pub(crate) q: Rows<'inner, S>,
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
    pub struct OwnedRows<'x> {
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
    // The actual OwnedRows is stored in a thread local
    inner_phantom: PhantomData<(OwnedRows<'inner>, *const ())>,
    inner: usize,

    prepared: DynPrepared<O>,
    cached: Vec<MyAlias>,
}

impl<O> Iterator for Iter<'_, O> {
    type Item = O;

    fn next(&mut self) -> Option<Self::Item> {
        TXN.with_borrow_mut(|combi| {
            let combi = combi.as_mut().unwrap();
            combi.with_dependent_mut(|_txn, row_store| {
                // If rows is already dropped then we just return None.
                // This can happen if this is called in a thread_local destructor or something.
                let rows = row_store.get_mut(self.inner)?;
                rows.with_dependent_mut(|_, rows| {
                    let row = rows.next().unwrap()?;
                    Some(self.prepared.call(Row::new(row, &self.cached)))
                })
            })
        })
    }
}

impl<O> Drop for Iter<'_, O> {
    fn drop(&mut self) {
        TXN.with_borrow_mut(|combi| {
            let combi = combi.as_mut().unwrap();
            combi.with_dependent_mut(|_txn, row_store| {
                // If the rows is already dropped that is fine.
                // This can happen if this is called in a thread_local destructor or something.
                row_store.try_remove(self.inner);
            })
        })
    }
}

impl<'t, 'inner, S> Query<'t, 'inner, S> {
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
    /// then you have to use something like [slice::sort]. See also [Self::order_by].
    pub fn into_iter<O>(&self, select: impl IntoSelect<'inner, S, Out = O>) -> Iter<'t, O> {
        self.order_by().into_iter(select)
    }

    /// Use [Self::order_by] to refine the order or retrieved rows (partially).
    /// This is useful if not all rows are retrieved, e.g. for top N style queries.
    ///
    /// Every additional call to [OrderBy::asc] and [OrderBy::desc] refines the order further.
    /// This means that e.g. `order_by().asc(category).asc(priority)`, will have all items
    /// with the same `category` grouped together and only within the group are items sorted
    /// by priority.
    pub fn order_by<'q>(&'q self) -> OrderBy<'q, 't, 'inner, S> {
        OrderBy {
            query: self,
            order: Vec::new(),
        }
    }
}

/// [Query] is borrowed to prevent joining new tables.
/// If a copy was made, it would not know about new tables.
#[derive(Clone)]
pub struct OrderBy<'q, 't, 'inner, S> {
    query: &'q Query<'t, 'inner, S>,
    order: Vec<(DynTypedExpr, sea_query::Order)>,
}

impl<'t, 'inner, S> OrderBy<'_, 't, 'inner, S> {
    /// Add an additional value to sort on in ascending order.
    pub fn asc<'q, T: OrdTyp>(mut self, key: impl IntoExpr<'inner, S, Typ = T>) -> Self {
        self.order
            .push((DynTypedExpr::erase(key), sea_query::Order::Asc));
        self
    }

    /// Add an additional value to sort on in descending order.
    pub fn desc<'q, T: OrdTyp>(mut self, key: impl IntoExpr<'inner, S, Typ = T>) -> Self {
        self.order
            .push((DynTypedExpr::erase(key), sea_query::Order::Desc));
        self
    }

    /// Turn a database query into an iterator of results.
    ///
    /// Results are ordered as specified by [Self::asc] and [Self::desc].
    ///
    /// Rows of which the order is not determined by the calls to [Self::asc] and [Self::desc],
    /// are returned in unspecified order. See also [Query::into_iter].
    pub fn into_iter<O>(&self, select: impl IntoSelect<'inner, S, Out = O>) -> Iter<'t, O> {
        let mut cacher = Cacher::new();
        let prepared = select.into_select().inner.prepare(&mut cacher);
        let (select, cached) = self
            .query
            .ast
            .clone()
            .full()
            .simple_ordered(cacher.columns, self.order.clone());
        let (sql, values) = select.build_rusqlite(SqliteQueryBuilder);

        TXN.with_borrow_mut(|txn| {
            let combi = txn.as_mut().unwrap();

            combi.with_dependent_mut(|conn, rows_store| {
                track_stmt(conn.get(), &sql, &values);
                let statement = MutBorrow::new(conn.get().prepare_cached(&sql).unwrap());

                let idx = rows_store.insert(OwnedRows::new(statement, |stmt| {
                    stmt.borrow_mut().query(&*values.as_params()).unwrap()
                }));

                Iter {
                    inner: idx,
                    inner_phantom: PhantomData,
                    prepared,
                    cached,
                }
            })
        })
    }
}

pub(crate) fn track_stmt(conn: &Connection, sql: &String, values: &RusqliteValues) {
    if COLLECT.get() {
        SQL_AND_PLAN.with_borrow_mut(|map| {
            map.entry(sql.clone())
                .or_insert_with(|| get_node(conn, values, sql));
        });
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
