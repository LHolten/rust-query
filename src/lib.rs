mod ast;
pub mod value;

use std::{cell::OnceCell, marker::PhantomData};

use ast::{MySelect, MyTable, Source};

use elsa::FrozenVec;
use value::{Db, MyIdenT, TableInfo, Value};

use crate::value::MyAlias;

pub struct Query<'inner, 'outer> {
    // we might store 'inner
    phantom: PhantomData<dyn Fn(&'inner &'outer ()) -> &'inner &'outer ()>,
    ast: &'outer MySelect,
    // outer: PhantomData<>
}

pub trait Table {
    const NAME: &'static str;
    const ID: &'static str = "id";
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
        T::iden(item)
    }
}

impl<'inner, 'outer> Query<'inner, 'outer> {
    pub fn table<T: Table>(&mut self, _t: T) -> Db<'inner, T> {
        let table = Source::Table(MyTable {
            name: T::NAME,
            columns: FrozenVec::new(),
        });
        let Source::Table(table) = self.ast.sources.push_get(Box::new(table)) else {
            unreachable!()
        };
        Db {
            col: Builder::new(table).iden::<T>(T::ID).col,
            info: TableInfo {
                table,
                inner: OnceCell::new(),
            },
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
        // self.ast.filters.push(prop.into_expr());
    }

    // the values of which all variants need to be preserved
    // TODO: add a variant with ordering?
    pub fn all<V: Value + 'inner>(&mut self, val: V) -> Db<'outer, V::Typ> {
        let item = (V::Typ::new_alias(), val.into_expr());
        let last = self.ast.group.push_get(Box::new(item));
        V::Typ::iden(&last.0)
    }

    pub fn into_groups(self) -> Group<'inner, 'outer> {
        Group(self)
    }
}

pub struct Group<'inner, 'outer>(Query<'inner, 'outer>);

impl<'inner, 'outer> Group<'inner, 'outer> {
    // TODO: add a variant with ordering?
    pub fn any<V: Value + 'inner>(&mut self, val: V) -> Db<'outer, V::Typ> {
        let alias = MyAlias::new();
        // self.0.ast.sort.push((alias, val.into_expr()));
        // alias.iden()
        todo!()
    }

    fn avg<V: Value<Typ = i64> + 'inner>(&mut self, val: V) -> Db<'outer, i64> {
        let alias = MyAlias::new();
        // let expr = Func::avg(val.into_expr()).into();
        // self.0.ast.aggr.push((alias, expr));
        // alias.iden()
        todo!()
    }

    fn count_distinct<V: Value + 'inner>(&mut self, val: V) -> Db<'outer, i64> {
        let alias = MyAlias::new();
        // let expr = Func::count_distinct(val.into_expr()).into();
        // self.0.ast.aggr.push((alias, expr));
        // alias.iden()
        todo!()
    }
}

pub fn new_query<F, R>(f: F) -> R
where
    F: for<'a, 'names> FnOnce(Exec<'names>, Query<'a, 'names>) -> R,
{
    todo!()
}

pub struct Exec<'a> {
    // we are contravariant with respect to 'a
    phantom: PhantomData<dyn Fn(&'a ())>,
}

impl<'names> Exec<'names> {
    pub fn all_rows(self, q: Query<'_, 'names>) -> Rows<'names> {
        todo!()
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
                let foo = g.all(g_test.foo);

                let mut g = g.into_groups();
                let bar_avg = g.avg(g_test.bar);
                (foo, bar_avg)
            });
            q.filter(out.0);
            let out = q.all(out.1);

            new_query(|e, mut p| {
                let test_p = p.table(TestTable);
                let bar = p.all(test_p.bar);
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
