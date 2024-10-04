use std::marker::PhantomData;

use sea_query::Iden;

use crate::{alias::Field, ast::MySelect, value::MyTyp, Value};

pub struct Cacher<'x, 't, S> {
    pub(crate) _p: PhantomData<fn(&'t S) -> &'t S>,
    pub(crate) ast: &'x MySelect,
}

impl<S> Copy for Cacher<'_, '_, S> {}

impl<S> Clone for Cacher<'_, '_, S> {
    fn clone(&self) -> Self {
        *self
    }
}

pub struct Cached<'t, T> {
    _p: PhantomData<fn(&'t T) -> &'t T>,
    field: Field,
}

impl<'t, T> Clone for Cached<'t, T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<'t, T> Copy for Cached<'t, T> {}

impl<'t, S> Cacher<'_, 't, S> {
    pub fn cache<T>(&mut self, val: impl Value<'t, S, Typ = T>) -> Cached<'t, T> {
        let expr = val.build_expr(self.ast.builder());
        let new_field = || self.ast.scope.new_field();
        let field = *self.ast.select.get_or_init(expr, new_field);
        Cached {
            _p: PhantomData,
            field,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Row<'x, 't, 'a> {
    pub(crate) _p: PhantomData<fn(&'t ()) -> &'t ()>,
    pub(crate) _p2: PhantomData<fn(&'a ()) -> &'a ()>,
    pub(crate) row: &'x rusqlite::Row<'x>,
}

impl<'t, 'a> Row<'_, 't, 'a> {
    pub fn get<T: MyTyp>(&self, val: Cached<'t, T>) -> T::Out<'a> {
        let idx = &*val.field.to_string();
        self.row.get_unwrap(idx)
    }
}

/// This trait is implemented by everything that can be retrieved from the database.
/// Implement it using the derive proc macro on a struct.
pub trait Dummy<'t, 'a, S>: Sized {
    type Out;
    fn prepare(self, cacher: Cacher<'_, 't, S>) -> impl FnMut(Row<'_, 't, 'a>) -> Self::Out;

    fn map<T>(self, f: impl FnMut(Self::Out) -> T) -> impl Dummy<'t, 'a, S, Out = T> {
        DummyMap(self, f)
    }
}

struct DummyMap<A, F>(A, F);

impl<'t, 'a, S, A, F, T> Dummy<'t, 'a, S> for DummyMap<A, F>
where
    A: Dummy<'t, 'a, S>,
    F: FnMut(A::Out) -> T,
{
    type Out = T;

    fn prepare(mut self, cacher: Cacher<'_, 't, S>) -> impl FnMut(Row<'_, 't, 'a>) -> Self::Out {
        let mut cached = self.0.prepare(cacher);
        move |row| self.1(cached(row))
    }
}

impl<'t, 'a, S, T: Value<'t, S, Typ: MyTyp>> Dummy<'t, 'a, S> for T {
    type Out = <T::Typ as MyTyp>::Out<'a>;

    fn prepare(self, mut cacher: Cacher<'_, 't, S>) -> impl FnMut(Row<'_, 't, 'a>) -> Self::Out {
        let cached = cacher.cache(self);
        move |row| row.get(cached)
    }
}

impl<'t, 'a, S, A: Dummy<'t, 'a, S>, B: Dummy<'t, 'a, S>> Dummy<'t, 'a, S> for (A, B) {
    type Out = (A::Out, B::Out);

    fn prepare(self, cacher: Cacher<'_, 't, S>) -> impl FnMut(Row<'_, 't, 'a>) -> Self::Out {
        let mut prepared_a = self.0.prepare(cacher);
        let mut prepared_b = self.1.prepare(cacher);
        move |row| (prepared_a(row), prepared_b(row))
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
        A: Value<'t, S, Typ = i64>,
        B: Value<'t, S, Typ = String>,
    {
        type Out = User;

        fn prepare(
            self,
            mut cacher: Cacher<'_, 't, S>,
        ) -> impl FnMut(Row<'_, 't, 'a>) -> Self::Out {
            let a = cacher.cache(self.a);
            let b = cacher.cache(self.b);
            move |row| User {
                a: row.get(a),
                b: row.get(b),
            }
        }
    }
}
