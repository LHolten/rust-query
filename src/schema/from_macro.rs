use std::{
    collections::{BTreeMap, BTreeSet},
    marker::PhantomData,
};

use crate::{
    schema::canonical,
    value::{EqTyp, MyTyp},
};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Column {
    pub def: canonical::Column,
    pub span: (usize, usize),
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Index {
    // column order matters for performance
    pub columns: Vec<String>,
    pub unique: bool,
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
            },
            span,
        };
        let old = self.ast.columns.insert(name.to_owned(), item);
        debug_assert!(old.is_none());
    }

    pub fn index(&mut self, cols: &[&'static str], unique: bool, span: (usize, usize)) {
        let mut index = Index {
            columns: Vec::default(),
            unique,
            span,
        };
        for &col in cols {
            index.columns.push(col.to_owned());
        }
        self.ast.indices.insert(index);
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
