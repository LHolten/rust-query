use std::{
    cell::OnceCell,
    collections::{BTreeMap, BTreeSet},
    marker::PhantomData,
};

use sea_query::{ExprTrait, QueryBuilder};

use crate::{
    IntoExpr, TableRow, Transaction,
    ast::{CONST_0, CONST_1},
    schema::{canonical, from_db},
    value::{EqTyp, MyTyp},
};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Column {
    pub def: canonical::Column,
    pub span: (usize, usize),
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Index {
    pub def: from_db::Index,
    pub span: (usize, usize),
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Table {
    pub columns: BTreeMap<String, Column>,
    pub indices: BTreeSet<Index>,
    pub span: (usize, usize),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Schema {
    pub tables: BTreeMap<&'static str, Table>,
    pub span: (usize, usize),
}

pub struct TypBuilder<S> {
    pub(crate) ast: Table,
    _p: PhantomData<S>,
}

impl<S> Default for TypBuilder<S> {
    fn default() -> Self {
        Self {
            ast: Default::default(),
            _p: Default::default(),
        }
    }
}

impl<S> TypBuilder<S> {
    pub fn col<T: SchemaType<S, ExprTyp: MyTyp>>(
        &mut self,
        name: &'static str,
        span: (usize, usize),
    ) {
        let item = Column {
            def: canonical::Column {
                typ: T::ExprTyp::TYP,
                nullable: T::ExprTyp::NULLABLE,
                fk: T::ExprTyp::FK.map(|(table, fk)| (table.to_owned(), fk.to_owned())),
                check: {
                    if let Some(check) = T::check(sea_query::Alias::new(name)) {
                        let mut sql = String::new();
                        sea_query::SqliteQueryBuilder.prepare_expr(&check, &mut sql);
                        Some(sql)
                    } else {
                        None
                    }
                },
            },
            span,
        };
        let old = self.ast.columns.insert(name.to_owned(), item);
        debug_assert!(old.is_none());
    }

    pub fn index(&mut self, cols: &[&'static str], unique: bool, span: (usize, usize)) {
        let def = from_db::Index {
            columns: cols.iter().copied().map(str::to_owned).collect(),
            unique,
        };
        self.ast.indices.insert(Index { def, span });
    }

    pub fn check_unique_compatible<T: EqTyp>(&mut self) {}
}

#[diagnostic::on_unimplemented(
    message = "Can not use `{Self}` as a column type in schema `{S}`",
    note = "`TableRow<Table>` can be used as a schema column types as long as the table `Table` is not #[no_reference]"
)]
pub trait SchemaType<S>: for<'x> IntoExpr<'x, S, Typ = Self::ExprTyp> + MigrateTyp {
    fn check(_col: sea_query::Alias) -> Option<sea_query::SimpleExpr> {
        None
    }
    fn out_to_lazy<'t>(self) -> <Self as MigrateTyp>::Lazy<'t>;
}

impl<S> SchemaType<S> for bool {
    fn check(col: sea_query::Alias) -> Option<sea_query::SimpleExpr> {
        Some(sea_query::Expr::col(col).is_in([CONST_0, CONST_1]))
    }
    fn out_to_lazy<'t>(self) -> <Self as MigrateTyp>::Lazy<'t> {
        self
    }
}
impl<S> SchemaType<S> for String {
    fn out_to_lazy<'t>(self) -> <Self as MigrateTyp>::Lazy<'t> {
        self
    }
}
impl<S> SchemaType<S> for Vec<u8> {
    fn out_to_lazy<'t>(self) -> <Self as MigrateTyp>::Lazy<'t> {
        self
    }
}
impl<S> SchemaType<S> for i64 {
    fn out_to_lazy<'t>(self) -> <Self as MigrateTyp>::Lazy<'t> {
        self
    }
}
impl<S> SchemaType<S> for f64 {
    fn out_to_lazy<'t>(self) -> <Self as MigrateTyp>::Lazy<'t> {
        self
    }
}
impl<S, T: SchemaType<S, Typ: EqTyp>> SchemaType<S> for Option<T> {
    fn out_to_lazy<'t>(self) -> <Self as MigrateTyp>::Lazy<'t> {
        todo!()
    }
}
// only tables with `Referer = ()` are valid columns
impl<T: crate::Table<Referer = ()>> SchemaType<T::Schema> for TableRow<T> {
    fn out_to_lazy<'t>(self) -> <Self as MigrateTyp>::Lazy<'t> {
        crate::Lazy {
            id: self,
            lazy: OnceCell::new(),
            txn: Transaction::new_ref(),
        }
    }
}

pub trait MigrateTyp {
    type ExprTyp;
    type From: MigrateTyp;
    type FromLazy<'x>;
    fn migrate(prev: Self::From) -> Self;
    fn from_lazy(lazy: &Self::FromLazy<'_>) -> Self;
    fn out_to_value(self) -> sea_query::Value;
    type Lazy<'t>: Sized;
    fn out_to_lazy<'t>(self) -> Self::Lazy<'t>;
}
macro_rules! impl_migrate {
    ($typ:ty) => {
        impl MigrateTyp for $typ {
            type ExprTyp = Self;
            type From = Self;
            type FromLazy<'x> = Self;
            fn migrate(prev: Self) -> Self {
                prev
            }
            fn from_lazy(lazy: &Self::FromLazy<'_>) -> Self {
                lazy.clone()
            }
            fn out_to_value(self) -> sea_query::Value {
                self.into()
            }
            type Lazy<'t> = Self;
            fn out_to_lazy<'t>(self) -> Self::Lazy<'t> {
                self
            }
        }
    };
}
impl_migrate!(i64);
impl_migrate!(String);
impl_migrate!(bool);
impl_migrate!(Vec<u8>);
impl_migrate!(f64);

impl<T: MigrateTyp> MigrateTyp for Option<T> {
    type ExprTyp = Option<T::ExprTyp>;
    type From = Option<T::From>;
    type FromLazy<'x> = Option<T::FromLazy<'x>>;
    fn migrate(prev: Self::From) -> Self {
        prev.map(T::migrate)
    }
    fn from_lazy(lazy: &Self::FromLazy<'_>) -> Self {
        lazy.as_ref().map(T::from_lazy)
    }
    fn out_to_value(self) -> sea_query::Value {
        self.map(T::out_to_value)
            .unwrap_or(sea_query::Value::Bool(None))
    }
    type Lazy<'t> = Option<T::Lazy<'t>>;
    fn out_to_lazy<'t>(self) -> Self::Lazy<'t> {
        self.map(T::out_to_lazy)
    }
}

impl<T: crate::Table> MigrateTyp for TableRow<T> {
    type ExprTyp = T;
    type From = TableRow<T::MigrateFrom>;
    type FromLazy<'x> = crate::Lazy<'x, <T as MyTyp>::Prev>;
    fn migrate(prev: Self::From) -> Self {
        TableRow::migrate_row(prev)
    }
    fn from_lazy(lazy: &Self::FromLazy<'_>) -> Self {
        TableRow::migrate_row(lazy.table_row())
    }
    fn out_to_value(self) -> sea_query::Value {
        self.inner.idx.into()
    }
    type Lazy<'t> = crate::Lazy<'t, T>;
    fn out_to_lazy<'t>(self) -> Self::Lazy<'t> {
        crate::Lazy {
            id: self,
            lazy: OnceCell::new(),
            txn: Transaction::new_ref(),
        }
    }
}

#[cfg(test)]
mod tests {
    use sea_query::{Alias, SqliteQueryBuilder};

    use super::*;

    #[test]
    fn test_bool_check() {
        let res = <bool as SchemaType<()>>::check(Alias::new("foo")).unwrap();
        let mut out = String::new();
        SqliteQueryBuilder.prepare_expr(&res, &mut out);
        assert_eq!(out, r#""foo" IN (0, 1)"#);
    }
}
