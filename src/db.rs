use std::{fmt::Debug, marker::PhantomData, ops::Deref};

use ref_cast::RefCast;
use sea_query::{Alias, SimpleExpr};

use crate::{
    Expr, IntoColumn, LocalClient, Table,
    alias::{Field, MyAlias},
    value::{MyTyp, Private, Typed, ValueBuilder},
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

impl<T: MyTyp, P: Typed<Typ: Table>> Typed for Col<T, P> {
    type Typ = T;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        sea_query::Expr::col((self.inner.build_table(b), self.field)).into()
    }
}

/// Table reference that is the result of a join.
/// It can only be used in the query where it was created.
/// Invariant in `'t`.
pub(crate) struct Join<T> {
    pub(crate) table: MyAlias,
    pub(crate) _p: PhantomData<T>,
}

impl<T> Join<T> {
    pub(crate) fn new(table: MyAlias) -> Self {
        Self {
            table,
            _p: PhantomData,
        }
    }
}

impl<T> Clone for Join<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Join<T> {}

impl<T: Table> Typed for Join<T> {
    type Typ = T;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        sea_query::Expr::col((self.build_table(b), Alias::new(T::ID))).into()
    }
    fn build_table(&self, _: ValueBuilder) -> MyAlias {
        self.table
    }
}

/// Row reference that can be used in any query in the same transaction.
///
/// [TableRow] is covariant in `'t` and restricted to a single thread to prevent it from being used in a different transaction.
///
/// Note that the [TableRow] can typically only be used at the top level of each query (not inside aggregates).
/// `rustc` sometimes suggested making the transaction lifetime `'static` to get around this issue.
/// While it is a valid and correct suggestion, you probably don't want a `'static` transaction.
///
/// The appropriate solution is to use [crate::args::Aggregate::filter_on] to bring [TableRow]
/// columns into the [crate::aggregate] inner scope.
pub struct TableRow<'t, T> {
    pub(crate) _p: PhantomData<&'t ()>,
    pub(crate) _local: PhantomData<LocalClient>,
    pub(crate) inner: TableRowInner<T>,
}

pub struct TableRowInner<T> {
    pub(crate) _p: PhantomData<T>,
    pub(crate) idx: i64,
}

impl<'t, T> PartialEq for TableRow<'t, T> {
    fn eq(&self, other: &Self) -> bool {
        self.inner.idx == other.inner.idx
    }
}

impl<T> Debug for TableRow<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "db_{}", self.inner.idx)
    }
}

impl<T> Clone for TableRow<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for TableRow<'_, T> {}

impl<T> Clone for TableRowInner<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for TableRowInner<T> {}

impl<T: Table> Deref for TableRow<'_, T> {
    type Target = T::Ext<Self>;

    fn deref(&self) -> &Self::Target {
        RefCast::ref_cast(self)
    }
}

impl<'t, T> From<TableRow<'t, T>> for sea_query::Value {
    fn from(value: TableRow<T>) -> Self {
        value.inner.idx.into()
    }
}

impl<T: Table> Typed for TableRowInner<T> {
    type Typ = T;
    fn build_expr(&self, _: ValueBuilder) -> SimpleExpr {
        sea_query::Expr::val(self.idx).into()
    }
}

impl<'t, T> Private for TableRow<'t, T> {}
impl<'t, T: Table> IntoColumn<'t, T::Schema> for TableRow<'t, T> {
    type Typ = T;
    fn into_column(self) -> Expr<'t, T::Schema, Self::Typ> {
        Expr::new(self.inner)
    }
}

/// This makes it possible to use TableRow as a parameter in
/// rusqlite queries and statements.
impl<T> rusqlite::ToSql for TableRow<'_, T> {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        self.inner.idx.to_sql()
    }
}

#[cfg(test)]
#[allow(unused)]
mod tests {
    use std::convert::Infallible;

    use crate::schema_pragma::FakeInsert;

    use super::*;
    struct Admin;

    impl Table for Admin {
        type Ext<T> = AdminDummy<T>;

        type Schema = ();
        type Referer = ();
        fn get_referer_unchecked() -> Self::Referer {}

        fn typs(_: &mut crate::hash::TypBuilder<Self::Schema>) {}

        type Conflict<'t> = Infallible;
        type Update<'t> = ();
        type TryUpdate<'t> = ();

        fn update_into_try_update<'t>(val: Self::Update<'t>) -> Self::TryUpdate<'t> {
            todo!()
        }

        fn apply_try_update<'t>(
            val: Self::TryUpdate<'t>,
            old: Expr<'t, Self::Schema, Self>,
        ) -> impl crate::private::TableInsert<
            't,
            T = Self,
            Schema = Self::Schema,
            Conflict = Self::Conflict<'t>,
        > {
            FakeInsert(PhantomData)
        }

        const ID: &'static str = "";
        const NAME: &'static str = "";
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
}
