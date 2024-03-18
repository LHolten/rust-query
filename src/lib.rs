mod ast;
mod value;

use std::marker::PhantomData;

use ast::{MyDef, MySelect, Source};

use sea_query::Func;
use value::{MyIden, Value};

use crate::value::MyAlias;

pub struct Query<'inner, 'outer> {
    // we might store 'inner
    selected: PhantomData<&'inner &'outer ()>,
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
    pub fn query<F>(&mut self, f: F)
    where
        F: for<'a> FnOnce(Query<'a, 'inner>),
    {
        let mut ast = MySelect::default();
        let inner = Query {
            selected: PhantomData,
            ast: &mut ast,
        };
        f(inner);
        self.ast.sources.push(ast::Source::Select(ast))
    }

    pub fn filter(&mut self, prop: impl Value + 'inner) {
        self.ast.filters.push(prop.into_expr());
    }

    // the values of which all variants need to be preserved
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

pub fn new_query<F>(f: F)
where
    F: for<'a> FnOnce(Base<'a>),
{
}

pub struct Base<'a> {
    inner: &'a mut MySelect,
}

impl<'names> Base<'names> {
    pub fn all_rows<F>(self, f: F) -> impl Iterator<Item = Row<'names>>
    where
        F: for<'a> FnOnce(Query<'a, 'names>),
    {
        let query = Query {
            selected: PhantomData,
            ast: self.inner,
        };
        f(query);
        // query.ast.into_select(None);
        [].into_iter()
    }
}

pub struct Row<'names> {
    inner: PhantomData<&'names MySelect>,
}

impl<'names> Row<'names> {
    pub fn get_string(&self, val: impl Value + 'names) -> String {
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

    fn sub_query<'a, 'b>(q: &mut Query<'a, 'b>, val: impl Value + 'a) -> impl Value + 'a {
        q.filter(val);
        val
    }

    #[test]
    fn test() {
        new_query(|b| {
            let rows = b.all_rows(|mut q| {
                let q_test = q.table(TestTable);
                let mut out = None;
                q.query(|mut g| {
                    let g_test = g.table(TestTable);
                    g.filter(q_test.foo);
                    let foo = g.all(g_test.foo);

                    let mut g = g.into_groups();
                    let bar_avg = g.avg(g_test.bar);
                    out = Some((foo, bar_avg));
                });
                q.filter(out.unwrap());

                new_query(|p| {
                    let mut out = None;
                    let mut rows = p.all_rows(|mut p| {
                        let test_p = p.table(TestTable);
                        // q.filter(test_p.foo);
                        // FIXME: this should give an error
                        p.filter(q_test.foo);

                        out = Some(p.all(test_p.bar));
                    });

                    let val = rows.next().unwrap().get_string(out.unwrap());
                });

                let x = sub_query(&mut q, q_test.foo);
                q.filter(x);
            });
        });
    }

    fn get_match<'a, 'b>(q: &mut Query<'a, 'b>, foo: impl Value + 'a) -> impl Value + 'a {
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
