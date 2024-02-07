mod ast;
mod value;

use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use ast::{MyDef, MySelect, MyTable, Operation};
use sea_query::{Alias, SelectStatement};
use value::{MyIden, Value};

use crate::value::MyAlias;

// Query is only valid if `names` are in scope
pub struct Query<'names> {
    names: PhantomData<&'names ()>,
    ast: &'names mut MySelect,
}

pub trait Table {
    const NAME: &'static str;
    // these names are defined in `'query`
    type Dummy<'names>;

    fn build<'a, F>(f: F) -> Self::Dummy<'a>
    where
        F: FnMut(&'static str) -> MyIden<'a>;
}

impl<'names> Query<'names> {
    pub fn join<T: Table>(&mut self, _t: T) -> T::Dummy<'names> {
        let mut columns = Vec::new();
        let res = T::build(|name| {
            let alias = MyAlias::new();
            columns.push((Alias::new(name), alias));
            alias.iden()
        });
        self.ast.0.push(Operation::From(MyTable::Def(MyDef {
            table: Alias::new(T::NAME),
            columns,
        })));
        res
    }

    // join another query that is grouped by some value
    pub fn query<F>(&mut self, f: F)
    where
        F: for<'a> FnOnce(Group<'a, 'names>),
    {
        let mut ast = MySelect(vec![]);
        let inner = Query {
            names: PhantomData,
            ast: &mut ast,
        };
        let group = Group { outer: self, inner };
        f(group);
        self.ast
            .0
            .push(ast::Operation::From(ast::MyTable::Select(ast)))
    }

    pub fn filter(&mut self, prop: impl Value + 'names) {
        self.ast.0.push(ast::Operation::Filter(prop.into_expr()));
    }
}

pub struct Group<'a, 'names> {
    outer: &'a mut Query<'names>,
    inner: Query<'a>,
}

impl<'a, 'names> Deref for Group<'a, 'names> {
    type Target = Query<'a>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a, 'names> DerefMut for Group<'a, 'names> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<'a, 'names> Group<'a, 'names> {
    fn by(self, val: impl Value + 'a) -> Aggr<'a, 'names> {
        Aggr { group: self }
    }
}

pub struct Aggr<'a, 'names> {
    group: Group<'a, 'names>,
}

impl<'a, 'names> Aggr<'a, 'names> {
    pub fn rank_asc(&mut self, by: impl Value + 'names) -> MyIden<'names> {
        todo!()
    }

    // fn values(&mut self) -> Value<'names> {
    //     todo!()
    // }

    fn average(&mut self, val: impl Value + 'a) -> MyIden<'names> {
        todo!()
    }

    fn count(&mut self) -> MyIden<'names> {
        todo!()
    }
}

pub fn new_query<F>(f: F) -> SelectStatement
where
    F: for<'a> FnOnce(Query<'a>),
{
    let mut inner = MySelect(vec![]);
    let query = Query {
        names: PhantomData,
        ast: &mut inner,
    };
    f(query);
    inner.into_select(None)
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
            TestDummy { foo: f("foo") }
        }
    }
    struct TestDummy<'names> {
        foo: MyIden<'names>,
    }

    fn sub_query<'a>(q: &mut Query<'a>, val: impl Value + 'a) -> impl Value + 'a {
        q.filter(val);
        val
    }

    #[test]
    fn test() {
        new_query(|mut q| {
            let q_test = q.join(TestTable);
            let mut out = None;
            q.query(|mut g| {
                let g_test = g.join(TestTable);
                g.filter(q_test.foo);
                let mut aggr = g.by(g_test.foo);
                out = Some(aggr.average(g_test.foo));
            });
            q.filter(out.unwrap());

            // new_query(|mut p| {
            //     let test_p = p.join(TestTable);
            //     // q.filter(test_p.foo);
            //     // p.filter(test_q.foo);
            // });
            let test_q = q.join(TestTable);
            // let x = sub_query(&mut q, test_q.foo);
            // q.filter(x);
        });
    }

    fn get_match<'a>(q: &mut Query<'a>, foo: impl Value + 'a) -> impl Value + 'a {
        let test = q.join(TestTable);
        q.filter(test.foo.eq(foo));
        test.foo
    }

    fn transpose() {
        new_query(|mut q| {
            let alpha = q.join(TestTable);
            let mut beta = None;
            q.query(|mut g| {
                let res = get_match(&mut g, alpha.foo);
                let mut res = g.by(res);
                beta = Some(res.rank_asc(alpha.foo));
            });
            q.filter(alpha.foo.eq(beta.unwrap()))
        });
    }
}
