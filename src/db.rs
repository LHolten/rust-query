use std::{fmt::Debug, marker::PhantomData, ops::Deref};

use ref_cast::RefCast;
use rusqlite::types::FromSql;
use sea_query::{Alias, Expr, SimpleExpr};

use crate::{
    alias::{Field, MyAlias},
    value::{MyTyp, Typed, ValueBuilder},
    HasId, ThreadToken, Value,
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

impl<T: MyTyp, X: Clone> Deref for Col<T, X> {
    type Target = T::Wrap<Self>;

    fn deref(&self) -> &Self::Target {
        RefCast::ref_cast(self)
    }
}

impl<T: MyTyp, P> Typed for Col<T, P> {
    type Typ = T;
}
impl<'t, S, T: MyTyp, P: Value<'t, S>> Value<'t, S> for Col<T, P>
where
    P::Typ: HasId,
{
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        Expr::col((self.inner.build_table(b), self.field)).into()
    }
}

/// Table reference that is the result of a join.
/// It can only be used in the query where it was created.
/// Invariant in `'t`.
pub struct Join<'t, T> {
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

impl<'t, T: MyTyp> Deref for Join<'t, T> {
    type Target = T::Wrap<Self>;

    fn deref(&self) -> &Self::Target {
        RefCast::ref_cast(self)
    }
}

impl<T: MyTyp> Typed for Join<'_, T> {
    type Typ = T;
}
impl<'t, T: HasId> Value<'t, T::Schema> for Join<'t, T> {
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        Expr::col((self.build_table(b), Alias::new(T::ID))).into()
    }
    fn build_table(&self, _: ValueBuilder) -> MyAlias {
        self.table
    }
}

/// Row reference that can be used in any query in the same transaction.
///
/// [Row] is covariant in `'t` and restricted to a single thread to prevent it from being used in a different transaction.
pub struct Row<'t, T> {
    pub(crate) _p: PhantomData<&'t T>,
    pub(crate) _local: PhantomData<ThreadToken>,
    pub(crate) idx: i64,
}

impl<'t, T> PartialEq for Row<'t, T> {
    fn eq(&self, other: &Self) -> bool {
        self.idx == other.idx
    }
}

impl<T> Debug for Row<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "db_{}", self.idx)
    }
}

impl<T> Clone for Row<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Row<'_, T> {}

impl<T: MyTyp> Deref for Row<'_, T> {
    type Target = T::Wrap<Self>;

    fn deref(&self) -> &Self::Target {
        RefCast::ref_cast(self)
    }
}

impl<T> FromSql for Row<'_, T> {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        Ok(Self {
            _p: PhantomData,
            _local: PhantomData,
            idx: value.as_i64()?,
        })
    }
}

impl<'t, T> From<Row<'t, T>> for sea_query::Value {
    fn from(value: Row<T>) -> Self {
        value.idx.into()
    }
}

impl<T: MyTyp> Typed for Row<'_, T> {
    type Typ = T;
}
impl<'t, T: HasId> Value<'t, T::Schema> for Row<'_, T> {
    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        Expr::val(self.idx).into()
    }
}

#[cfg(test)]
#[allow(unused)]
mod tests {
    use crate::Table;

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

    impl HasId for Admin {
        const ID: &'static str = "";
        const NAME: &'static str = "";
    }

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
