#![allow(private_bounds)]

extern crate self as rust_query;

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
pub use group::aggregate;
use hash::TypBuilder;
pub use query::Rows;
use ref_cast::RefCast;
pub use rust_query_macros::FromDummy;
pub use token::ThreadToken;
pub use transaction::{Database, Transaction, TransactionMut};
pub use value::{DynValue, UnixEpoch, Value};

/// Types that are the result of a database operation.
pub mod ops {
    pub use crate::db::Col;
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
    pub use crate::hash::TypBuilder;
    pub use crate::hash::{hash_schema, KangarooHasher};
    pub use crate::insert::{Reader, Writable};
    pub use crate::migrate::{
        Migration, Schema, SchemaBuilder, TableCreation, TableMigration, TableTypBuilder, C, M,
    };
    pub use crate::value::{MyTyp, Typed, ValueBuilder};

    pub use expect_test::Expect;
    pub use ref_cast::RefCast;
    pub use sea_query::SimpleExpr;

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
    impl<T: crate::Table> ValidInSchema<T::Schema> for T {
        type N = NotNull;
    }

    pub fn valid_in_schema<S, T: ValidInSchema<S>>() {}
}

pub trait Table: Sized + 'static {
    type Ext<T>: RefCast<From = T>;

    type Schema;

    fn join<'inner>(rows: &mut Rows<'inner, Self::Schema>) -> DynValue<'inner, Self::Schema, Self> {
        rows.join()
    }

    // used for the first join (useful for pragmas)
    #[doc(hidden)]
    fn name(&self) -> String {
        Self::NAME.to_owned()
    }
    #[doc(hidden)]
    fn typs(f: &mut TypBuilder);

    const ID: &'static str = "";
    const NAME: &'static str = "";
}
