use std::{fmt::Debug, marker::PhantomData, ops::Deref};

use ref_cast::RefCast;

use crate::{
    alias::{Field, MyAlias},
    Table,
};

pub struct Col<T, X> {
    pub(crate) _p: PhantomData<T>,
    pub(crate) field: Field,
    pub(crate) inner: X,
}

impl<T, X: Clone> Clone for Col<T, X> {
    fn clone(&self) -> Self {
        Self {
            _p: self._p,
            field: self.field,
            inner: self.inner.clone(),
        }
    }
}

impl<T, X: Copy> Copy for Col<T, X> {}

impl<T, X> Col<T, X> {
    pub fn new(key: &'static str, x: X) -> Self {
        Self {
            _p: PhantomData,
            field: Field::Str(key),
            inner: x,
        }
    }
}

impl<T: Table, X: Clone> Deref for Col<T, X> {
    type Target = T::Dummy<Self>;

    fn deref(&self) -> &Self::Target {
        RefCast::ref_cast(self)
    }
}

pub type DbCol<'t, T> = Col<T, Db<'t, T>>;

impl<'t, T> DbCol<'t, T> {
    pub(crate) fn db(table: MyAlias, field: Field) -> Self {
        Col {
            _p: PhantomData,
            field,
            inner: Db {
                table,
                _p: PhantomData,
            },
        }
    }
}

pub struct Db<'t, T> {
    pub(crate) table: MyAlias,
    pub(crate) _p: PhantomData<dyn Fn(&'t T) -> &'t T>,
}

impl<'t, T> Clone for Db<'t, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'t, T> Copy for Db<'t, T> {}

impl<'t, T: Table> Deref for Db<'t, T> {
    type Target = T::Dummy<Self>;

    fn deref(&self) -> &Self::Target {
        RefCast::ref_cast(self)
    }
}

pub struct Just<T> {
    pub(crate) _p: PhantomData<T>,
    pub(crate) idx: i64,
}

impl<T> Debug for Just<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Just").field("idx", &self.idx).finish()
    }
}

impl<T> Clone for Just<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Just<T> {}

impl<T: Table> Deref for Just<T> {
    type Target = T::Dummy<Self>;

    fn deref(&self) -> &Self::Target {
        RefCast::ref_cast(self)
    }
}

#[cfg(test)]
mod tests {
    use ref_cast::RefCast;

    use super::*;
    struct Admin;

    impl Table for Admin {
        type Dummy<T> = AdminDummy<T>;

        type Schema = ();

        fn name(&self) -> String {
            todo!()
        }

        fn typs(_: &mut crate::TypBuilder) {}
    }

    #[repr(transparent)]
    #[derive(RefCast)]
    struct AdminDummy<X>(X);

    impl<X: Clone> AdminDummy<X> {
        fn a(&self) -> Col<Admin, X> {
            Col::new("a", self.0.clone())
        }
        fn b(&self) -> Col<Admin, X> {
            Col::new("b", self.0.clone())
        }
    }

    fn test(x: Db<Admin>) {
        let _res = &x.a().b().a().a();
    }
}
