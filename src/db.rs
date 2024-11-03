use std::{fmt::Debug, marker::PhantomData, ops::Deref};

use ref_cast::RefCast;
use rusqlite::types::FromSql;
use sea_query::{Alias, Expr, SimpleExpr};

use crate::{
    alias::{Field, MyAlias},
    value::{operations::Assume, MyTyp, Typed, ValueBuilder},
    IntoColumn, Table, ThreadToken,
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

pub struct TableRowId<T> {
    pub(crate) val: i64,
    _p: PhantomData<T>,
}

impl<T> Debug for TableRowId<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "db_{}", self.val)
    }
}

impl<T> PartialEq for TableRowId<T> {
    fn eq(&self, other: &Self) -> bool {
        self.val == other.val
    }
}

impl<T> Clone for TableRowId<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for TableRowId<T> {}

// TODO: consider implementing build_table for this and uniques
impl<T: Table> Typed for TableRowId<T> {
    type Typ = Option<T>;

    fn build_expr(&self, b: crate::value::ValueBuilder) -> sea_query::SimpleExpr {
        let val = Expr::val(self.val).into();
        b.get_unique(T::NAME, vec![(T::ID, val)])
    }
}

impl<'t, T: Table> IntoColumn<'t, T::Schema> for TableRowId<T> {
    type Owned = TableRowId<T>;

    fn into_owned(self) -> Self::Owned {
        self
    }
}

impl<T> FromSql for TableRowId<T> {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        Ok(Self {
            _p: PhantomData,
            val: value.as_i64()?,
        })
    }
}

/// Row reference that can be used in any query in the same transaction.
///
/// [TableRow] is covariant in `'t` and restricted to a single thread to prevent it from being used in a different transaction.
pub struct TableRow<'t, T> {
    pub(crate) _local: PhantomData<&'t ThreadToken>,
    pub id: TableRowId<T>,
}

impl<'t, T> PartialEq for TableRow<'t, T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T> Debug for TableRow<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.id.fmt(f)
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

impl<T> FromSql for TableRow<'_, T> {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        Ok(Self {
            _local: PhantomData,
            id: FromSql::column_result(value)?,
        })
    }
}

impl<T: Table> Typed for TableRow<'_, T> {
    type Typ = T;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.id.build_expr(b)
    }
}

impl<'t, T: Table> IntoColumn<'t, T::Schema> for TableRow<'_, T> {
    type Owned = Assume<TableRowId<T>>;

    fn into_owned(self) -> Self::Owned {
        Assume(self.id)
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
