use std::{fmt::Debug, marker::PhantomData, ops::Deref};

use ref_cast::RefCast;
use sea_query::{Alias, Expr, SimpleExpr};

use crate::{
    alias::{Field, MyAlias},
    value::{MyTyp, Typed, ValueBuilder},
    IntoColumn, LocalClient, Table,
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
    type Target = T::Ext<Self>;

    fn deref(&self) -> &Self::Target {
        RefCast::ref_cast(self)
    }
}

impl<T: MyTyp, P: Typed<Typ: Table>> Typed for Col<T, P> {
    type Typ = T;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        Expr::col((self.inner.build_table(b), self.field)).into()
    }
}
impl<'t, S, T: MyTyp, P: IntoColumn<'t, S, Typ: Table>> IntoColumn<'t, S> for Col<T, P> {
    type Owned = Col<T, P::Owned>;

    fn into_owned(self) -> Self::Owned {
        Col {
            _p: PhantomData,
            field: self.field,
            inner: self.inner.into_owned(),
        }
    }
}

/// Table reference that is the result of a join.
/// It can only be used in the query where it was created.
/// Invariant in `'t`.
pub(crate) struct Join<'t, T> {
    pub(crate) table: MyAlias,
    pub(crate) _p: PhantomData<fn(&'t T) -> &'t T>,
}

impl<'t, T> Join<'t, T> {
    pub(crate) fn new(table: MyAlias) -> Self {
        Self {
            table,
            _p: PhantomData,
        }
    }
}

impl<'t, T> Clone for Join<'t, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'t, T> Copy for Join<'t, T> {}

impl<'t, T: Table> Deref for Join<'t, T> {
    type Target = T::Ext<Self>;

    fn deref(&self) -> &Self::Target {
        RefCast::ref_cast(self)
    }
}

impl<T: Table> Typed for Join<'_, T> {
    type Typ = T;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        Expr::col((self.build_table(b), Alias::new(T::ID))).into()
    }
    fn build_table(&self, _: ValueBuilder) -> MyAlias {
        self.table
    }
}
impl<'t, T: Table> IntoColumn<'t, T::Schema> for Join<'t, T> {
    type Owned = Self;

    fn into_owned(self) -> Self::Owned {
        self
    }
}

/// Row reference that can be used in any query in the same transaction.
///
/// [TableRow] is covariant in `'t` and restricted to a single thread to prevent it from being used in a different transaction.
pub struct TableRow<'t, T> {
    pub(crate) _p: PhantomData<&'t T>,
    pub(crate) _local: PhantomData<LocalClient>,
    pub(crate) idx: i64,
}

impl<'t, T> PartialEq for TableRow<'t, T> {
    fn eq(&self, other: &Self) -> bool {
        self.idx == other.idx
    }
}

impl<T> Debug for TableRow<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "db_{}", self.idx)
    }
}

impl<T> Clone for TableRow<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for TableRow<'_, T> {}

impl<T: Table> Deref for TableRow<'_, T> {
    type Target = T::Ext<Self>;

    fn deref(&self) -> &Self::Target {
        RefCast::ref_cast(self)
    }
}

impl<'t, T> From<TableRow<'t, T>> for sea_query::Value {
    fn from(value: TableRow<T>) -> Self {
        value.idx.into()
    }
}

impl<T: Table> Typed for TableRow<'_, T> {
    type Typ = T;
    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        Expr::val(self.idx).into()
    }
}

impl<'t, T: Table> IntoColumn<'t, T::Schema> for TableRow<'t, T> {
    type Owned = TableRow<'t, T>;

    fn into_owned(self) -> Self::Owned {
        self
    }
}

#[cfg(test)]
#[allow(unused)]
mod tests {
    use super::*;
    struct Admin;

    impl Table for Admin {
        type Ext<T> = AdminDummy<T>;

        type Schema = ();

        fn typs(_: &mut crate::hash::TypBuilder) {}

        type Dummy<'t> = ();
        fn dummy<'t>(_: impl IntoColumn<'t, Self::Schema, Typ = Self>) -> Self::Dummy<'t> {}
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

    fn test(x: Join<Admin>) {
        let _res = &x.a().b().a().a();
    }
}
