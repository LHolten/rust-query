use std::marker::PhantomData;

use sea_query::Iden;

use crate::{alias::Field, ast::MySelect, value::MyTyp, Value};

pub struct Cacher<'t> {
    pub(crate) _p: PhantomData<fn(&'t ()) -> &'t ()>,
    pub(crate) ast: &'t MySelect,
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

impl<'t> Cacher<'t> {
    pub fn cache<T>(&mut self, val: impl Value<'t, Typ = T>) -> Cached<'t, T> {
        let expr = val.build_expr(self.ast.builder());
        let field = *self.ast.select.get_or_init(expr, Field::new);
        Cached {
            _p: PhantomData,
            field,
        }
    }
}

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

pub trait FromRow<'t, 'a> {
    type Out;
    fn prepare(self, cacher: Cacher<'t>) -> impl FnMut(Row<'_, 't, 'a>) -> Self::Out;
}

impl<'t, 'a, T: Value<'t, Typ: MyTyp>> FromRow<'t, 'a> for T {
    type Out = <T::Typ as MyTyp>::Out<'a>;

    fn prepare(self, mut cacher: Cacher<'t>) -> impl FnMut(Row<'_, 't, 'a>) -> Self::Out {
        let cached = cacher.cache(self);
        move |row| row.get(cached)
    }
}

impl<'t, 'a, A: Value<'t, Typ: MyTyp>, B: Value<'t, Typ: MyTyp>> FromRow<'t, 'a> for (A, B) {
    type Out = (<A::Typ as MyTyp>::Out<'a>, <B::Typ as MyTyp>::Out<'a>);

    fn prepare(self, mut cacher: Cacher<'t>) -> impl FnMut(Row<'_, 't, 'a>) -> Self::Out {
        let cached_a = cacher.cache(self.0);
        let cached_b = cacher.cache(self.1);
        move |row| (row.get(cached_a), row.get(cached_b))
    }
}

pub(crate) struct AdHoc<F> {
    f: F,
}

impl<'t, 'a, T, F, G> FromRow<'t, 'a> for AdHoc<F>
where
    G: FnMut(Row<'_, 't, 'a>) -> T,
    F: FnOnce(Cacher<'t>) -> G,
{
    type Out = T;

    fn prepare(self, cacher: Cacher<'t>) -> impl FnMut(Row<'_, 't, 'a>) -> Self::Out {
        (self.f)(cacher)
    }
}

impl<F> AdHoc<F> {
    pub fn new<'t, 'a, T, G>(f: F) -> Self
    where
        G: FnMut(Row<'_, 't, 'a>) -> T,
        F: FnOnce(Cacher<'t>) -> G,
    {
        Self { f }
    }
}

#[cfg(test)]
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

    impl<'t, 'a, A, B> FromRow<'t, 'a> for UserDummy<A, B>
    where
        A: Value<'t, Typ = i64>,
        B: Value<'t, Typ = String>,
    {
        type Out = User;

        fn prepare(self, mut cacher: Cacher<'t>) -> impl FnMut(Row<'_, 't, 'a>) -> Self::Out {
            let a = cacher.cache(self.a);
            let b = cacher.cache(self.b);
            move |row| User {
                a: row.get(a),
                b: row.get(b),
            }
        }
    }
}
