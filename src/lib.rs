mod value;

use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

// Query is only valid if `names` are in scope
pub struct Query<'names> {
    names: PhantomData<&'names ()>,
}

#[derive(Clone, Copy)]
pub struct Value<'names> {
    names: PhantomData<&'names ()>,
}

pub trait Table {
    // these names are defined in `'query`
    type Dummy<'names>;
}

impl<'names> Query<'names> {
    pub fn join<T: Table>(&mut self, _t: T) -> T::Dummy<'names> {
        todo!()
    }

    // join another query that is grouped by some value
    pub fn query<F>(&mut self, f: F)
    where
        F: for<'a> FnOnce(Group<'a, 'names>),
    {
        todo!()
    }

    pub fn filter(&mut self, prop: Value<'names>) {}
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
    pub fn rank_asc(&mut self, by: Value<'a>) -> Value<'a> {
        todo!()
    }

    fn by(self, val: Value<'a>) -> Aggr<'a, 'names> {
        Aggr {
            group: self,
            by: val,
        }
    }
}

pub struct Aggr<'a, 'names> {
    group: Group<'a, 'names>,
    by: Value<'a>,
}

impl<'a, 'names> Aggr<'a, 'names> {
    fn values(&mut self) -> Value<'names> {
        todo!()
    }

    fn average(&mut self, val: Value<'a>) -> Value<'names> {
        todo!()
    }

    fn count(&mut self) -> Value<'names> {
        todo!()
    }
}

pub fn new_query<F>(f: F)
where
    F: for<'a> FnOnce(Query<'a>),
{
    f(Query { names: PhantomData })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestTable;

    impl Table for TestTable {
        type Dummy<'names> = TestDummy<'names>;
    }
    struct TestDummy<'names> {
        foo: Value<'names>,
    }

    fn sub_query<'a>(q: &mut Query<'a>, val: Value<'a>) -> Value<'a> {
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
        })
    }

    fn get_match<'a>(q: &mut Query<'a>, foo: Value<'a>) -> Value<'a> {
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
                beta = Some(res.values());
            });
            q.filter(alpha.foo.eq(beta.unwrap()))
        })
    }
}
