#![allow(private_bounds)]

mod ast;
mod client;
mod exec;
mod group;
mod hash;
mod insert;
mod migrate;
mod mymap;
mod pragma;
mod query;
mod value;

pub use client::Client;
pub use expect_test::expect;
pub use migrate::{Migrator, Prepare};
pub use query::Query;
pub use rust_query_macros::schema;
pub use value::{Db, Null, UnixEpoch, Value};

pub mod ops {
    pub use crate::value::{IsNotNull, MyAdd, MyAnd, MyEq, MyLt, MyNot, UnwrapOr};
}

pub mod args {
    pub use crate::exec::{Execute, Row};
    pub use crate::group::Aggregate;
}

#[doc(hidden)]
pub mod private {
    pub use crate::hash::hash_schema;
    pub use crate::insert::{Reader, Writable};
    pub use crate::migrate::{Migration, Schema, SchemaBuilder, TableMigration, TableTypBuilder};
    pub use expect_test::Expect;
}

use ast::{Joins, MyTable};

use elsa::FrozenVec;
use value::{Field, FieldAlias, MyAlias, MyIdenT};

#[derive(Default)]
#[doc(hidden)]
pub struct TypBuilder {
    ast: hash::Table,
}

impl TypBuilder {
    pub fn col<T: MyIdenT>(&mut self, name: &'static str) {
        let mut item = hash::Column {
            name: name.to_owned(),
            typ: T::TYP,
            nullable: T::NULLABLE,
            fk: None,
        };
        if let Some((table, fk)) = T::FK {
            item.fk = Some((table.to_owned(), fk.to_owned()))
        }
        self.ast.columns.insert(item)
    }

    pub fn unique(&mut self, cols: &[&'static str]) {
        let mut unique = hash::Unique::default();
        for &col in cols {
            unique.columns.insert(col.to_owned());
        }
        self.ast.uniques.insert(unique);
    }
}

#[doc(hidden)]
pub trait Table {
    // const NAME: &'static str;
    // these names are defined in `'query`
    type Dummy<'t>;

    type Schema;

    fn name(&self) -> String;

    fn build(f: Builder<'_>) -> Self::Dummy<'_>;

    fn typs(f: &mut TypBuilder);
}

// TODO: maybe remove this trait?
#[doc(hidden)]
pub trait ValidInSchema<S> {}

impl<S> ValidInSchema<S> for String {}
impl<S> ValidInSchema<S> for i64 {}
impl<S> ValidInSchema<S> for f64 {}
impl<S, T: ValidInSchema<S>> ValidInSchema<S> for Option<T> {}
impl<T: Table> ValidInSchema<T::Schema> for T {}

#[doc(hidden)]
pub fn valid_in_schema<S, T: ValidInSchema<S>>() {}

#[doc(hidden)]
pub trait HasId: Table {
    const ID: &'static str;
    const NAME: &'static str;
}

#[doc(hidden)]
pub struct Builder<'a> {
    joined: &'a FrozenVec<Box<(Field, MyTable)>>,
    table: MyAlias,
}

impl<'a> Builder<'a> {
    fn new(joins: &'a Joins) -> Self {
        Self::new_full(&joins.joined, joins.table)
    }

    fn new_full(joined: &'a FrozenVec<Box<(Field, MyTable)>>, table: MyAlias) -> Self {
        Builder { joined, table }
    }

    pub fn col<T: MyIdenT>(&self, name: &'static str) -> Db<'a, T> {
        let field = FieldAlias {
            table: self.table,
            col: Field::Str(name),
        };
        T::iden_full(self.joined, field)
    }
}

pub struct NoTable(());

impl MyIdenT for NoTable {
    type Info<'t> = value::ValueInfo;

    const TYP: hash::ColumnType = hash::ColumnType::Integer;
}
