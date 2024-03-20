mod ast;
mod value;

use std::marker::PhantomData;

use ast::{MyDef, MySelect, Source};

use sea_query::Func;
use value::{MyIden, Value};

use crate::value::MyAlias;

pub struct Query<'inner, 'outer> {
    // we might store 'inner
    phantom: PhantomData<dyn Fn(&'inner &'outer ()) -> &'inner &'outer ()>,
    ast: &'inner mut MySelect,
    // outer: PhantomData<>
}

pub trait Table {
    const NAME: &'static str;
    // these names are defined in `'query`
    type Dummy<'names>;

    fn build<'a, F>(f: F) -> Self::Dummy<'a>
    where
        F: FnMut(&'static str) -> MyIden<'a>;
}

impl<'inner, 'outer> Query<'inner, 'outer> {
    pub fn table<T: Table>(&mut self, _t: T) -> T::Dummy<'inner> {
        let mut columns = Vec::new();
        let res = T::build(|name| {
            let alias = MyAlias::new();
            columns.push((name, alias));
            alias.iden()
        });
        self.ast.sources.push(Source::Table(MyDef {
            table: T::NAME,
            columns,
        }));
        res
    }

    // join another query that is grouped by some value
    pub fn query<F, R>(&mut self, f: F) -> R
    where
        F: for<'a> FnOnce(Query<'a, 'inner>) -> R,
    {
        let mut ast = MySelect::default();
        let inner = Query {
            phantom: PhantomData,
            ast: &mut ast,
        };
        let res = f(inner);
        self.ast.sources.push(ast::Source::Select(ast));
        res
    }

    pub fn filter(&mut self, prop: impl Value + 'inner) {
        self.ast.filters.push(prop.into_expr());
    }

    // the values of which all variants need to be preserved
    // TODO: add a variant with ordering
    pub fn all(&mut self, val: impl Value + 'inner) -> MyIden<'outer> {
        let alias = MyAlias::new();
        self.ast.group.push((alias, val.into_expr()));
        alias.iden()
    }

    pub fn into_groups(self) -> Group<'inner, 'outer> {
        Group(self)
    }
}

pub struct Group<'inner, 'outer>(Query<'inner, 'outer>);

impl<'inner, 'outer> Group<'inner, 'outer> {
    pub fn any(&mut self, val: impl Value + 'inner, prefer_large: bool) -> MyIden<'outer> {
        let alias = MyAlias::new();
        self.0.ast.sort.push((alias, val.into_expr(), prefer_large));
        alias.iden()
    }

    fn avg<'a>(&'a mut self, val: impl Value + 'inner) -> MyIden<'outer> {
        let alias = MyAlias::new();
        let expr = Func::avg(val.into_expr()).into();
        self.0.ast.aggr.push((alias, expr));
        alias.iden()
    }

    fn count_distinct(&mut self, val: impl Value + 'inner) -> MyIden<'outer> {
        let alias = MyAlias::new();
        let expr = Func::count_distinct(val.into_expr()).into();
        self.0.ast.aggr.push((alias, expr));
        alias.iden()
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
    pub fn all_rows(self, q: Query<'_, 'names>) -> Vec<Row<'names>> {
        todo!()
    }
}

pub struct Row<'names> {
    inner: PhantomData<dyn Fn(&'names ())>,
}

impl<'names> Row<'names> {
    pub fn get_i64(&self, val: impl Value + 'names) -> i64 {
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
        fn build<'a, F>(mut f: F) -> Self::Dummy<'a>
        where
            F: FnMut(&'static str) -> MyIden<'a>,
        {
            TestDummy {
                foo: f("foo"),
                bar: f("bar"),
            }
        }
    }
    struct TestDummy<'names> {
        foo: MyIden<'names>,
        bar: MyIden<'names>,
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
            q.filter(out);
            let out = q.all(out);

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
                row.get_i64(out);
            }
        });
    }

    fn get_match<'a, 'b>(q: &mut Query<'a, 'b>, foo: impl Value + 'a) -> MyIden<'a> {
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
