#![allow(private_bounds)]

extern crate self as rust_query;

mod aggregate;
mod alias;
mod ast;
mod client;
mod db;
mod dummy;
mod exec;
mod hash;
mod insert;
mod migrate;
mod mymap;
mod pragma;
mod ref_cast_impl;
mod rows;
mod token;
mod transaction;
mod value;

pub use crate::dummy::Dummy;
pub use aggregate::aggregate;
pub use db::TableRow;
use hash::TypBuilder;
use ref_cast::RefCast;
pub use rows::Rows;
pub use rust_query_macros::FromDummy;
pub use token::ThreadToken;
pub use transaction::{Database, Transaction, TransactionMut};
pub use value::{IntoColumn, UnixEpoch, Column};

/// Types that are used as closure arguments.
///
/// You generally don't need to import these types.
pub mod args {
    pub use crate::aggregate::Aggregate;
    pub use crate::exec::Query;
}

/// Types to declare schemas and migrations.
///
/// A good starting point is too look at [crate::migration::schema].
pub mod migration {
    pub use crate::migrate::{Alter, Create, Migrator, NoTable, Prepare};
    pub use expect_test::expect;
    pub use rust_query_macros::schema;
}

/// These items are only exposed for use by the proc macros.
/// Direct use is unsupported.
#[doc(hidden)]
pub mod private {
    pub use crate::db::Col;
    pub use crate::dummy::{Cached, Cacher, Dummy, Row};
    pub use crate::exec::show_sql;
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

    fn join<'inner>(rows: &mut Rows<'inner, Self::Schema>) -> Column<'inner, Self::Schema, Self> {
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
