#![allow(private_bounds)]

mod ast;
pub mod client;
pub mod insert;
mod mymap;
pub mod pragma;
pub mod schema;
pub mod value;

use std::{
    cell::Cell,
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use ast::{Joins, MySelect, MyTable, Source};

use elsa::FrozenVec;
use sea_query::{Alias, Expr, Func, Iden, SqliteQueryBuilder};
use value::{Db, Field, FieldAlias, FkInfo, IsNotNull, MyAlias, MyIdenT, UnwrapOr, Value};

pub struct Exec<'outer, 'inner> {
    q: Query<'outer, 'inner>,
}

impl<'outer, 'inner> Deref for Exec<'outer, 'inner> {
    type Target = Query<'outer, 'inner>;

    fn deref(&self) -> &Self::Target {
        &self.q
    }
}

impl<'outer, 'inner> DerefMut for Exec<'outer, 'inner> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.q
    }
}

pub struct Query<'outer, 'inner>
where
    'outer: 'inner,
{
    // we might store 'inner
    phantom: PhantomData<dyn Fn(&'inner ()) -> &'inner ()>,
    phantom2: PhantomData<dyn Fn(&'outer ()) -> &'outer ()>,
    ast: &'inner MySelect,
    joins: &'outer Joins,
    client: &'inner rusqlite::Connection,
    // outer: PhantomData<>
}

pub trait Table {
    // const NAME: &'static str;
    // these names are defined in `'query`
    type Dummy<'t>;

    fn name(&self) -> String;

    fn build(f: Builder<'_>) -> Self::Dummy<'_>;
}

pub trait HasId: Table {
    const ID: &'static str;
    const NAME: &'static str;
}

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

impl<'outer, 'inner> Query<'outer, 'inner> {
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

    pub fn table<T: HasId>(&mut self, t: T) -> Db<'inner, T> {
        let joins = self.new_source(t);
        let field = FieldAlias {
            table: joins.table,
            col: Field::Str(T::ID),
        };
        FkInfo::joined(&joins.joined, field)
    }

    pub fn flat_table<T: Table>(&mut self, t: T) -> T::Dummy<'inner> {
        let joins = self.new_source(t);
        T::build(Builder::new(joins))
    }

    // join another query that is grouped by some value
    pub fn query<F, R>(&mut self, f: F) -> R
    where
        F: for<'a> FnOnce(&'a mut Query<'inner, 'a>) -> R,
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
        let mut inner = Query {
            phantom: PhantomData,
            phantom2: PhantomData,
            ast,
            joins,
            client: self.client,
        };
        f(&mut inner)
    }

    pub fn filter(&mut self, prop: impl Value<'inner>) {
        self.ast.filters.push(Box::new(prop.build_expr()));
    }

    pub fn filter_on<T: MyIdenT>(
        &mut self,
        val: impl Value<'inner, Typ = T>,
        on: impl Value<'outer, Typ = T>,
    ) {
        let alias = MyAlias::new();
        self.ast
            .filter_on
            .push(Box::new((val.build_expr(), alias, on.build_expr())))
    }

    pub fn filter_some<T: MyIdenT>(&mut self, val: Db<'inner, Option<T>>) -> Db<'inner, T> {
        self.ast
            .filters
            .push(Box::new(Expr::expr(val.build_expr())).is_not_null().into());
        T::iden_full(&self.joins.joined, val.field)
    }

    pub fn select<V: Value<'inner>>(&'inner self, val: V) -> Db<'outer, V::Typ> {
        let alias = self.ast.select.get_or_init(val.build_expr(), Field::new);
        V::Typ::iden_any(self.joins, *alias)
    }

    // only one Group can exist at a time
    pub fn group(&'inner mut self) -> Group<'outer, 'inner> {
        self.ast.group.set(true);
        Group { inner: self }
    }

    // pub fn window<'out, V: Value + 'inner>(&'out self, val: V) -> &'out Group<'inner, V> {
    //     todo!()
    // }
}

pub struct Group<'outer, 'inner> {
    inner: &'inner mut Query<'outer, 'inner>,
}

// if we have a single row that is null for all columns, then
// this should be treated as if there are zero rows.
impl<'outer, 'inner> Group<'outer, 'inner> {
    pub fn avg<V: Value<'inner, Typ = i64>>(&self, val: V) -> Db<'outer, Option<i64>> {
        let expr = Func::cast_as(Func::avg(val.build_expr()), Alias::new("integer"));
        let alias = self.inner.ast.select.get_or_init(expr.into(), Field::new);
        Option::iden_any(self.inner.joins, *alias)
    }

    pub fn max<V: Value<'inner, Typ = i64>>(&self, val: V) -> Db<'outer, Option<i64>> {
        let expr = Func::max(val.build_expr());
        let alias = self.inner.ast.select.get_or_init(expr.into(), Field::new);
        Option::iden_any(self.inner.joins, *alias)
    }

    pub fn sum_float<V: Value<'inner, Typ = f64>>(
        &self,
        val: V,
    ) -> UnwrapOr<Db<'outer, Option<f64>>, f64> {
        let expr = Func::cast_as(Func::sum(val.build_expr()), Alias::new("integer"));
        let alias = self.inner.ast.select.get_or_init(expr.into(), Field::new);
        UnwrapOr(Option::iden_any(self.inner.joins, *alias), 0.)
    }

    pub fn count_distinct<V: Value<'inner>>(
        &self,
        val: V,
    ) -> UnwrapOr<Db<'outer, Option<i64>>, i64> {
        let expr = Func::count_distinct(val.build_expr());
        let alias = self.inner.ast.select.get_or_init(expr.into(), Field::new);
        UnwrapOr(Option::iden_any(self.inner.joins, *alias), 0)
    }

    pub fn exists(&self) -> IsNotNull<Db<'outer, i64>> {
        let expr = Expr::val(1);
        let alias = self.inner.ast.select.get_or_init(expr.into(), Field::new);
        IsNotNull(i64::iden_any(self.inner.joins, *alias))
    }

    // evil
    // pub fn rank<V: Value<'inner>>(&self, val: V) -> Db<'outer, i64> {
    //     // let expr = Func::count_distinct(val.build_expr());
    //     // let alias = self.ast.aggr.get_or_init(expr.into(), MyAlias::new);
    //     // i64::iden_any(self.joins, Field::U64(*alias))
    //     todo!()
    // }
}

impl<'outer, 'inner> Exec<'outer, 'inner> {
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
        // todo!()
    }
}

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
