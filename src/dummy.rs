use std::marker::PhantomData;

use ref_cast::{ref_cast_custom, RefCastCustom};
use sea_query::Iden;

use crate::{
    alias::Field,
    ast::MySelect,
    value::{MyTyp, Typed},
    IntoColumn,
};

#[derive(RefCastCustom)]
#[repr(transparent)]
pub struct Cacher<'t, 'i, S> {
    pub(crate) _p: PhantomData<fn(&'t ()) -> &'i ()>,
    pub(crate) _p2: PhantomData<S>,
    pub(crate) ast: MySelect,
}

impl<S> Cacher<'_, '_, S> {
    #[ref_cast_custom]
    pub(crate) fn new<'x>(val: &'x MySelect) -> &'x Self;
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

impl<'t, 'i, S> Cacher<'t, 'i, S> {
    pub fn cache<T: 'static>(&self, val: impl IntoColumn<'t, S, Typ = T>) -> Cached<'i, T> {
        let val = val.into_column().inner;
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
pub struct Row<'x, 'i, 'a> {
    pub(crate) _p: PhantomData<fn(&'i ()) -> &'a ()>,
    pub(crate) _p2: PhantomData<&'i &'a ()>,
    pub(crate) row: &'x rusqlite::Row<'x>,
}

impl<'t, 'a> Row<'_, 't, 'a> {
    pub fn get<T: MyTyp>(&self, val: Cached<'t, T>) -> T::Out<'a> {
        let idx = &*val.field.to_string();
        T::from_sql(self.row.get_ref_unwrap(idx)).unwrap()
    }
}

/// This trait is implemented by everything that can be retrieved from the database.
///
/// Implement it on custom structs using [crate::FromDummy].
pub trait Dummy<'t, 'a, S>: Sized {
    /// The type that results from querying this dummy.
    type Out: 'a;

    #[doc(hidden)]
    fn prepare<'i>(
        self,
        cacher: &Cacher<'t, 'i, S>,
    ) -> Box<dyn 'i + FnMut(Row<'_, 'i, 'a>) -> Self::Out>;

    /// Map a dummy to another dummy using native rust.
    ///
    /// This is useful when retrieving a struct from the database that contains types not supported by the database.
    /// It is also useful in migrations to process rows using arbitrary rust.
    fn map_dummy<T: 'a>(
        self,
        f: impl 'a + FnMut(Self::Out) -> T,
    ) -> impl Dummy<'t, 'a, S, Out = T> {
        DummyMap(self, f)
    }
}

struct DummyMap<A, F>(A, F);

impl<'t, 'a, S, A, F, T> Dummy<'t, 'a, S> for DummyMap<A, F>
where
    A: Dummy<'t, 'a, S>,
    F: 'a + FnMut(A::Out) -> T,
    T: 'a,
{
    type Out = T;

    fn prepare<'i>(
        mut self,
        cacher: &Cacher<'t, 'i, S>,
    ) -> Box<dyn 'i + FnMut(Row<'_, 'i, 'a>) -> Self::Out> {
        let mut cached = self.0.prepare(cacher);
        Box::new(move |row| self.1(cached(row)))
    }
}

impl<'t, 'a, S, T: IntoColumn<'t, S, Typ: MyTyp>> Dummy<'t, 'a, S> for T {
    type Out = <T::Typ as MyTyp>::Out<'a>;

    fn prepare<'i>(
        self,
        cacher: &Cacher<'t, 'i, S>,
    ) -> Box<dyn 'i + FnMut(Row<'_, 'i, 'a>) -> Self::Out> {
        let cached = cacher.cache(self);
        Box::new(move |row| row.get(cached))
    }
}

impl<'t, 'a, S, A: Dummy<'t, 'a, S>, B: Dummy<'t, 'a, S>> Dummy<'t, 'a, S> for (A, B) {
    type Out = (A::Out, B::Out);

    fn prepare<'i>(
        self,
        cacher: &Cacher<'t, 'i, S>,
    ) -> Box<dyn 'i + FnMut(Row<'_, 'i, 'a>) -> Self::Out> {
        let mut prepared_a = self.0.prepare(cacher);
        let mut prepared_b = self.1.prepare(cacher);
        Box::new(move |row| (prepared_a(row), prepared_b(row)))
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

        fn prepare<'i>(
            self,
            cacher: &Cacher<'t, 'i, S>,
        ) -> Box<dyn 'i + FnMut(Row<'_, 'i, 'a>) -> Self::Out> {
            let a = cacher.cache(self.a);
            let b = cacher.cache(self.b);
            Box::new(move |row| User {
                a: row.get(a),
                b: row.get(b),
            })
        }
    }
}
