use std::{
    collections::{BTreeMap, BTreeSet},
    marker::PhantomData,
};

use sea_query::{ExprTrait, QueryBuilder};

use crate::{
    IntoExpr, TableRow,
    ast::{CONST_0, CONST_1},
    schema::{canonical, from_db},
    value::{DbTyp, EqTyp},
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
    pub fn col<T: SchemaType<S>>(&mut self, name: &'static str, span: (usize, usize)) {
        let item = Column {
            def: canonical::Column {
                typ: T::TYP,
                nullable: T::NULLABLE,
                fk: T::FK.map(|(table, fk)| (table.to_owned(), fk.to_owned())),
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
pub trait SchemaType<S>: for<'x> IntoExpr<'x, S, Typ = Self> + DbTyp {
    fn check(_col: sea_query::Alias) -> Option<sea_query::SimpleExpr> {
        None
    }
}

impl<S> SchemaType<S> for bool {
    fn check(col: sea_query::Alias) -> Option<sea_query::SimpleExpr> {
        Some(sea_query::Expr::col(col).is_in([CONST_0, CONST_1]))
    }
}
impl<S> SchemaType<S> for String {}
impl<S> SchemaType<S> for Vec<u8> {}
impl<S> SchemaType<S> for i64 {}
impl<S> SchemaType<S> for f64 {}
impl<S, T: SchemaType<S, Typ: EqTyp>> SchemaType<S> for Option<T> {}
// only tables with `Referer = ()` are valid columns
impl<T: crate::Table<Referer = ()>> SchemaType<T::Schema> for TableRow<T> {}

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
