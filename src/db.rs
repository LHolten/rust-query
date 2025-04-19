use std::{fmt::Debug, marker::PhantomData, ops::Deref};

use ref_cast::RefCast;
use sea_query::{Alias, SimpleExpr};

use crate::{
    Expr, IntoExpr, LocalClient, Table,
    alias::{Field, MyAlias},
    value::{MyTyp, Typed, ValueBuilder},
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
impl<'t, T> TableRow<'t, T> {
    pub(crate) fn new(idx: i64) -> Self {
        Self {
            _p: PhantomData,
            _local: PhantomData,
            inner: TableRowInner {
                _p: PhantomData,
                idx,
            },
        }
    }
}

impl<'t, T> Eq for TableRow<'t, T> {}

impl<'t, T> PartialOrd for TableRow<'t, T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.inner.idx.partial_cmp(&other.inner.idx)
    }
}

impl<'t, T> Ord for TableRow<'t, T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.inner.idx.cmp(&other.inner.idx)
    }
}

pub(crate) struct TableRowInner<T> {
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

impl<'t, S, T: Table> IntoExpr<'t, S> for TableRow<'t, T> {
    type Typ = T;
    fn into_expr(self) -> Expr<'t, S, Self::Typ> {
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

    use crate::{IntoSelectExt, Select, private::Reader};

    use super::*;
    struct Admin;

    impl Table for Admin {
        type MigrateFrom = Self;
        type Ext<T> = AdminSelect<T>;

        type Schema = ();
        type Referer = ();
        fn get_referer_unchecked() -> Self::Referer {}

        fn typs(_: &mut crate::hash::TypBuilder<Self::Schema>) {}

        type Conflict<'t> = Infallible;
        type UpdateOk<'t> = ();
        type Update<'t> = ();
        type Insert<'t> = ();

        fn read<'t>(val: &Self::Insert<'t>, f: &Reader<'t, Self::Schema>) {
            todo!()
        }

        fn get_conflict_unchecked<'t>(
            txn: &crate::Transaction<'t, Self::Schema>,
            val: &Self::Insert<'t>,
        ) -> Self::Conflict<'t> {
            todo!()
        }

        fn update_into_try_update<'t>(val: Self::UpdateOk<'t>) -> Self::Update<'t> {
            todo!()
        }

        fn apply_try_update<'t>(
            val: Self::Update<'t>,
            old: Expr<'t, Self::Schema, Self>,
        ) -> Self::Insert<'t> {
            todo!()
        }

        const ID: &'static str = "";
        const NAME: &'static str = "";
    }

    #[repr(transparent)]
    #[derive(RefCast)]
    struct AdminSelect<X>(X);

    impl<X: Clone> AdminSelect<X> {
        fn a(&self) -> Col<Admin, X> {
            Col::new("a", self.0.clone())
        }
        fn b(&self) -> Col<Admin, X> {
            Col::new("b", self.0.clone())
        }
    }
}
