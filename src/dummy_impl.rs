use std::marker::PhantomData;

use sea_query::Iden;

use crate::{
    alias::Field,
    value::{DynTyped, DynTypedExpr, MyTyp, SecretFromSql},
    Column,
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

    pub(crate) fn cache<'a, T: MyTyp>(&mut self, val: DynTyped<T>) -> Cached<T::Out<'a>> {
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

    pub fn get<T: SecretFromSql>(&self, val: Cached<T>) -> T {
        let field = self.fields[val.idx];
        let idx = &*field.to_string();
        T::from_sql(self.row.get_ref_unwrap(idx)).unwrap()
    }
}

pub(crate) trait Prepared {
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

// pub struct NewPackage<'columns, 'transaction, S, T> {
//     pub(crate) inner: T,
//     pub(crate) _p: PhantomData<&'columns ()>,
//     pub(crate) _p2: PhantomData<&'transaction ()>,
//     pub(crate) _p3: PhantomData<S>,
// }

// impl<S, T> NewPackage<'_, '_, S, T> {
//     pub(crate) fn new(val: T) -> Self {
//         Self {
//             inner: val,
//             _p: PhantomData,
//             _p2: PhantomData,
//             _p3: PhantomData,
//         }
//     }
// }

// pub trait IntoDummyParent<'transaction> {
//     type Dummy;
// }

// pub trait IntoDummy<'columns, 'transaction, S>: IntoDummyParent<'transaction> {
//     fn into_dummy(self) -> NewPackage<'columns, 'transaction, S, Self::Dummy>;
// }

pub trait DummyParent<'transaction>: Sized {
    /// The type that results from querying this dummy.
    type Out;

    /// The result of the [Dummy::into_impl] method.
    ///
    /// Just like the [Dummy::into_impl] implemenation, this should be specified
    /// using the associated types of other [Dummy] implementations.
    type Prepared: Prepared<Out = Self::Out>;
}

/// This trait is implemented by everything that can be retrieved from the database.
///
/// This trait can be automatically implemented using [rust_query_macros::Dummy].
pub trait Dummy<'columns, 'transaction, S>: DummyParent<'transaction> {
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

impl<'transaction, D, F, O> DummyParent<'transaction> for MapDummy<D, F>
where
    D: DummyParent<'transaction>,
    F: FnMut(D::Out) -> O,
{
    type Out = O;

    type Prepared = MapPrepared<D::Prepared, F>;
}

impl<'columns, 'transaction, S, D, F, O> Dummy<'columns, 'transaction, S> for MapDummy<D, F>
where
    D: Dummy<'columns, 'transaction, S>,
    F: FnMut(D::Out) -> O,
{
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

impl<X, M, Out> Prepared for MapPrepared<X, M>
where
    X: Prepared,
    M: FnMut(X::Out) -> Out,
{
    type Out = Out;

    fn call(&mut self, row: Row<'_>) -> Self::Out {
        (self.map)(self.inner.call(row))
    }
}

impl Prepared for () {
    type Out = ();

    fn call(&mut self, _row: Row<'_>) -> Self::Out {}
}

impl<'transaction> DummyParent<'transaction> for () {
    type Out = ();

    type Prepared = ();
}

impl<'columns, 'transaction, S> Dummy<'columns, 'transaction, S> for () {
    fn prepare<'i>(self, _cacher: &'i mut Cacher<'columns, S>) -> Package<'i, Self::Prepared> {
        Package::new(())
    }
}

impl<T: SecretFromSql> Prepared for Cached<T> {
    type Out = T;

    fn call(&mut self, row: Row<'_>) -> Self::Out {
        row.get(*self)
    }
}

impl<'columns, 'transaction, S, T: MyTyp> DummyParent<'transaction> for Column<'columns, S, T> {
    type Out = T::Out<'transaction>;

    type Prepared = Cached<T::Out<'transaction>>;
}

impl<'columns, 'transaction, S, T: MyTyp> Dummy<'columns, 'transaction, S>
    for Column<'columns, S, T>
{
    fn prepare<'i>(self, cacher: &'i mut Cacher<'columns, S>) -> Package<'i, Self::Prepared> {
        Package::new(cacher.cache(self.inner))
    }
}

impl<'transaction, A, B> Prepared for (A, B)
where
    A: Prepared,
    B: Prepared,
{
    type Out = (A::Out, B::Out);

    fn call(&mut self, row: Row<'_>) -> Self::Out {
        (self.0.call(row), self.1.call(row))
    }
}

impl<'transaction, A, B> DummyParent<'transaction> for (A, B)
where
    A: DummyParent<'transaction>,
    B: DummyParent<'transaction>,
{
    type Out = (A::Out, B::Out);

    type Prepared = (A::Prepared, B::Prepared);
}

impl<'columns, 'transaction, S, A, B> Dummy<'columns, 'transaction, S> for (A, B)
where
    A: Dummy<'columns, 'transaction, S>,
    B: Dummy<'columns, 'transaction, S>,
{
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

    // impl<'t, 'a, S, A, B> Dummy<'t, 'a, S> for UserDummy<A, B>
    // where
    //     A: IntoColumn<'t, S, Typ = i64>,
    //     B: IntoColumn<'t, S, Typ = String>,
    // {
    //     type Out = User;
    //     type Prepared = <MapDummy<(A, B), fn((i64, String)) -> User> as Dummy<'t, 'a, S>>::Prepared;

    //     fn prepare<'i>(self, cacher: &'i mut Cacher<'t, S>) -> Package<'i, Self::Prepared> {
    //         (self.a, self.b)
    //             .map_dummy((|(a, b)| User { a, b }) as fn((i64, String)) -> User)
    //             .prepare(cacher)
    //     }
    // }
}
