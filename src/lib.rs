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
mod ref_cast_impl;
mod token;
mod transaction;
mod value;

pub use db::Row;
pub use query::Rows;
pub use rust_query_macros::FromDummy;
pub use token::ThreadToken;
pub use transaction::{Database, Transaction, TransactionMut};
pub use value::{UnixEpoch, Val, Value};

/// Types that are the result of a database operation.
pub mod ops {
    pub use crate::db::{Col, Join};
    pub use crate::group::Aggr;
    pub use crate::value::operations::{Add, And, Assume, Const, Eq, IsNotNull, Lt, Not, UnwrapOr};
}

/// Types that are used as closure arguments.
pub mod args {
    pub use crate::exec::Query;
    pub use crate::group::Aggregate;
}

/// Types to declare schemas and migrations.
pub mod migration {
    pub use crate::migrate::{Migrator, NoTable, Prepare};
    pub use expect_test::expect;
    pub use rust_query_macros::schema;
}

#[doc(hidden)]
pub mod private {
    pub use crate::exec::show_sql;
    pub use crate::from_row::{Cached, Cacher, Dummy, Row};
    pub use crate::hash::{hash_schema, KangarooHasher};
    pub use crate::insert::{Reader, Writable};
    pub use crate::migrate::{Migration, Schema, SchemaBuilder, TableMigration, TableTypBuilder};
    pub use crate::value::{MyTyp, NoParam, ValueBuilder};

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
pub trait Table: Sized + 'static {
    // const NAME: &'static str;
    // these names are defined in `'query`
    type Dummy<T>: RefCast<From = T>;

    type Schema;

    fn name(&self) -> String;

    fn typs(f: &mut TypBuilder);
}

struct Null;
struct NotNull;

// TODO: maybe remove this trait?
// currently this prevents storing booleans and nested enums.
trait ValidInSchema<S> {
    type N;
}

impl<S> ValidInSchema<S> for String {
    type N = NotNull;
}
impl<S> ValidInSchema<S> for i64 {
    type N = NotNull;
}
impl<S> ValidInSchema<S> for f64 {
    type N = NotNull;
}
impl<S, T: ValidInSchema<S, N = NotNull>> ValidInSchema<S> for Option<T> {
    type N = Null;
}
impl<T: Table> ValidInSchema<T::Schema> for T {
    type N = NotNull;
}

#[doc(hidden)]
pub fn valid_in_schema<S, T: ValidInSchema<S>>() {}

#[doc(hidden)]
pub trait HasId: Table {
    const ID: &'static str;
    const NAME: &'static str;
}
