use std::marker::PhantomData;

use sea_query::Iden;

use crate::{alias::Field, ast::MySelect, value::MyTyp, Value};

pub struct Cacher<'t, S> {
    pub(crate) _p: PhantomData<fn(&'t S) -> &'t S>,
    pub(crate) ast: &'t MySelect,
}

impl<S> Copy for Cacher<'_, S> {}

impl<S> Clone for Cacher<'_, S> {
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

impl<'t, S> Cacher<'t, S> {
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

pub trait FromRow<'t, 'a, S> {
    type Out;
    fn prepare(self, cacher: Cacher<'t, S>) -> impl FnMut(Row<'_, 't, 'a>) -> Self::Out;
}

impl<'t, 'a, S, T: Value<'t, S, Typ: MyTyp>> FromRow<'t, 'a, S> for (T,) {
    type Out = <T::Typ as MyTyp>::Out<'a>;

    fn prepare(self, mut cacher: Cacher<'t, S>) -> impl FnMut(Row<'_, 't, 'a>) -> Self::Out {
        let cached = cacher.cache(self.0);
        move |row| row.get(cached)
    }
}

impl<'t, 'a, S, A: Value<'t, S, Typ: MyTyp>, B: Value<'t, S, Typ: MyTyp>> FromRow<'t, 'a, S>
    for (A, B)
{
    type Out = (<A::Typ as MyTyp>::Out<'a>, <B::Typ as MyTyp>::Out<'a>);

    fn prepare(self, mut cacher: Cacher<'t, S>) -> impl FnMut(Row<'_, 't, 'a>) -> Self::Out {
        let cached_a = cacher.cache(self.0);
        let cached_b = cacher.cache(self.1);
        move |row| (row.get(cached_a), row.get(cached_b))
    }
}

pub(crate) struct AdHoc<F> {
    f: F,
}

impl<'t, 'a, S: 't, T, F, G> FromRow<'t, 'a, S> for AdHoc<F>
where
    G: FnMut(Row<'_, 't, 'a>) -> T,
    F: FnOnce(Cacher<'t, S>) -> G,
{
    type Out = T;

    fn prepare(self, cacher: Cacher<'t, S>) -> impl FnMut(Row<'_, 't, 'a>) -> Self::Out {
        (self.f)(cacher)
    }
}

impl<F> AdHoc<F> {
    pub fn new<'t, 'a, S: 't, T, G>(f: F) -> Self
    where
        G: FnMut(Row<'_, 't, 'a>) -> T,
        F: FnOnce(Cacher<'t, S>) -> G,
    {
        Self { f }
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

    impl<'t, 'a, S, A, B> FromRow<'t, 'a, S> for UserDummy<A, B>
    where
        A: Value<'t, S, Typ = i64>,
        B: Value<'t, S, Typ = String>,
    {
        type Out = User;

        fn prepare(self, mut cacher: Cacher<'t, S>) -> impl FnMut(Row<'_, 't, 'a>) -> Self::Out {
            let a = cacher.cache(self.a);
            let b = cacher.cache(self.b);
            move |row| User {
                a: row.get(a),
                b: row.get(b),
            }
        }
    }
}
