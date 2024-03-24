#![allow(private_bounds)]

mod ast;
pub mod value;

use std::{
    cell::{Cell, OnceCell},
    marker::PhantomData,
};

use ast::{Joins, MySelect, Source};

use elsa::FrozenVec;
use sea_query::{Alias, Func, Iden, SimpleExpr, SqliteQueryBuilder};
use value::{Db, Field, FkInfo, MyAlias, MyIdenT, Value};

pub struct Query<'inner, 'outer> {
    // we might store 'inner
    phantom: PhantomData<dyn Fn(&'inner ()) -> &'inner ()>,
    phantom2: PhantomData<dyn Fn(&'outer ()) -> &'outer ()>,
    ast: &'inner MySelect,
    joins: &'outer Joins,
    // outer: PhantomData<>
}

pub trait Table {
    const NAME: &'static str;
    const ID: &'static str;
    // these names are defined in `'query`
    type Dummy<'names>;

    fn build(f: Builder<'_>) -> Self::Dummy<'_>;
}

pub struct Builder<'a> {
    table: &'a Joins,
}

impl<'a> Builder<'a> {
    fn new(table: &'a Joins) -> Self {
        Builder { table }
    }

    pub fn iden<T: MyIdenT>(&self, name: &'static str) -> Db<'a, T> {
        T::iden_any(self.table, Field::Str(name))
    }
}

impl<'inner, 'outer> Query<'inner, 'outer> {
    pub fn table<T: Table>(&mut self, _t: T) -> Db<'inner, T> {
        let joins = Joins {
            alias: MyAlias::new(),
            joined: FrozenVec::new(),
        };
        let source = Box::new(Source::Table(T::NAME, joins));
        let Source::Table(_, joins) = self.ast.sources.push_get(source) else {
            unreachable!()
        };
        Db {
            info: FkInfo {
                field: Field::Str(T::ID),
                table: joins,
                // prevent unnecessary join
                inner: OnceCell::from(T::build(Builder::new(joins))),
            },
        }
    }

    // join another query that is grouped by some value
    pub fn query<F, R>(&mut self, f: F) -> R
    where
        F: for<'a> FnOnce(Query<'a, 'inner>) -> R,
    {
        let joins = Joins {
            alias: MyAlias::new(),
            joined: FrozenVec::new(),
        };
        let source = Source::Select(MySelect::default(), joins);
        let source = self.ast.sources.push_get(Box::new(source));
        let Source::Select(ast, joins) = source else {
            unreachable!()
        };
        let inner = Query {
            phantom: PhantomData,
            phantom2: PhantomData,
            ast,
            joins,
        };
        f(inner)
    }

    pub fn filter(&mut self, prop: impl Value + 'inner) {
        self.ast.filters.push(Box::new(prop.build_expr()));
    }

    // the values of which all variants need to be preserved
    // TODO: add a variant with ordering?
    pub fn all<V: Value + 'inner>(&mut self, val: &V) -> Db<'outer, V::Typ> {
        let alias = MyAlias::new();
        let item = (alias, val.build_expr());
        self.ast.group.push(Box::new(item));
        V::Typ::iden_any(self.joins, Field::U64(alias))
    }

    pub fn into_groups(self) -> Group<'inner, 'outer> {
        Group(self)
    }
}

pub struct Group<'inner, 'outer>(Query<'inner, 'outer>);

impl<'inner, 'outer> Group<'inner, 'outer> {
    // TODO: add a variant with ordering?
    pub fn any<V: Value + 'inner>(&mut self, val: &V) -> Db<'outer, V::Typ> {
        let alias = MyAlias::new();
        let item = (alias, val.build_expr());
        self.0.ast.sort.push(Box::new(item));
        V::Typ::iden_any(self.0.joins, Field::U64(alias))
    }

    pub fn avg<V: Value<Typ = i64> + 'inner>(&mut self, val: V) -> Db<'outer, i64> {
        let alias = MyAlias::new();
        let expr = Func::cast_as(Func::avg(val.build_expr()), Alias::new("integer"));
        self.0.ast.aggr.push(Box::new((alias, expr.into())));
        i64::iden_any(self.0.joins, Field::U64(alias))
    }

    pub fn count_distinct<V: Value + 'inner>(&mut self, val: &V) -> Db<'outer, i64> {
        let alias = MyAlias::new();
        let item = (alias, Func::count_distinct(val.build_expr()).into());
        self.0.ast.aggr.push(Box::new(item));
        i64::iden_any(self.0.joins, Field::U64(alias))
    }
}

pub fn new_query<F, R>(f: F) -> R
where
    F: for<'a, 'names> FnOnce(Exec<'names>, Query<'a, 'names>) -> R,
{
    let e = Exec {
        phantom: PhantomData,
    };
    let ast = MySelect::default();
    let joins = Joins {
        alias: MyAlias::new(),
        joined: FrozenVec::new(),
    };
    let q = Query {
        phantom: PhantomData,
        phantom2: PhantomData,
        ast: &ast,
        joins: &joins,
    };
    f(e, q)
}

pub struct Exec<'a> {
    // we are contravariant with respect to 'a
    phantom: PhantomData<dyn Fn(&'a ())>,
}

trait IntoQuery<'a, 'b> {
    fn into_query(self) -> Query<'a, 'b>;
}

impl<'a, 'b> IntoQuery<'a, 'b> for Query<'a, 'b> {
    fn into_query(self) -> Query<'a, 'b> {
        self
    }
}

impl<'a, 'b> IntoQuery<'a, 'b> for Group<'a, 'b> {
    fn into_query(self) -> Query<'a, 'b> {
        self.0
    }
}

impl<'names> Exec<'names> {
    pub fn into_vec<'z, F, T>(&self, q: impl IntoQuery<'z, 'names>, mut f: F) -> Vec<T>
    where
        F: FnMut(Row<'_, 'names>) -> T,
    {
        let q = q.into_query();
        let inner_select = q.ast.build_select();
        let last = FrozenVec::new();
        let mut select = q.joins.wrap(&inner_select, 0, &last);
        let sql = select.to_string(SqliteQueryBuilder);

        println!("{sql}");
        let conn = rusqlite::Connection::open("examples/Chinook_Sqlite.sqlite").unwrap();
        let mut statement = conn.prepare(&sql).unwrap();
        let mut rows = statement.query([]).unwrap();

        let mut out = vec![];
        while let Some(row) = rows.next().unwrap() {
            let updated = Cell::new(false);
            let row = Row {
                offset: out.len(),
                inner: PhantomData,
                row,
                ast: q.ast,
                joins: q.joins,
                conn: &conn,
                updated: &updated,
                last: &last,
            };
            out.push(f(row));

            if updated.get() {
                println!("UPDATING!");
                select = q.joins.wrap(&inner_select, out.len(), &last);
                let sql = select.to_string(SqliteQueryBuilder);
                println!("{sql}");

                drop(rows);
                statement = conn.prepare(&sql).unwrap();
                rows = statement.query([]).unwrap();
            }
        }
        out
    }
}

pub struct Row<'x, 'names> {
    offset: usize,
    inner: PhantomData<dyn Fn(&'names ())>,
    row: &'x rusqlite::Row<'x>,
    ast: &'x MySelect,
    joins: &'x Joins,
    conn: &'x rusqlite::Connection,
    updated: &'x Cell<bool>,
    last: &'x FrozenVec<Box<(MyAlias, SimpleExpr)>>,
}

impl<'names> Row<'_, 'names> {
    pub fn get<V: Value + 'names>(&self, val: V) -> V::Typ
    where
        V::Typ: MyIdenT + rusqlite::types::FromSql,
    {
        let expr = val.build_expr();
        let Some((alias, _)) = self.last.iter().find(|x| x.1 == expr) else {
            let alias = MyAlias::new();
            self.last.push(Box::new((alias, expr)));
            return self.requery(alias);
        };

        let idx = &*alias.to_string();
        self.row.get_unwrap(idx)
    }

    fn requery<T: MyIdenT + rusqlite::types::FromSql>(&self, alias: MyAlias) -> T {
        let mut select = self.ast.build_select();
        select = self.joins.wrap(&select, self.offset, self.last);

        let sql = select.to_string(SqliteQueryBuilder);
        let mut statement = self.conn.prepare(&sql).unwrap();
        let mut rows = statement.query([]).unwrap();
        self.updated.set(true);

        let idx = &*alias.to_string();
        rows.next().unwrap().unwrap().get_unwrap(idx)
    }
}
