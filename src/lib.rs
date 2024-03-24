mod ast;
pub mod value;

use std::{
    cell::{Cell, OnceCell},
    marker::PhantomData,
};

use ast::{Joins, MySelect, Source};

use elsa::FrozenVec;
use sea_query::{Alias, Func, Iden, SqliteQueryBuilder};
use value::{Db, Field, FkInfo, MyAlias, MyIdenT, MyTableT, Value};

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
    pub fn new(table: &'a Joins) -> Self {
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
        self.ast.filters.push(Box::new(prop.into_expr()));
    }

    // the values of which all variants need to be preserved
    // TODO: add a variant with ordering?
    pub fn all<V: Value + 'inner>(&mut self, val: &V) -> Db<'outer, V::Typ> {
        let alias = MyAlias::new();
        let item = (alias, val.into_expr());
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
        let item = (alias, val.into_expr());
        self.0.ast.sort.push(Box::new(item));
        V::Typ::iden_any(self.0.joins, Field::U64(alias))
    }

    pub fn avg<V: Value<Typ = i64> + 'inner>(&mut self, val: V) -> Db<'outer, i64> {
        let alias = MyAlias::new();
        let expr = Func::cast_as(Func::avg(val.into_expr()), Alias::new("integer"));
        self.0.ast.aggr.push(Box::new((alias, expr.into())));
        i64::iden_any(self.0.joins, Field::U64(alias))
    }

    pub fn count_distinct<V: Value + 'inner>(&mut self, val: &V) -> Db<'outer, i64> {
        let alias = MyAlias::new();
        let item = (alias, Func::count_distinct(val.into_expr()).into());
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

impl<'names> Exec<'names> {
    pub fn into_vec<F, T>(&self, q: Query<'_, 'names>, mut f: F) -> Vec<T>
    where
        F: FnMut(Row<'_, 'names>) -> T,
    {
        let inner_select = q.ast.build_select();
        let mut select = q.joins.wrap(&inner_select, 0);
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
            };
            out.push(f(row));

            if updated.get() {
                select = q.joins.wrap(&inner_select, out.len());
                let sql = select.to_string(SqliteQueryBuilder);

                drop(rows);
                statement = conn.prepare(&sql).unwrap();
                rows = statement.query([]).unwrap();
            }
        }
        out
    }

    pub fn into_vec2<F, T>(&self, q: Group<'_, 'names>, f: F) -> Vec<T>
    where
        F: FnMut(Row<'_, 'names>) -> T,
    {
        self.into_vec(q.0, f)
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
}

impl<'names> Row<'_, 'names> {
    pub fn get<T: MyIdenT + rusqlite::types::FromSql>(&self, val: Db<'names, T>) -> T {
        let alias = val.info.alias();
        let idx = &*alias.col.to_string();
        match self.row.get(idx) {
            Ok(res) => res,
            Err(rusqlite::Error::InvalidColumnName(_)) => self.requery(idx),
            Err(e) => {
                panic!("{}", e)
            }
        }
    }

    fn requery<T: MyIdenT + rusqlite::types::FromSql>(&self, idx: &str) -> T {
        let mut select = self.ast.build_select();
        select = self.joins.wrap(&select, self.offset);

        let sql = select.to_string(SqliteQueryBuilder);
        println!("{sql}");
        let mut statement = self.conn.prepare(&sql).unwrap();
        let mut rows = statement.query([]).unwrap();
        self.updated.set(true);
        rows.next().unwrap().unwrap().get_unwrap(idx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestTable;

    impl Table for TestTable {
        type Dummy<'names> = TestDummy<'names>;
        const NAME: &'static str = "test";
        const ID: &'static str = "id";
        fn build(f: Builder<'_>) -> Self::Dummy<'_> {
            TestDummy {
                foo: f.iden("foo"),
                bar: f.iden("bar"),
            }
        }
    }
    struct TestDummy<'names> {
        foo: Db<'names, i64>,
        bar: Db<'names, i64>,
    }

    #[test]
    fn test() {
        // new_query(|e, mut q| {
        //     let q_test = q.table(TestTable);
        //     let out = q.query(|mut g| {
        //         let g_test = g.table(TestTable);
        //         g.filter(q_test.foo);
        //         let foo = g.all(&g_test.foo);

        //         let mut g = g.into_groups();
        //         let bar_avg = g.avg(g_test.bar);
        //         (foo, bar_avg)
        //     });
        //     q.filter(out.0);
        //     let out = q.all(&out.1);

        //     new_query(|e, mut p| {
        //         let test_p = p.table(TestTable);
        //         let bar = p.all(&test_p.bar);
        //         // q.filter(bar);
        //         // q.filter(test_p.foo);
        //         // p.filter(q_test.foo);
        //         // p.filter(out);

        //         let rows = e.all_rows(p);
        //         // let val = rows[0].get_i64(out);
        //     });

        //     for row in e.all_rows(q) {
        //         row.get(out);
        //     }
        // });
    }

    fn get_match<'a, 'b>(q: &mut Query<'a, 'b>, foo: impl Value + 'a) -> Db<'a, i64> {
        let test = q.table(TestTable);
        q.filter(test.foo.eq(foo));
        test.foo
    }

    // fn transpose() {
    //     new_query(|mut q| {
    //         let alpha = q.table(TestTable);
    //         let mut beta = None;
    //         q.query(|mut g| {
    //             let res = get_match(&mut g, alpha.foo);
    //             let mut res = g.group(res);
    //             beta = Some(res.rank_asc(alpha.foo));
    //         });
    //         q.filter(alpha.foo.eq(beta.unwrap()))
    //     });
    // }
}
