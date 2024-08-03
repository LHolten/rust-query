#![allow(private_bounds)]

use ref_cast::RefCast;
use value::MyTyp;

mod alias;
mod ast;
mod client;
mod db;
mod exec;
mod from_row;
mod group;
mod hash;
mod insert;
mod migrate;
mod mymap;
mod pragma;
mod query;
mod value;

pub use client::Client;
pub use db::{Db, Just};
pub use expect_test::expect;
pub use migrate::{Migrator, Prepare};
pub use query::Query;
pub use rust_query_macros::schema;
pub use rust_query_macros::FromRow;
pub use value::{UnixEpoch, Value};

pub mod ops {
    pub use crate::db::Col;
    pub use crate::value::{Assume, IsNotNull, MyAdd, MyAnd, MyEq, MyLt, MyNot, UnwrapOr};
}

pub mod args {
    pub use crate::exec::Execute;
    pub use crate::group::Aggregate;
    pub use crate::migrate::ReadClient;
}

#[doc(hidden)]
pub mod private {
    pub use crate::from_row::{Cached, Cacher, FromRow, Row};
    pub use crate::hash::hash_schema;
    pub use crate::insert::{Reader, Writable};
    pub use crate::migrate::{Migration, Schema, SchemaBuilder, TableMigration, TableTypBuilder};
    pub use crate::value::{MyTyp, ValueBuilder};

    pub use expect_test::Expect;
    pub use ref_cast::RefCast;
    pub use sea_query::SimpleExpr;
}

#[derive(Default)]
#[doc(hidden)]
pub struct TypBuilder {
    ast: hash::Table,
}

impl TypBuilder {
    pub fn col<T: MyTyp>(&mut self, name: &'static str) {
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
pub trait Table: 'static {
    // const NAME: &'static str;
    // these names are defined in `'query`
    type Dummy<T>: RefCast<From = T>;

    type Schema;

    fn name(&self) -> String;

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

/// Special table name that is used as souce of newly created tables.
pub struct NoTable(());
