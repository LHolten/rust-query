use std::marker::PhantomData;

use sea_query::Iden;

use crate::{
    alias::Field,
    value::{DynTyped, DynTypedExpr, MyTyp},
    IntoColumn,
};

/// Opaque type used to implement [crate::Dummy].
pub struct Cacher<'columns, S> {
    pub(crate) columns: Vec<DynTypedExpr>,
    _p: PhantomData<fn(&'columns ())>,
    _p3: PhantomData<S>,
}

impl<S> Cacher<'_, S> {
    pub(crate) fn new() -> Self {
        Self {
            columns: Vec::new(),
            _p: PhantomData,
            _p3: PhantomData,
        }
    }
}

pub(crate) struct Cached<T> {
    pub(crate) idx: usize,
    _p: PhantomData<T>,
}

impl<T> Clone for Cached<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Cached<T> {}

impl<S> Cacher<'_, S> {
    pub(crate) fn cache_erased(&mut self, val: DynTypedExpr) -> usize {
        let idx = self.columns.len();
        self.columns.push(val);
        idx
    }

    pub(crate) fn cache<'a, T: 'static>(&mut self, val: DynTyped<T>) -> Cached<T> {
        Cached {
            _p: PhantomData,
            idx: self.cache_erased(val.erase()),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Row<'x> {
    pub(crate) row: &'x rusqlite::Row<'x>,
    pub(crate) fields: &'x [Field],
}

impl<'x> Row<'x> {
    pub(crate) fn new(row: &'x rusqlite::Row<'x>, fields: &'x [Field]) -> Self {
        Self { row, fields }
    }

    pub fn get<'transaction, T: MyTyp>(&self, val: Cached<T>) -> T::Out<'transaction> {
        let field = self.fields[val.idx];
        let idx = &*field.to_string();
        T::from_sql(self.row.get_ref_unwrap(idx)).unwrap()
    }
}

pub(crate) trait Prepared<'transaction> {
    type Out;

    fn call(&mut self, row: Row<'_>) -> Self::Out;
}

pub struct Package<'i, T> {
    pub(crate) inner: T,
    pub(crate) _p: PhantomData<&'i ()>,
}

impl<T> Package<'_, T> {
    pub(crate) fn new(val: T) -> Self {
        Self {
            inner: val,
            _p: PhantomData,
        }
    }
}

/// This trait is implemented by everything that can be retrieved from the database.
///
/// This trait can be automatically implemented using [rust_query_macros::Dummy].
pub trait Dummy<'columns, 'transaction, S>: Sized {
    /// The type that results from querying this dummy.
    type Out;

    /// The result of the [Dummy::into_impl] method.
    ///
    /// Just like the [Dummy::into_impl] implemenation, this should be specified
    /// using the associated types of other [Dummy] implementations.
    type Prepared: Prepared<'transaction, Out = Self::Out>;

    /// This method is what tells rust-query how to retrieve the dummy.
    ///
    /// The only way to implement this method is by constructing a different dummy and
    /// calling the [Dummy::into_impl] method on that other dummy.
    fn prepare<'i>(self, cacher: &'i mut Cacher<'columns, S>) -> Package<'i, Self::Prepared>;

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

/// This is the result of the [Dummy::map_dummy] method.
///
/// [MapDummy] retrieves the same columns as the dummy that it wraps,
/// but then it processes those columns using a rust closure.
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

    type Prepared = MapPrepared<D::Prepared, F>;

    fn prepare<'i>(self, cacher: &'i mut Cacher<'columns, S>) -> Package<'i, Self::Prepared> {
        Package::new(MapPrepared {
            inner: self.dummy.prepare(cacher).inner,
            map: self.func,
        })
    }
}

pub(crate) struct MapPrepared<X, M> {
    inner: X,
    map: M,
}

impl<'transaction, X, M, Out> Prepared<'transaction> for MapPrepared<X, M>
where
    X: Prepared<'transaction>,
    M: FnMut(X::Out) -> Out,
{
    type Out = Out;

    fn call(&mut self, row: Row<'_>) -> Self::Out {
        (self.map)(self.inner.call(row))
    }
}

impl Prepared<'_> for () {
    type Out = ();

    fn call(&mut self, _row: Row<'_>) -> Self::Out {}
}

impl<'columns, 'transaction, S> Dummy<'columns, 'transaction, S> for () {
    type Out = ();

    type Prepared = ();

    fn prepare<'i>(self, _cacher: &'i mut Cacher<'columns, S>) -> Package<'i, Self::Prepared> {
        Package::new(())
    }
}

impl<'transaction, T: MyTyp> Prepared<'transaction> for Cached<T> {
    type Out = T::Out<'transaction>;

    fn call(&mut self, row: Row<'_>) -> Self::Out {
        row.get(*self)
    }
}

impl<'columns, 'transaction, S, T> Dummy<'columns, 'transaction, S> for T
where
    T: IntoColumn<'columns, S, Typ: MyTyp>,
{
    type Out = <T::Typ as MyTyp>::Out<'transaction>;

    type Prepared = Cached<T::Typ>;

    fn prepare<'i>(self, cacher: &'i mut Cacher<'columns, S>) -> Package<'i, Self::Prepared> {
        Package::new(cacher.cache(self.into_column().inner))
    }
}

impl<'transaction, A, B> Prepared<'transaction> for (A, B)
where
    A: Prepared<'transaction>,
    B: Prepared<'transaction>,
{
    type Out = (A::Out, B::Out);

    fn call(&mut self, row: Row<'_>) -> Self::Out {
        (self.0.call(row), self.1.call(row))
    }
}

impl<'columns, 'transaction, S, A, B> Dummy<'columns, 'transaction, S> for (A, B)
where
    A: Dummy<'columns, 'transaction, S>,
    B: Dummy<'columns, 'transaction, S>,
{
    type Out = (A::Out, B::Out);

    type Prepared = (A::Prepared, B::Prepared);

    fn prepare<'i>(self, cacher: &'i mut Cacher<'columns, S>) -> Package<'i, Self::Prepared> {
        let prepared_a = self.0.prepare(cacher).inner;
        let prepared_b = self.1.prepare(cacher).inner;
        Package::new((prepared_a, prepared_b))
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
        type Prepared = <MapDummy<(A, B), fn((i64, String)) -> User> as Dummy<'t, 'a, S>>::Prepared;

        fn prepare<'i>(self, cacher: &'i mut Cacher<'t, S>) -> Package<'i, Self::Prepared> {
            (self.a, self.b)
                .map_dummy((|(a, b)| User { a, b }) as fn((i64, String)) -> User)
                .prepare(cacher)
        }
    }
}
