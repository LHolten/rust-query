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

pub struct Cached<'i, T> {
    pub(crate) _p: PhantomData<fn(&'i T) -> &'i T>,
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

impl<'x, 'i, 'a> Row<'x, 'i, 'a> {
    pub(crate) fn new(row: &'x rusqlite::Row<'x>, fields: &'x [Field]) -> Self {
        Self {
            row,
            fields,
            _p: PhantomData,
        }
    }

    pub fn get<T: MyTyp>(&self, val: Cached<'i, T>) -> T::Out<'a> {
        let field = self.fields[val.idx];
        let idx = &*field.to_string();
        T::from_sql(self.row.get_ref_unwrap(idx)).unwrap()
    }
}

pub trait Prepared<'i, 'a> {
    type Out;

    fn call(&mut self, row: Row<'_, 'i, 'a>) -> Self::Out;
}

/// This trait is implemented by everything that can be retrieved from the database.
///
/// Implement it on custom structs using [crate::FromDummy].
pub trait Dummy<'columns, 'transaction, S>: Sized {
    /// The type that results from querying this dummy.
    type Out;

    #[doc(hidden)]
    type Prepared<'i>: Prepared<'i, 'transaction, Out = Self::Out>;

    #[doc(hidden)]
    fn prepare<'i>(self, cacher: &mut Cacher<'columns, 'i, S>) -> Self::Prepared<'i>;

    /// Map a dummy to another dummy using native rust.
    ///
    /// This is useful when retrieving a struct from the database that contains types not supported by the database.
    /// It is also useful in migrations to process rows using arbitrary rust.
    fn map_dummy<T, F: FnMut(Self::Out) -> T>(
        self,
        f: F,
    ) -> impl Dummy<'columns, 'transaction, S, Out = T> {
        let d = PubDummy::new(self);
        PubDummy {
            columns: d.columns,
            inner: MapPrepared {
                inner: d.inner,
                map: f,
            },
            _p: PhantomData,
            _p2: PhantomData,
        }
    }
}

pub struct MapPrepared<X, M> {
    inner: X,
    map: M,
}

impl<'transaction, X, M, Out> Prepared<'static, 'transaction> for MapPrepared<X, M>
where
    X: Prepared<'static, 'transaction>,
    M: FnMut(X::Out) -> Out,
{
    type Out = Out;

    fn call(&mut self, row: Row<'_, 'static, 'transaction>) -> Self::Out {
        (self.map)(self.inner.call(row))
    }
}

pub struct DynDummy<X> {
    offset: usize,
    func: X,
}

impl<'i, 'a, X: Prepared<'static, 'a>> Prepared<'i, 'a> for DynDummy<X> {
    type Out = X::Out;

    fn call(&mut self, row: Row<'_, 'i, 'a>) -> Self::Out {
        self.func
            .call(Row::new(row.row, &row.fields[self.offset..]))
    }
}

/// Erases the `'i` lifetime
pub struct PubDummy<'columns, S, X> {
    pub(crate) columns: Vec<DynTypedExpr>,
    pub(crate) inner: X,
    pub(crate) _p: PhantomData<fn(&'columns ()) -> &'columns ()>,
    pub(crate) _p2: PhantomData<S>,
}

impl<'columns, S, X> PubDummy<'columns, S, X> {
    pub fn new<'a>(val: impl Dummy<'columns, 'a, S, Prepared<'static> = X>) -> Self {
        let mut cacher = Cacher {
            _p: PhantomData,
            _p2: PhantomData,
            columns: vec![],
        };
        PubDummy {
            inner: val.prepare(&mut cacher),
            columns: cacher.columns,
            _p: PhantomData,
            _p2: PhantomData,
        }
    }
}

impl<'columns, 'transaction, S, X: Prepared<'static, 'transaction>> Dummy<'columns, 'transaction, S>
    for PubDummy<'columns, S, X>
{
    type Out = X::Out;
    type Prepared<'i> = DynDummy<X>;

    fn prepare<'i>(self, cacher: &mut Cacher<'_, 'i, S>) -> Self::Prepared<'i> {
        let mut diff = None;
        self.columns.into_iter().enumerate().for_each(|(old, x)| {
            let new = cacher.cache_erased(x);
            let _diff = new - old;
            debug_assert!(diff.is_none_or(|it| it == _diff));
            diff = Some(_diff);
        });
        let diff = diff.unwrap_or_default();
        DynDummy {
            offset: diff,
            func: self.inner,
        }
    }
}

impl<'i, 'a, T: MyTyp> Prepared<'i, 'a> for Cached<'i, T> {
    type Out = T::Out<'a>;

    fn call(&mut self, row: Row<'_, 'i, 'a>) -> Self::Out {
        row.get(*self)
    }
}

impl<'t, 'a, S, T: IntoColumn<'t, S, Typ: MyTyp>> Dummy<'t, 'a, S> for T {
    type Out = <T::Typ as MyTyp>::Out<'a>;
    type Prepared<'i> = Cached<'i, T::Typ>;

    fn prepare<'i>(self, cacher: &mut Cacher<'t, 'i, S>) -> Self::Prepared<'i> {
        cacher.cache(self)
    }
}

impl<'i, 'a, A: Prepared<'i, 'a>, B: Prepared<'i, 'a>> Prepared<'i, 'a> for (A, B) {
    type Out = (A::Out, B::Out);

    fn call(&mut self, row: Row<'_, 'i, 'a>) -> Self::Out {
        (self.0.call(row), self.1.call(row))
    }
}

impl<'t, 'a, S, A: Dummy<'t, 'a, S>, B: Dummy<'t, 'a, S>> Dummy<'t, 'a, S> for (A, B) {
    type Out = (A::Out, B::Out);
    type Prepared<'i> = (A::Prepared<'i>, B::Prepared<'i>);

    fn prepare<'i>(self, cacher: &mut Cacher<'t, 'i, S>) -> Self::Prepared<'i> {
        let prepared_a = self.0.prepare(cacher);
        let prepared_b = self.1.prepare(cacher);
        (prepared_a, prepared_b)
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

    impl<'i, 'a, A, B> Prepared<'i, 'a> for UserDummy<A, B>
    where
        A: Prepared<'i, 'a, Out = i64>,
        B: Prepared<'i, 'a, Out = String>,
    {
        type Out = User;

        fn call(&mut self, row: Row<'_, 'i, 'a>) -> Self::Out {
            User {
                a: self.a.call(row),
                b: self.b.call(row),
            }
        }
    }

    impl<'t, 'a, S, A, B> Dummy<'t, 'a, S> for UserDummy<A, B>
    where
        A: IntoColumn<'t, S, Typ = i64>,
        B: IntoColumn<'t, S, Typ = String>,
    {
        type Out = User;
        type Prepared<'i> = UserDummy<Cached<'i, i64>, Cached<'i, String>>;

        fn prepare<'i>(self, cacher: &mut Cacher<'t, 'i, S>) -> Self::Prepared<'i> {
            let a = cacher.cache(self.a);
            let b = cacher.cache(self.b);
            UserDummy { a, b }
        }
    }
}
