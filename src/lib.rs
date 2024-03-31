#![allow(private_bounds)]

mod ast;
mod mymap;
pub mod pragma;
pub mod value;

use std::{cell::Cell, marker::PhantomData};

use ast::{Joins, MySelect, MyTable, Source};

use elsa::FrozenVec;
use sea_query::{Alias, Expr, Func, Iden, SimpleExpr, SqliteQueryBuilder};
use value::{Db, Field, FieldAlias, FkInfo, MyAlias, MyIdenT, UnwrapOr, Unwrapped, Value};

pub struct Query<'outer, 'inner> {
    // we might store 'inner
    phantom: PhantomData<dyn Fn(&'inner ()) -> &'inner ()>,
    phantom2: PhantomData<dyn Fn(&'outer ()) -> &'outer ()>,
    ast: &'inner MySelect,
    joins: &'inner Joins,
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
        F: for<'a> FnOnce(&'inner mut Query<'inner, 'a>) -> R,
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
        let inner = Box::leak(Box::new(Query {
            phantom: PhantomData,
            phantom2: PhantomData,
            ast,
            joins,
        }));
        f(inner)
    }

    pub fn filter(&mut self, prop: impl Value<'inner>) {
        self.ast.filters.push(Box::new(prop.build_expr()));
    }

    pub fn unwrap<T: MyIdenT>(
        &mut self,
        val: impl Value<'inner, Typ = Option<T>>,
    ) -> impl Value<'inner, Typ = T> {
        self.ast
            .filters
            .push(Box::new(Expr::expr(val.build_expr())).is_not_null().into());
        Unwrapped(val)
    }

    pub fn select<V: Value<'inner>>(&'outer self, val: V) -> Db<'outer, V::Typ> {
        let alias = self.ast.select.get_or_init(val.build_expr(), MyAlias::new);
        V::Typ::iden_any(self.joins, Field::U64(*alias))
    }

    // only one group can exist at a time
    pub fn project_on<T: HasId>(
        &'outer mut self,
        val: impl Value<'inner, Typ = T>,
    ) -> Group<'outer, 'inner, T> {
        let table_alias = MyAlias::new();
        let alias = MyAlias::new();
        self.ast
            .group
            .get_or_init(|| (val.build_expr(), T::NAME, T::ID, table_alias, alias));
        Group {
            inner: self,
            table_alias,
            phantom: PhantomData,
        }
    }

    // pub fn window<'out, V: Value + 'inner>(&'out self, val: V) -> &'out Group<'inner, V> {
    //     todo!()
    // }
}

pub struct Group<'outer, 'inner, T> {
    inner: &'outer mut Query<'outer, 'inner>,
    table_alias: MyAlias,
    phantom: PhantomData<T>,
}

// if we have a single row that is null for all columns, then
// this should be treated as if there are zero rows.
impl<'outer, 'inner, T: HasId> Group<'outer, 'inner, T> {
    pub fn select(&self) -> Db<'outer, T> {
        let joined = &self.inner.joins.joined;
        let field = FieldAlias {
            table: self.table_alias,
            col: Field::Str(T::ID),
        };
        FkInfo::joined(joined, field)
    }

    pub fn avg<V: Value<'inner, Typ = i64>>(&self, val: V) -> Db<'outer, Option<i64>> {
        let expr = Func::cast_as(Func::avg(val.build_expr()), Alias::new("integer"));
        let alias = self.inner.ast.select.get_or_init(expr.into(), MyAlias::new);
        Option::iden_any(self.inner.joins, Field::U64(*alias))
    }

    pub fn count_distinct<V: Value<'inner>>(&self, val: V) -> UnwrapOr<Db<'outer, Option<i64>>> {
        let expr = Func::count_distinct(val.build_expr());
        let alias = self.inner.ast.select.get_or_init(expr.into(), MyAlias::new);
        UnwrapOr(Option::iden_any(self.inner.joins, Field::U64(*alias)), 0)
    }

    // evil
    // pub fn rank<V: Value<'inner>>(&self, val: V) -> Db<'outer, i64> {
    //     // let expr = Func::count_distinct(val.build_expr());
    //     // let alias = self.ast.aggr.get_or_init(expr.into(), MyAlias::new);
    //     // i64::iden_any(self.joins, Field::U64(*alias))
    //     todo!()
    // }

    pub fn into_vec<F, R>(&self, limit: u32, f: F) -> Vec<R>
    where
        F: FnMut(Row<'_, 'outer>) -> R,
    {
        self.inner.into_vec(limit, f)
    }
}

pub fn new_query<F, R>(f: F) -> R
where
    F: for<'a, 'names> FnOnce(&'names mut Query<'names, 'a>) -> R,
{
    let ast = MySelect::default();
    let joins = Joins {
        table: MyAlias::new(),
        joined: FrozenVec::new(),
    };
    let mut q = Query {
        phantom: PhantomData,
        phantom2: PhantomData,
        ast: &ast,
        joins: &joins,
    };
    f(&mut q)
}

impl<'outer, 'inner> Query<'outer, 'inner> {
    pub fn into_vec<F, T>(&self, limit: u32, mut f: F) -> Vec<T>
    where
        F: FnMut(Row<'_, 'outer>) -> T,
    {
        let last = FrozenVec::new();
        let mut select = self.joins.wrap(self.ast, 0, limit, &last);
        let sql = select.to_string(SqliteQueryBuilder);

        eprintln!("{sql}");
        let conn = rusqlite::Connection::open("examples/Chinook_Sqlite.sqlite").unwrap();
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
                joins: self.joins,
                conn: &conn,
                updated: &updated,
                last: &last,
            };
            out.push(f(row));

            if updated.get() {
                eprintln!("UPDATING!");
                select = self.joins.wrap(self.ast, out.len(), limit, &last);
                let sql = select.to_string(SqliteQueryBuilder);
                eprintln!("{sql}");

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
    joins: &'x Joins,
    conn: &'x rusqlite::Connection,
    updated: &'x Cell<bool>,
    last: &'x FrozenVec<Box<(MyAlias, SimpleExpr)>>,
}

impl<'names> Row<'_, 'names> {
    pub fn get<V: Value<'names>>(&self, val: V) -> V::Typ
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
        let select = self
            .joins
            .wrap(self.ast, self.offset, self.limit, self.last);

        let sql = select.to_string(SqliteQueryBuilder);
        eprintln!("REQUERY");
        eprintln!("{sql}");
        let mut statement = self.conn.prepare(&sql).unwrap();
        let mut rows = statement.query([]).unwrap();
        self.updated.set(true);

        let idx = &*alias.to_string();
        rows.next().unwrap().unwrap().get_unwrap(idx)
    }
}
