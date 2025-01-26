use std::marker::PhantomData;

use sea_query::Iden;

use crate::{
    alias::Field,
    value::{DynTyped, DynTypedExpr, MyTyp, SecretFromSql},
    IntoColumn,
};

/// Opaque type used to implement [crate::Dummy].
pub(crate) struct Cacher {
    pub(crate) columns: Vec<DynTypedExpr>,
}

impl Cacher {
    pub(crate) fn new() -> Self {
        Self {
            columns: Vec::new(),
        }
    }
}

pub struct Cached<T> {
    pub(crate) idx: usize,
    _p: PhantomData<T>,
}

impl<T> Clone for Cached<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Cached<T> {}

impl Cacher {
    pub(crate) fn cache_erased(&mut self, val: DynTypedExpr) -> usize {
        let idx = self.columns.len();
        self.columns.push(val);
        idx
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

    pub fn get<'transaction, T: SecretFromSql<'transaction>>(&self, val: Cached<T>) -> T {
        let field = self.fields[val.idx];
        let idx = &*field.to_string();
        T::from_sql(self.row.get_ref_unwrap(idx)).unwrap()
    }
}

pub(crate) trait Prepared {
    type Out;

    fn call(&mut self, row: Row<'_>) -> Self::Out;
}

pub struct Dummy<'columns, 'transaction, S, Out> {
    pub(crate) inner: DynDummyImpl<'transaction, Out>,
    pub(crate) _p: PhantomData<&'columns ()>,
    pub(crate) _p2: PhantomData<S>,
}

pub struct DynDummyImpl<'transaction, Out> {
    inner: Box<dyn 'transaction + FnOnce(&mut Cacher) -> DynPrepared<'transaction, Out>>,
}

impl<'transaction, Out> DummyImpl<'transaction> for DynDummyImpl<'transaction, Out> {
    type Out = Out;
    type Prepared = DynPrepared<'transaction, Out>;

    fn prepare(self, cacher: &mut Cacher) -> Self::Prepared {
        (self.inner)(cacher)
    }
}

pub struct DynPrepared<'transaction, Out> {
    inner: Box<dyn 'transaction + Prepared<Out = Out>>,
}

impl<Out> Prepared for DynPrepared<'_, Out> {
    type Out = Out;
    fn call(&mut self, row: Row<'_>) -> Self::Out {
        self.inner.call(row)
    }
}

impl<'transaction, S, Out> Dummy<'_, 'transaction, S, Out> {
    pub(crate) fn new(val: impl 'transaction + DummyImpl<'transaction, Out = Out>) -> Self {
        Self {
            inner: DynDummyImpl {
                inner: Box::new(|cacher| DynPrepared {
                    inner: Box::new(val.prepare(cacher)),
                }),
            },
            _p: PhantomData,
            _p2: PhantomData,
        }
    }
}

impl<'columns, 'transaction, S, Out: 'transaction> IntoDummy<'columns, 'transaction, S>
    for Dummy<'columns, 'transaction, S, Out>
{
    type Out = Out;

    fn into_dummy(self) -> Dummy<'columns, 'transaction, S, Self::Out> {
        self
    }
}

pub trait DummyImpl<'transaction> {
    type Out;
    #[doc(hidden)]
    type Prepared: Prepared<Out = Self::Out>;
    #[doc(hidden)]
    fn prepare(self, cacher: &mut Cacher) -> Self::Prepared;
}

/// This trait is implemented by everything that can be retrieved from the database.
///
/// This trait can be automatically implemented using [rust_query_macros::Dummy].
pub trait IntoDummy<'columns, 'transaction, S>: Sized {
    /// The type that results from querying this dummy.
    type Out: 'transaction;

    /// This method is what tells rust-query how to retrieve the dummy.
    ///
    /// The only way to implement this method is by constructing a different dummy and
    /// calling the [IntoDummy::into_dummy] method on that other dummy.
    fn into_dummy(self) -> Dummy<'columns, 'transaction, S, Self::Out>;

    /// Map a dummy to another dummy using native rust.
    ///
    /// This is useful when retrieving a struct from the database that contains types not supported by the database.
    /// It is also useful in migrations to process rows using arbitrary rust.
    fn map_dummy<T, F: FnMut(Self::Out) -> T + 'transaction>(
        self,
        f: F,
    ) -> Dummy<'columns, 'transaction, S, T> {
        Dummy::new(MapImpl {
            dummy: self.into_dummy().inner,
            func: f,
        })
    }
}

/// This is the result of the [Dummy::map_dummy] method.
///
/// [MapImpl] retrieves the same columns as the dummy that it wraps,
/// but then it processes those columns using a rust closure.
pub struct MapImpl<D, F> {
    dummy: D,
    func: F,
}

impl<'transaction, D, F, O> DummyImpl<'transaction> for MapImpl<D, F>
where
    D: DummyImpl<'transaction>,
    F: FnMut(D::Out) -> O,
{
    type Out = O;
    type Prepared = MapPrepared<D::Prepared, F>;

    fn prepare(self, cacher: &mut Cacher) -> Self::Prepared {
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

impl DummyImpl<'_> for () {
    type Out = ();
    type Prepared = ();

    fn prepare(self, _cacher: &mut Cacher) -> Self::Prepared {}
}

impl<'columns, 'transaction, S> IntoDummy<'columns, 'transaction, S> for () {
    type Out = ();

    fn into_dummy(self) -> Dummy<'columns, 'transaction, S, Self::Out> {
        Dummy::new(())
    }
}

impl<'transaction, T: SecretFromSql<'transaction>> Prepared for Cached<T> {
    type Out = T;

    fn call(&mut self, row: Row<'_>) -> Self::Out {
        row.get(*self)
    }
}

pub struct ColumnImpl<T> {
    pub(crate) expr: DynTyped<T>,
}

impl<'transaction, T: MyTyp> DummyImpl<'transaction> for ColumnImpl<T> {
    type Out = T::Out<'transaction>;
    type Prepared = Cached<Self::Out>;

    fn prepare(self, cacher: &mut Cacher) -> Self::Prepared {
        Cached {
            idx: cacher.cache_erased(self.expr.erase()),
            _p: PhantomData,
        }
    }
}

impl<'columns, 'transaction, S, T> IntoDummy<'columns, 'transaction, S> for T
where
    T: IntoColumn<'columns, S>,
{
    type Out = <T::Typ as MyTyp>::Out<'transaction>;

    fn into_dummy(self) -> Dummy<'columns, 'transaction, S, Self::Out> {
        Dummy::new(ColumnImpl {
            expr: self.into_column().inner,
        })
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

impl<'transaction, A, B> DummyImpl<'transaction> for (A, B)
where
    A: DummyImpl<'transaction>,
    B: DummyImpl<'transaction>,
{
    type Out = (A::Out, B::Out);
    type Prepared = (A::Prepared, B::Prepared);

    fn prepare(self, cacher: &mut Cacher) -> Self::Prepared {
        let prepared_a = self.0.prepare(cacher);
        let prepared_b = self.1.prepare(cacher);
        (prepared_a, prepared_b)
    }
}

impl<'columns, 'transaction, S, A, B> IntoDummy<'columns, 'transaction, S> for (A, B)
where
    A: IntoDummy<'columns, 'transaction, S>,
    B: IntoDummy<'columns, 'transaction, S>,
{
    type Out = (A::Out, B::Out);

    fn into_dummy(self) -> Dummy<'columns, 'transaction, S, Self::Out> {
        Dummy::new((self.0.into_dummy().inner, self.1.into_dummy().inner))
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

    impl<'columns, 'transaction, S, A, B> IntoDummy<'columns, 'transaction, S> for UserDummy<A, B>
    where
        A: IntoColumn<'columns, S, Typ = i64>,
        B: IntoColumn<'columns, S, Typ = String>,
    {
        type Out = User;

        fn into_dummy(self) -> Dummy<'columns, 'transaction, S, Self::Out> {
            (self.a, self.b)
                .map_dummy((|(a, b)| User { a, b }) as fn((i64, String)) -> User)
                .into_dummy()
        }
    }
}
