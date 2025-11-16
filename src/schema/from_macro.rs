use std::{
    collections::{BTreeMap, BTreeSet},
    marker::PhantomData,
};

use crate::value::{EqTyp, MyTyp};

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum ColumnType {
    Integer = 0,
    Float = 1,
    String = 2,
    Blob = 3,
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Column {
    pub typ: ColumnType,
    pub nullable: bool,
    pub fk: Option<(String, String)>,
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Index {
    // column order matters for performance
    pub columns: Vec<String>,
    pub unique: bool,
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Table {
    pub columns: BTreeMap<String, Column>,
    pub indices: BTreeSet<Index>,
}

#[derive(Debug, Hash, Default, PartialEq, Eq)]
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
    pub fn col<T: SchemaType<S>>(&mut self, name: &'static str) {
        let mut item = Column {
            typ: T::TYP,
            nullable: T::NULLABLE,
            fk: None,
        };
        if let Some((table, fk)) = T::FK {
            item.fk = Some((table.to_owned(), fk.to_owned()))
        }
        let old = self.ast.columns.insert(name.to_owned(), item);
        debug_assert!(old.is_none());
    }

    pub fn index(&mut self, cols: &[&'static str], unique: bool) {
        let mut index = Index {
            columns: Vec::default(),
            unique,
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
