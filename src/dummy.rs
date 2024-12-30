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
    _p: PhantomData<fn(&'t T) -> &'t T>,
    idx: usize,
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
    pub(crate) _p2: PhantomData<&'i &'a ()>,
    pub(crate) row: &'x rusqlite::Row<'x>,
    pub(crate) mapping: &'x [Field],
}

impl<'i, 'a> Row<'_, 'i, 'a> {
    pub fn get<T: MyTyp>(&self, val: Cached<'i, T>) -> T::Out<'a> {
        let field = self.mapping[val.idx];
        let idx = &*field.to_string();
        T::from_sql(self.row.get_ref_unwrap(idx)).unwrap()
    }
}

/// Add the implied bound `T: 'i` which can not be added as `Self::Out: 'i` for some reason
pub struct Wrapped<'i, T>(pub T, pub(crate) PhantomData<&'i T>);
impl<'i, T> Wrapped<'i, T> {
    pub fn new(val: T) -> Self {
        Self(val, PhantomData)
    }
}

pub struct Prepared<'i, 'a, Out> {
    inner: Box<dyn 'i + FnMut(Row<'_, 'i, 'a>) -> Wrapped<'i, Out>>,
}

impl<'i, 'a, Out> Prepared<'i, 'a, Out> {
    pub fn new(func: impl 'i + FnMut(Row<'_, 'i, 'a>) -> Wrapped<'i, Out>) -> Self {
        Prepared {
            inner: Box::new(func),
        }
    }

    pub fn call(&mut self, row: Row<'_, 'i, 'a>) -> Wrapped<'i, Out> {
        (self.inner)(row)
    }
}

/// This trait is implemented by everything that can be retrieved from the database.
///
/// Implement it on custom structs using [crate::FromDummy].
pub trait Dummy<'t, 'a, S>: Sized {
    /// The type that results from querying this dummy.
    type Out;

    #[doc(hidden)]
    fn prepare<'i>(self, cacher: &mut Cacher<'t, 'i, S>) -> Prepared<'i, 'a, Self::Out>;

    /// Map a dummy to another dummy using native rust.
    ///
    /// This is useful when retrieving a struct from the database that contains types not supported by the database.
    /// It is also useful in migrations to process rows using arbitrary rust.
    fn map_dummy<T>(self, f: impl 'a + FnMut(Self::Out) -> T) -> impl Dummy<'t, 'a, S, Out = T>
    where
        Self::Out: 'a, // this bound is not too bad, because the mapped dummy is probably one of the database ones
    {
        DummyMap(self, f)
    }
}

pub struct DynDummy<'a, Out> {
    pub(crate) columns: Vec<DynTypedExpr>,
    pub(crate) func: Prepared<'a, 'a, Out>,
}

impl<'a, Out> DynDummy<'a, Out> {
    pub fn new<'t, S>(val: impl Dummy<'t, 'a, S, Out = Out>) -> Self {
        let mut cacher = Cacher {
            _p: PhantomData,
            _p2: PhantomData,
            columns: vec![],
        };
        let prepared = val.prepare(&mut cacher);
        DynDummy {
            columns: cacher.columns,
            func: prepared,
        }
    }
}
pub struct PubDummy<'outer, 'transaction, S, Out> {
    pub(crate) inner: DynDummy<'transaction, Out>,
    pub(crate) _p: PhantomData<fn(&'outer ()) -> &'outer ()>,
    pub(crate) _p2: PhantomData<S>,
}

impl<'outer, 'transaction, S, Out> Dummy<'outer, 'transaction, S>
    for PubDummy<'outer, 'transaction, S, Out>
{
    type Out = Out;

    fn prepare<'i>(
        mut self,
        cacher: &mut Cacher<'_, 'i, S>,
    ) -> Prepared<'i, 'transaction, Self::Out> {
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
        Prepared::new(move |row| {
            let row = Row {
                _p: PhantomData,
                _p2: PhantomData,
                row: row.row,
                mapping: &row.mapping[diff..],
            };
            self.inner.func.call(row)
        })
    }
}

struct DummyMap<A, F>(A, F);

impl<'t, 'a, S, A, F, T> Dummy<'t, 'a, S> for DummyMap<A, F>
where
    A: Dummy<'t, 'a, S>,
    F: 'a + FnMut(A::Out) -> T,
    A::Out: 'a,
{
    type Out = T;

    fn prepare<'i>(mut self, cacher: &mut Cacher<'t, 'i, S>) -> Prepared<'i, 'a, Self::Out> {
        let mut cached = self.0.prepare(cacher);
        Prepared::new(move |row| Wrapped::new(self.1(cached.call(row).0)))
    }
}

impl<'t, 'a, S, T: IntoColumn<'t, S, Typ: MyTyp>> Dummy<'t, 'a, S> for T {
    type Out = <T::Typ as MyTyp>::Out<'a>;

    fn prepare<'i>(self, cacher: &mut Cacher<'t, 'i, S>) -> Prepared<'i, 'a, Self::Out> {
        let cached = cacher.cache(self);
        Prepared::new(move |row| Wrapped::new(row.get(cached)))
    }
}

impl<'t, 'a, S, A: Dummy<'t, 'a, S>, B: Dummy<'t, 'a, S>> Dummy<'t, 'a, S> for (A, B) {
    type Out = (A::Out, B::Out);

    fn prepare<'i>(self, cacher: &mut Cacher<'t, 'i, S>) -> Prepared<'i, 'a, Self::Out> {
        let mut prepared_a = self.0.prepare(cacher);
        let mut prepared_b = self.1.prepare(cacher);
        Prepared::new(move |row| Wrapped::new((prepared_a.call(row).0, prepared_b.call(row).0)))
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

    impl<'t, 'a, S, A, B> Dummy<'t, 'a, S> for UserDummy<A, B>
    where
        A: IntoColumn<'t, S, Typ = i64>,
        B: IntoColumn<'t, S, Typ = String>,
    {
        type Out = User;

        fn prepare<'i>(self, cacher: &mut Cacher<'t, 'i, S>) -> Prepared<'i, 'a, Self::Out> {
            let a = cacher.cache(self.a);
            let b = cacher.cache(self.b);
            Prepared::new(move |row| {
                Wrapped::new(User {
                    a: row.get(a),
                    b: row.get(b),
                })
            })
        }
    }
}
