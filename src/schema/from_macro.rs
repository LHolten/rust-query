use std::{
    collections::{BTreeMap, BTreeSet},
    marker::PhantomData,
};

use sea_query::QueryBuilder;

use crate::{
    FromExpr, IntoExpr,
    schema::{canonical, from_db},
    value::{DbTyp, EqTyp, StorableTyp},
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
                typ: <T::Typ as DbTyp>::TYP,
                nullable: <T::Typ as DbTyp>::NULLABLE,
                fk: <T::Typ as DbTyp>::FK.map(|(table, fk)| (table.to_owned(), fk.to_owned())),
                check: {
                    if let Some(check) = <T::Typ as DbTyp>::check(sea_query::Alias::new(name)) {
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
pub trait SchemaType<S>: IntoExpr<'static, S> + FromExpr<S, Self::Typ> {}

impl<T, S> SchemaType<S> for T where
    T: IntoExpr<'static, S, Typ: StorableTyp> + FromExpr<S, Self::Typ>
{
}

#[cfg(test)]
mod tests {
    use sea_query::{Alias, SqliteQueryBuilder};

    use super::*;

    #[test]
    fn test_bool_check() {
        let res = <bool as DbTyp>::check(Alias::new("foo")).unwrap();
        let mut out = String::new();
        SqliteQueryBuilder.prepare_expr(&res, &mut out);
        assert_eq!(out, r#""foo" IN (0, 1)"#);
    }
}
