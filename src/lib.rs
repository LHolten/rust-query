mod ast;
pub mod value;

use std::{cell::OnceCell, marker::PhantomData, process::exit};

use ast::{MySelect, MyTable, Source};

use sea_query::{Func, SqliteQueryBuilder};
use value::{Db, MyIdenT, MyTableAlias, Value};

use crate::value::MyAlias;

pub struct Query<'inner, 'outer> {
    // we might store 'inner
    phantom: PhantomData<dyn Fn(&'inner &'outer ()) -> &'inner &'outer ()>,
    ast: &'outer MySelect,
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
    table: &'a MyTable,
}

impl<'a> Builder<'a> {
    pub fn new(table: &'a MyTable) -> Self {
        Builder { table }
    }

    pub fn iden<T: MyIdenT>(&self, name: &'static str) -> Db<'a, T> {
        let t = self.table;
        let item = if let Some(item) = t.columns.iter().find(|item| item.0 == name) {
            &item.1
        } else {
            let alias = T::new_alias();
            &t.columns.push_get(Box::new((name, alias))).1
        };
        T::iden_any(item)
    }
}

impl<'inner, 'outer> Query<'inner, 'outer> {
    pub fn table<T: Table>(&mut self, _t: T) -> Db<'inner, T> {
        let alias = MyTableAlias::new(T::NAME, T::ID);

        let source = Box::new(Source::Table(alias));
        let Source::Table(alias) = self.ast.sources.push_get(source) else {
            unreachable!()
        };
        Db {
            col: alias,
            inner: OnceCell::new(),
        }
    }

    // join another query that is grouped by some value
    pub fn query<F, R>(&mut self, f: F) -> R
    where
        F: for<'a> FnOnce(Query<'a, 'inner>) -> R,
    {
        let source = Source::Select(MySelect::default());
        let source = self.ast.sources.push_get(Box::new(source));
        let Source::Select(ast) = source else {
            unreachable!()
        };
        let inner = Query {
            phantom: PhantomData,
            ast,
        };
        f(inner)
    }

    pub fn filter(&mut self, prop: impl Value + 'inner) {
        self.ast.filters.push(Box::new(prop.into_expr()));
    }

    // the values of which all variants need to be preserved
    // TODO: add a variant with ordering?
    pub fn all<V: Value + 'inner>(&mut self, val: &V) -> Db<'outer, V::Typ> {
        let item = (V::Typ::new_alias(), val.into_expr());
        let last = self.ast.group.push_get(Box::new(item));
        V::Typ::iden_any(&last.0)
    }

    pub fn into_groups(self) -> Group<'inner, 'outer> {
        Group(self)
    }
}

pub struct Group<'inner, 'outer>(Query<'inner, 'outer>);

impl<'inner, 'outer> Group<'inner, 'outer> {
    // TODO: add a variant with ordering?
    pub fn any<V: Value + 'inner>(&mut self, val: &V) -> Db<'outer, V::Typ> {
        let item = (V::Typ::new_alias(), val.into_expr());
        let last = self.0.ast.sort.push_get(Box::new(item));
        V::Typ::iden_any(&last.0)
    }

    pub fn avg<V: Value<Typ = i64> + 'inner>(&mut self, val: V) -> Db<'outer, i64> {
        let item = (MyAlias::new(), Func::avg(val.into_expr()).into());
        let last = self.0.ast.aggr.push_get(Box::new(item));
        i64::iden(&last.0)
    }

    pub fn count_distinct<V: Value + 'inner>(&mut self, val: &V) -> Db<'outer, i64> {
        let item = (MyAlias::new(), Func::count_distinct(val.into_expr()).into());
        let last = self.0.ast.aggr.push_get(Box::new(item));
        i64::iden(&last.0)
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
    let q = Query {
        phantom: PhantomData,
        ast: &ast,
    };
    f(e, q)
}

pub struct Exec<'a> {
    // we are contravariant with respect to 'a
    phantom: PhantomData<dyn Fn(&'a ())>,
}

impl<'names> Exec<'names> {
    pub fn all_rows(self, q: Query<'_, 'names>) -> Rows<'names> {
        let sql = q.ast.build_select().to_string(SqliteQueryBuilder);
        println!("{sql}");
        let conn = rusqlite::Connection::open("examples/Chinook_Sqlite.sqlite").unwrap();
        let statement = &mut conn.prepare(&sql).unwrap();
        let mut rows = statement.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            println!("{row:?}");
        }
        exit(0)
    }

    pub fn all_rows2(self, q: Group<'_, 'names>) -> Rows<'names> {
        self.all_rows(q.0)
    }
}

pub struct Rows<'names> {
    _p: PhantomData<&'names ()>,
}

impl<'names> Iterator for Rows<'names> {
    type Item = Row<'names>;

    fn next(&mut self) -> Option<Self::Item> {
        todo!()
    }
}

pub struct Row<'names> {
    inner: PhantomData<dyn Fn(&'names ())>,
}

impl<'names> Row<'names> {
    pub fn get<V: Value + 'names>(&self, val: V) -> V::Typ {
        todo!()
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
        new_query(|e, mut q| {
            let q_test = q.table(TestTable);
            let out = q.query(|mut g| {
                let g_test = g.table(TestTable);
                g.filter(q_test.foo);
                let foo = g.all(&g_test.foo);

                let mut g = g.into_groups();
                let bar_avg = g.avg(g_test.bar);
                (foo, bar_avg)
            });
            q.filter(out.0);
            let out = q.all(&out.1);

            new_query(|e, mut p| {
                let test_p = p.table(TestTable);
                let bar = p.all(&test_p.bar);
                // q.filter(bar);
                // q.filter(test_p.foo);
                // p.filter(q_test.foo);
                // p.filter(out);

                let rows = e.all_rows(p);
                // let val = rows[0].get_i64(out);
            });

            for row in e.all_rows(q) {
                row.get(out);
            }
        });
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
