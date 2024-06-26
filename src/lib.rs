#![allow(private_bounds)]

mod ast;
pub mod client;
pub mod group;
#[doc(hidden)]
pub mod insert;
mod mymap;
mod pragma;
pub mod schema;
pub mod value;

use std::{
    cell::Cell,
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use ast::{Joins, MySelect, MyTable, Source};

use elsa::FrozenVec;
use group::GroupQuery;
use sea_query::{Expr, Iden, SqliteQueryBuilder};
use value::{Db, Field, FieldAlias, FkInfo, MyAlias, MyIdenT, Value};

/// This is the top level query type and dereferences to [Query].
/// It has methods for turning queries into vectors and for inserting in the database.
pub struct Exec<'outer, 'inner> {
    phantom: PhantomData<&'outer ()>,
    q: Query<'inner>,
}

impl<'outer, 'inner> Deref for Exec<'outer, 'inner> {
    type Target = Query<'inner>;

    fn deref(&self) -> &Self::Target {
        &self.q
    }
}

impl<'outer, 'inner> DerefMut for Exec<'outer, 'inner> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.q
    }
}

/// This is the base type for other query types like [GroupQuery] and [Exec].
/// It contains most query functionality like joining tables and doing sub-queries.
pub struct Query<'inner> {
    // we might store 'inner
    phantom: PhantomData<dyn Fn(&'inner ()) -> &'inner ()>,
    ast: &'inner MySelect,
    client: &'inner rusqlite::Connection,
}

#[doc(hidden)]
pub trait Table {
    // const NAME: &'static str;
    // these names are defined in `'query`
    type Dummy<'t>;

    fn name(&self) -> String;

    fn build(f: Builder<'_>) -> Self::Dummy<'_>;
}

#[doc(hidden)]
pub trait HasId: Table {
    const ID: &'static str;
    const NAME: &'static str;
}

#[doc(hidden)]
pub struct Builder<'a> {
    joined: &'a FrozenVec<Box<(Field, MyTable)>>,
    table: MyAlias,
}

impl<'a> Builder<'a> {
    fn new(joins: &'a Joins) -> Self {
        Self::new_full(&joins.joined, joins.table)
    }

    fn new_full(joined: &'a FrozenVec<Box<(Field, MyTable)>>, table: MyAlias) -> Self {
        Builder { joined, table }
    }

    pub fn col<T: MyIdenT>(&self, name: &'static str) -> Db<'a, T> {
        let field = FieldAlias {
            table: self.table,
            col: Field::Str(name),
        };
        T::iden_full(self.joined, field)
    }
}

impl<'inner> Query<'inner> {
    fn new_source<T: Table>(&mut self, t: T) -> &'inner Joins {
        let joins = Joins {
            table: MyAlias::new(),
            joined: FrozenVec::new(),
        };
        let source = Box::new(Source::Table(t.name(), joins));
        let Source::Table(_, joins) = self.ast.sources.push_get(source) else {
            unreachable!()
        };
        joins
    }

    /// Join a table, this is like [Iterator::flat_map] but for queries.
    pub fn table<T: HasId>(&mut self, t: T) -> Db<'inner, T> {
        let joins = self.new_source(t);
        let field = FieldAlias {
            table: joins.table,
            col: Field::Str(T::ID),
        };
        FkInfo::joined(&joins.joined, field)
    }

    /// Join a table that has no integer primary key.
    /// Refer to [Query::table] for more information about joining tables.
    pub fn flat_table<T: Table>(&mut self, t: T) -> T::Dummy<'inner> {
        let joins = self.new_source(t);
        T::build(Builder::new(joins))
    }

    /// Perform a sub-query that returns a single result for each of the current rows.
    pub fn query<F, R>(&self, f: F) -> R
    where
        F: for<'a> FnOnce(&'a mut GroupQuery<'inner, 'a>) -> R,
    {
        let joins = Joins {
            table: MyAlias::new(),
            joined: FrozenVec::new(),
        };
        let source = Source::Select(MySelect::default(), joins);
        let source = self.ast.sources.push_get(Box::new(source));
        let Source::Select(ast, joins) = source else {
            unreachable!()
        };
        let inner = Query {
            phantom: PhantomData,
            ast,
            client: self.client,
        };
        let mut group = GroupQuery {
            query: inner,
            joins,
            phantom2: PhantomData,
        };
        f(&mut group)
    }

    /// Filter rows based on a column.
    pub fn filter(&mut self, prop: impl Value<'inner>) {
        self.ast.filters.push(Box::new(prop.build_expr()));
    }

    /// Filter out rows where this column is [None].
    pub fn filter_some<T: MyIdenT>(&mut self, val: &Db<'inner, Option<T>>) -> Db<'inner, T> {
        self.ast
            .filters
            .push(Box::new(Expr::expr(val.build_expr())).is_not_null().into());
        Db {
            info: val.info.clone(),
            field: val.field,
        }
    }
}

impl<'outer, 'inner> Exec<'outer, 'inner> {
    /// Turn a database query into a rust [Vec] of results.
    /// The callback is called exactly once for each row.
    /// The callback argument [Row] can be used to turn dummies into rust values.
    pub fn into_vec<F, T>(&self, limit: u32, mut f: F) -> Vec<T>
    where
        F: FnMut(Row<'_, 'inner>) -> T,
    {
        let mut select = self.ast.simple(0, limit);
        let sql = select.to_string(SqliteQueryBuilder);

        // eprintln!("{sql}");
        let conn = self.client;
        let mut statement = conn.prepare(&sql).unwrap();
        let mut rows = statement.query([]).unwrap();

        let mut out = vec![];
        while let Some(row) = rows.next().unwrap() {
            let updated = Cell::new(false);
            let row = Row {
                offset: out.len(),
                limit,
                inner: PhantomData,
                row,
                ast: self.ast,
                conn,
                updated: &updated,
            };
            out.push(f(row));

            if updated.get() {
                // eprintln!("UPDATING!");

                select = self.ast.simple(out.len(), limit);
                let sql = select.to_string(SqliteQueryBuilder);
                // eprintln!("{sql}");

                drop(rows);
                statement = conn.prepare(&sql).unwrap();
                rows = statement.query([]).unwrap();
            }
        }
        out
    }
}

/// This is the type used by [Exec::into_vec] to allow turning dummies into rust values.
pub struct Row<'x, 'names> {
    offset: usize,
    limit: u32,
    inner: PhantomData<dyn Fn(&'names ())>,
    row: &'x rusqlite::Row<'x>,
    ast: &'x MySelect,
    conn: &'x rusqlite::Connection,
    updated: &'x Cell<bool>,
}

impl<'names> Row<'_, 'names> {
    /// Turn a dummy into a rust value.
    pub fn get<V: Value<'names>>(&self, val: V) -> V::Typ
    where
        V::Typ: MyIdenT + rusqlite::types::FromSql,
    {
        let expr = val.build_expr();
        let Some((_, alias)) = self.ast.select.iter().find(|x| x.0 == expr) else {
            let alias = Field::new();

            self.ast.select.push(Box::new((expr, alias)));
            return self.requery(alias);
        };

        if self.updated.get() {
            // self.row is not up to date
            self.requery(*alias)
        } else {
            let idx = &*alias.to_string();
            self.row.get_unwrap(idx)
        }
    }

    fn requery<T: MyIdenT + rusqlite::types::FromSql>(&self, alias: Field) -> T {
        let select = self.ast.simple(self.offset, self.limit);
        let sql = select.to_string(SqliteQueryBuilder);
        // eprintln!("REQUERY");
        // eprintln!("{sql}");
        let mut statement = self.conn.prepare(&sql).unwrap();
        let mut rows = statement.query([]).unwrap();
        self.updated.set(true);

        let idx = &*alias.to_string();
        rows.next().unwrap().unwrap().get_unwrap(idx)
    }
}
