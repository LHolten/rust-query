use std::{
    collections::{BTreeMap, BTreeSet},
    marker::PhantomData,
};

use sea_query::{ExprTrait, QueryBuilder};

use crate::{
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
    pub tables: BTreeMap<String, Table>,
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

struct Null;
struct NotNull;

// TODO: maybe remove this trait?
// currently this prevents storing booleans and nested `Option`.
#[diagnostic::on_unimplemented(
    message = "Can not use `{Self}` as a column type in schema `{S}`",
    note = "Table names can be used as schema column types as long as they are not #[no_reference]"
)]
trait SchemaType<S>: MyTyp {
    type N;
    fn check(_col: sea_query::Alias) -> Option<sea_query::SimpleExpr> {
        None
    }
}

impl<S> SchemaType<S> for bool {
    type N = NotNull;
    fn check(col: sea_query::Alias) -> Option<sea_query::SimpleExpr> {
        Some(sea_query::Expr::col(col).is_in([CONST_0, CONST_1]))
    }
}
impl<S> SchemaType<S> for String {
    type N = NotNull;
}
impl<S> SchemaType<S> for Vec<u8> {
    type N = NotNull;
}
impl<S> SchemaType<S> for i64 {
    type N = NotNull;
}
impl<S> SchemaType<S> for f64 {
    type N = NotNull;
}
impl<S, T: SchemaType<S, N = NotNull>> SchemaType<S> for Option<T> {
    type N = Null;
}
// only tables with `Referer = ()` are valid columns
#[diagnostic::do_not_recommend]
impl<T: crate::Table<Referer = ()>> SchemaType<T::Schema> for T {
    type N = NotNull;
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
