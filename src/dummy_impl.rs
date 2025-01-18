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

impl<S> Cacher<'_, '_, S> {
    pub(crate) fn new() -> Self {
        Self {
            _p: PhantomData,
            _p2: PhantomData,
            columns: Vec::new(),
        }
    }
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

    pub(crate) fn cache<T: 'static>(
        &mut self,
        val: impl IntoColumn<'t, S, Typ = T>,
    ) -> Cached<'i, T> {
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

pub trait Prepared<'i, 'transaction> {
    type Out;

    fn call(&mut self, row: Row<'_, 'i, 'transaction>) -> Self::Out;
}

/// This trait is implemented by everything that can be retrieved from the database.
///
/// This trait can be automatically implemented using [rust_query_macros::Dummy].
pub trait Dummy<'columns, 'transaction, S>: Sized {
    /// The type that results from querying this dummy.
    type Out;

    /// The result of the [Dummy::prepare] method.
    ///
    /// Just like the [Dummy::prepare] implemenation, this should be specified
    /// using the associated types of other [Dummy] implementations.
    type Prepared<'i>: Prepared<'i, 'transaction, Out = Self::Out>;

    /// This method is what tells rust-query how to retrieve the dummy.
    ///
    /// The only way to implement this method is by constructing a different dummy and
    /// calling the [Dummy::prepare] method on that other dummy.
    fn prepare<'i>(self, cacher: &mut Cacher<'columns, 'i, S>) -> Self::Prepared<'i>;

    /// Map a dummy to another dummy using native rust.
    ///
    /// This is useful when retrieving a struct from the database that contains types not supported by the database.
    /// It is also useful in migrations to process rows using arbitrary rust.
    fn map_dummy<T, F: FnMut(Self::Out) -> T>(self, f: F) -> MapDummy<Self, F> {
        MapDummy {
            dummy: self,
            func: f,
        }
    }
}

pub struct MapDummy<D, F> {
    dummy: D,
    func: F,
}

impl<'columns, 'transaction, S, D, F, O> Dummy<'columns, 'transaction, S> for MapDummy<D, F>
where
    D: Dummy<'columns, 'transaction, S>,
    F: FnMut(D::Out) -> O,
{
    type Out = O;

    type Prepared<'i> = MapPrepared<D::Prepared<'i>, F>;

    fn prepare<'i>(self, cacher: &mut Cacher<'columns, 'i, S>) -> Self::Prepared<'i> {
        MapPrepared {
            inner: self.dummy.prepare(cacher),
            map: self.func,
        }
    }
}

pub struct MapPrepared<X, M> {
    inner: X,
    map: M,
}

impl<'i, 'transaction, X, M, Out> Prepared<'i, 'transaction> for MapPrepared<X, M>
where
    X: Prepared<'i, 'transaction>,
    M: FnMut(X::Out) -> Out,
{
    type Out = Out;

    fn call(&mut self, row: Row<'_, 'i, 'transaction>) -> Self::Out {
        (self.map)(self.inner.call(row))
    }
}

impl<'i, 'a> Prepared<'i, 'a> for () {
    type Out = ();

    fn call(&mut self, _row: Row<'_, 'i, 'a>) -> Self::Out {}
}

impl<'t, 'a, S> Dummy<'t, 'a, S> for () {
    type Out = ();
    type Prepared<'i> = ();

    fn prepare<'i>(self, _cacher: &mut Cacher<'t, 'i, S>) -> Self::Prepared<'i> {}
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
