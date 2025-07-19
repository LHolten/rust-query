use std::marker::PhantomData;

use sea_query::Iden;

use crate::{
    Expr,
    alias::MyAlias,
    value::{DynTyped, DynTypedExpr, MyTyp, SecretFromSql},
};

/// Opaque type used to implement [crate::Select].
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
    pub(crate) fields: &'x [MyAlias],
}

impl<'x> Row<'x> {
    pub(crate) fn new(row: &'x rusqlite::Row<'x>, fields: &'x [MyAlias]) -> Self {
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

/// [Select] is used to define what to query from the database for each row.
///
/// It defines a set of expressions to evaluate in the database, and then how to turn the results into rust values.
///
/// For this reason many [rust_query] APIs accept values that implement [IntoSelect].
pub struct Select<'columns, S, Out> {
    pub(crate) inner: DynSelectImpl<Out>,
    pub(crate) _p: PhantomData<&'columns ()>,
    pub(crate) _p2: PhantomData<S>,
}

impl<'columns, 'transaction, S, Out: 'static> Select<'columns, S, Out> {
    /// Map the result of a [Select] using native rust.
    ///
    /// This is useful when implementing [IntoSelect].
    pub fn map<T>(self, f: impl 'static + FnMut(Out) -> T) -> Select<'columns, S, T> {
        Select::new(MapImpl {
            dummy: self.inner,
            func: f,
        })
    }
}

pub struct DynSelectImpl<Out> {
    inner: Box<dyn FnOnce(&mut Cacher) -> DynPrepared<Out>>,
}

impl<Out> SelectImpl for DynSelectImpl<Out> {
    type Out = Out;
    type Prepared = DynPrepared<Out>;

    fn prepare(self, cacher: &mut Cacher) -> Self::Prepared {
        (self.inner)(cacher)
    }
}

pub struct DynPrepared<Out> {
    inner: Box<dyn Prepared<Out = Out>>,
}

impl<Out> Prepared for DynPrepared<Out> {
    type Out = Out;
    fn call(&mut self, row: Row<'_>) -> Self::Out {
        self.inner.call(row)
    }
}

impl<'transaction, S, Out> Select<'_, S, Out> {
    pub(crate) fn new(val: impl 'static + SelectImpl<Out = Out>) -> Self {
        Self {
            inner: DynSelectImpl {
                inner: Box::new(|cacher| DynPrepared {
                    inner: Box::new(val.prepare(cacher)),
                }),
            },
            _p: PhantomData,
            _p2: PhantomData,
        }
    }
}

impl<'columns, S, Out: 'static> IntoSelect<'columns, S> for Select<'columns, S, Out> {
    type Out = Out;

    fn into_select(self) -> Select<'columns, S, Self::Out> {
        self
    }
}

pub trait SelectImpl {
    type Out;
    #[doc(hidden)]
    type Prepared: Prepared<Out = Self::Out>;
    #[doc(hidden)]
    fn prepare(self, cacher: &mut Cacher) -> Self::Prepared;
}

/// This trait is implemented by everything that can be retrieved from the database.
///
/// The most common type that implements [IntoSelect] is [Expr].
/// Tuples of two values also implement [IntoSelect]. If you want to return more
/// than two values, then you should use a struct that derives [derive@rust_query::Select].
pub trait IntoSelect<'columns, S>: Sized {
    /// The type that results from executing the [Select].
    type Out: 'static;

    /// This method is what tells rust-query how to turn the value into a [Select].
    ///
    /// The only way to implement this method is by constructing a different value
    /// that implements [IntoSelect] and then calling the [IntoSelect::into_select] method
    /// on that other value. The result can then be modified with [Select::map].
    fn into_select(self) -> Select<'columns, S, Self::Out>;
}

/// This is the result of the [Select::map_select] method.
///
/// [MapImpl] retrieves the same columns as the [Select] that it wraps,
/// but then it processes those columns using a rust closure.
pub struct MapImpl<D, F> {
    dummy: D,
    func: F,
}

impl<D, F, O> SelectImpl for MapImpl<D, F>
where
    D: SelectImpl,
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

impl SelectImpl for () {
    type Out = ();
    type Prepared = ();

    fn prepare(self, _cacher: &mut Cacher) -> Self::Prepared {}
}

impl<'columns, S> IntoSelect<'columns, S> for () {
    type Out = ();

    fn into_select(self) -> Select<'columns, S, Self::Out> {
        Select::new(())
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

impl<T: MyTyp> SelectImpl for ColumnImpl<T> {
    type Out = T::Out;
    type Prepared = Cached<Self::Out>;

    fn prepare(self, cacher: &mut Cacher) -> Self::Prepared {
        Cached {
            idx: cacher.cache_erased(self.expr.erase()),
            _p: PhantomData,
        }
    }
}

impl<'columns, S, T> IntoSelect<'columns, S> for Expr<'columns, S, T>
where
    T: MyTyp,
{
    type Out = T::Out;

    fn into_select(self) -> Select<'columns, S, Self::Out> {
        Select::new(ColumnImpl { expr: self.inner })
    }
}

impl<'columns, S, T> IntoSelect<'columns, S> for &T
where
    T: IntoSelect<'columns, S> + Clone,
{
    type Out = T::Out;

    fn into_select(self) -> Select<'columns, S, Self::Out> {
        T::clone(self).into_select()
    }
}

impl<A, B> Prepared for (A, B)
where
    A: Prepared,
    B: Prepared,
{
    type Out = (A::Out, B::Out);

    fn call(&mut self, row: Row<'_>) -> Self::Out {
        (self.0.call(row), self.1.call(row))
    }
}

impl<A, B> SelectImpl for (A, B)
where
    A: SelectImpl,
    B: SelectImpl,
{
    type Out = (A::Out, B::Out);
    type Prepared = (A::Prepared, B::Prepared);

    fn prepare(self, cacher: &mut Cacher) -> Self::Prepared {
        let prepared_a = self.0.prepare(cacher);
        let prepared_b = self.1.prepare(cacher);
        (prepared_a, prepared_b)
    }
}

impl<'columns, S, A, B> IntoSelect<'columns, S> for (A, B)
where
    A: IntoSelect<'columns, S>,
    B: IntoSelect<'columns, S>,
{
    type Out = (A::Out, B::Out);

    fn into_select(self) -> Select<'columns, S, Self::Out> {
        Select::new((self.0.into_select().inner, self.1.into_select().inner))
    }
}

#[cfg(test)]
#[allow(unused)]
mod tests {
    use crate::IntoExpr;

    use super::*;

    struct User {
        a: i64,
        b: String,
    }

    struct UserSelect<A, B> {
        a: A,
        b: B,
    }

    impl<'columns, S, A, B> IntoSelect<'columns, S> for UserSelect<A, B>
    where
        A: IntoExpr<'columns, S, Typ = i64>,
        B: IntoExpr<'columns, S, Typ = String>,
    {
        type Out = User;

        fn into_select(self) -> Select<'columns, S, Self::Out> {
            (self.a.into_expr(), self.b.into_expr())
                .into_select()
                .map((|(a, b)| User { a, b }) as fn((i64, String)) -> User)
                .into_select()
        }
    }
}
