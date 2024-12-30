use std::marker::PhantomData;

use sea_query::Iden;

use crate::{
    alias::Field,
    value::{DynTypedExpr, MyTyp},
    IntoColumn,
};

pub struct Cacher<'t, 'i, S> {
    pub(crate) _p: PhantomData<fn(&'t ()) -> &'i ()>,
    pub(crate) _p2: PhantomData<S>,
    pub(crate) columns: Vec<DynTypedExpr>,
}

pub struct Cached<'t, T> {
    pub(crate) _p: PhantomData<fn(&'t T) -> &'t T>,
    pub(crate) idx: usize,
}

impl<'t, T> Clone for Cached<'t, T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<'t, T> Copy for Cached<'t, T> {}

impl<'t, 'i, S> Cacher<'t, 'i, S> {
    pub(crate) fn cache_erased(&mut self, val: DynTypedExpr) -> usize {
        let idx = self.columns.len();
        self.columns.push(val);
        idx
    }

    pub fn cache<T: 'static>(&mut self, val: impl IntoColumn<'t, S, Typ = T>) -> Cached<'i, T> {
        let val = val.into_column().inner;

        Cached {
            _p: PhantomData,
            idx: self.cache_erased(val.erase()),
        }
    }
}

#[derive(Clone, Copy)]
pub struct Row<'x, 'i, 'a> {
    pub(crate) _p: PhantomData<fn(&'i ()) -> &'a ()>,
    pub(crate) row: &'x rusqlite::Row<'x>,
    pub(crate) fields: &'x [Field],
}

impl<'i, 'a> Row<'_, 'i, 'a> {
    pub fn get<T: MyTyp>(&self, val: Cached<'i, T>) -> T::Out<'a> {
        let field = self.fields[val.idx];
        let idx = &*field.to_string();
        T::from_sql(self.row.get_ref_unwrap(idx)).unwrap()
    }
}

pub struct Prepared<'l, 'i, 'a, Out> {
    pub(crate) inner: Box<dyn 'l + FnMut(&rusqlite::Row, &[Field]) -> Out>,
    _p1: PhantomData<&'l &'i ()>,
    _p2: PhantomData<&'l &'a ()>,
    _p3: PhantomData<&'l Out>,
}

impl<'l, 'i, 'a, Out> Prepared<'l, 'i, 'a, Out> {
    pub fn new(mut func: impl 'l + FnMut(Row<'_, 'i, 'a>) -> Out) -> Self {
        Prepared {
            inner: Box::new(move |row, fields| {
                let wrapped = Row {
                    row,
                    fields,
                    _p: PhantomData,
                };
                func(wrapped)
            }),
            _p1: PhantomData,
            _p2: PhantomData,
            _p3: PhantomData,
        }
    }

    pub fn call(&mut self, row: Row<'_, 'i, 'a>) -> Out {
        (self.inner)(row.row, row.fields)
    }
}

/// This trait is implemented by everything that can be retrieved from the database.
///
/// Implement it on custom structs using [crate::FromDummy].
pub trait Dummy<'columns, 'captures, 'transaction, S>: Sized {
    /// The type that results from querying this dummy.
    type Out: 'captures;

    #[doc(hidden)]
    fn prepare<'i>(
        self,
        cacher: &mut Cacher<'columns, 'i, S>,
    ) -> Prepared<'captures, 'i, 'transaction, Self::Out>;

    /// Map a dummy to another dummy using native rust.
    ///
    /// This is useful when retrieving a struct from the database that contains types not supported by the database.
    /// It is also useful in migrations to process rows using arbitrary rust.
    fn map_dummy<T>(
        self,
        mut f: impl 'captures + FnMut(Self::Out) -> T,
    ) -> PubDummy<'columns, 'captures, 'transaction, S, T> {
        let mut d = DynDummy::new(self);
        let d = DynDummy {
            columns: d.columns,
            func: Box::new(move |row, fields| f((d.func)(row, fields))),
            // _p2: PhantomData,
        };
        PubDummy::new(d)
    }
}

/// The internal presentation of [PubDummy], the lifetimes here are the ones necessary for memory safety.
pub(crate) struct DynDummy<'l, Out> {
    pub columns: Vec<DynTypedExpr>,
    pub func: Box<dyn 'l + FnMut(&rusqlite::Row, &[Field]) -> Out>,
}

impl<'l, Out> DynDummy<'l, Out> {
    pub fn new<'t, 'a: 'l, S>(val: impl Dummy<'t, 'l, 'a, S, Out = Out>) -> Self
    where
        Out: 'l,
    {
        let mut cacher = Cacher {
            _p: PhantomData,
            _p2: PhantomData,
            columns: vec![],
        };
        let prepared = val.prepare(&mut cacher);
        DynDummy {
            columns: cacher.columns,
            func: prepared.inner,
        }
    }
}
pub struct PubDummy<'columns, 'captures, 'transaction, S, Out> {
    pub(crate) inner: DynDummy<'captures, Out>,
    _p: PhantomData<fn(&'columns ()) -> &'columns ()>,
    _p2: PhantomData<S>,
    _p3: PhantomData<&'captures Out>,
    _p4: PhantomData<&'captures &'transaction ()>,
}

impl<'captures, S, Out> PubDummy<'_, 'captures, '_, S, Out> {
    pub(crate) fn new(val: DynDummy<'captures, Out>) -> Self {
        Self {
            inner: val,
            _p: PhantomData,
            _p2: PhantomData,
            _p3: PhantomData,
            _p4: PhantomData,
        }
    }
}

impl<'outer, 'captures, 'transaction, S, Out> Dummy<'outer, 'captures, 'transaction, S>
    for PubDummy<'outer, 'captures, 'transaction, S, Out>
{
    type Out = Out;

    fn prepare<'i>(
        mut self,
        cacher: &mut Cacher<'_, 'i, S>,
    ) -> Prepared<'captures, 'i, 'transaction, Self::Out> {
        let mut diff = None;
        self.inner
            .columns
            .into_iter()
            .enumerate()
            .for_each(|(old, x)| {
                let new = cacher.cache_erased(x);
                let _diff = new - old;
                debug_assert!(diff.is_none_or(|it| it == _diff));
                diff = Some(_diff);
            });
        let diff = diff.unwrap_or_default();
        Prepared::new(move |row| (self.inner.func)(row.row, &row.fields[diff..]))
    }
}

impl<'t, 'a, S, T: IntoColumn<'t, S, Typ: MyTyp>> Dummy<'t, 'a, 'a, S> for T {
    type Out = <T::Typ as MyTyp>::Out<'a>;

    fn prepare<'i>(self, cacher: &mut Cacher<'t, 'i, S>) -> Prepared<'a, 'i, 'a, Self::Out> {
        let cached = cacher.cache(self);
        Prepared::new(move |row| row.get(cached))
    }
}

impl<'t, 'l, 'a, S, A: Dummy<'t, 'l, 'a, S>, B: Dummy<'t, 'l, 'a, S>> Dummy<'t, 'l, 'a, S>
    for (A, B)
{
    type Out = (A::Out, B::Out);

    fn prepare<'i>(self, cacher: &mut Cacher<'t, 'i, S>) -> Prepared<'l, 'i, 'a, Self::Out> {
        let mut prepared_a = self.0.prepare(cacher);
        let mut prepared_b = self.1.prepare(cacher);
        Prepared::new(move |row| (prepared_a.call(row), prepared_b.call(row)))
    }
}

#[cfg(test)]
#[allow(unused)]
mod tests {
    use super::*;

    struct User {
        a: i64,
        b: String,
    }

    struct UserDummy<A, B> {
        a: A,
        b: B,
    }

    impl<'t, 'l, 'a, S, A, B> Dummy<'t, 'l, 'a, S> for UserDummy<A, B>
    where
        A: IntoColumn<'t, S, Typ = i64>,
        B: IntoColumn<'t, S, Typ = String>,
    {
        type Out = User;

        fn prepare<'i>(self, cacher: &mut Cacher<'t, 'i, S>) -> Prepared<'l, 'i, 'a, Self::Out> {
            let a = cacher.cache(self.a);
            let b = cacher.cache(self.b);
            Prepared::new(move |row| User {
                a: row.get(a),
                b: row.get(b),
            })
        }
    }
}
