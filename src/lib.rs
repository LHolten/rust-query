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
    pub fn group_by<F>(&mut self, f: F)
    where
        F: for<'a> FnOnce(Group<'a, 'names>) -> Value<'a>,
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
    fn average(&mut self, val: Value<'a>) -> Value<'names> {
        todo!()
    }
}

pub fn new_query<F>(f: F)
where
    F: for<'a> FnOnce(&'a mut Query<'a>),
{
    let mut q = Query { names: PhantomData };
    f(&mut q)
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
            let mut out = None;
            q.group_by(|mut g| {
                let g_test = g.join(TestTable);
                out = Some(g.average(g_test.foo));
                g_test.foo
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
}
