#![allow(private_bounds)]
#![doc = include_str!("../README.md")]

extern crate self as rust_query;

mod aggregate;
mod alias;
mod ast;
mod client;
mod db;
mod dummy;
mod hash;
mod migrate;
mod mymap;
mod query;
mod ref_cast_impl;
mod rows;
mod schema_pragma;
mod transaction;
mod value;
mod writable;

pub use crate::dummy::Dummy;
pub use aggregate::aggregate;
pub use client::LocalClient;
pub use db::TableRow;
use hash::TypBuilder;
use ref_cast::RefCast;
pub use rows::Rows;
pub use rust_query_macros::FromDummy;
pub use transaction::{Database, Transaction, TransactionMut, TransactionWeak};
pub use value::{Column, IntoColumn, UnixEpoch};

/// Types that are used as closure arguments.
///
/// You generally don't need to import these types.
pub mod args {
    pub use crate::aggregate::Aggregate;
    pub use crate::query::Query;
}

/// Types to declare schemas and migrations.
///
/// A good starting point is too look at [crate::migration::schema].
pub mod migration {
    pub use crate::migrate::{Alter, Config, Create, Migrator, NoTable};
    pub use expect_test::expect;
    pub use rust_query_macros::schema;
}

/// These items are only exposed for use by the proc macros.
/// Direct use is unsupported.
#[doc(hidden)]
pub mod private {
    pub use crate::db::Col;
    pub use crate::dummy::{Cached, Cacher, Dummy, Row};
    pub use crate::hash::TypBuilder;
    pub use crate::hash::{hash_schema, KangarooHasher};
    pub use crate::migrate::{
        Migration, Schema, SchemaBuilder, TableCreation, TableMigration, TableTypBuilder, C, M,
    };
    pub use crate::query::show_sql;
    pub use crate::value::{MyTyp, Typed, ValueBuilder};
    pub use crate::writable::{Reader, Writable};

    pub use expect_test::Expect;
    pub use ref_cast::RefCast;
    pub use sea_query::SimpleExpr;
}

/// This trait is implemented for all table types as generated by the [crate::migration::schema] macro.
///
/// **You can not implement this trait yourself!**
pub trait Table: Sized + 'static {
    /// The associated type [Table::Ext] is used as the deref target by several types that implement [IntoColumn].
    /// This adds convenient methods to access related tables that have a foreign key constraint.
    #[doc(hidden)]
    type Ext<T>: RefCast<From = T>;

    /// The schema that this table is a part of.
    type Schema;

    /// Please refer to [Rows::join].
    fn join<'inner>(rows: &mut Rows<'inner, Self::Schema>) -> Column<'inner, Self::Schema, Self> {
        rows.join()
    }

    /// The type returned by the [Table::dummy] method.
    type Dummy<'t>;

    /// Create a dummy that can be used for [TransactionMut::try_insert] and [TransactionMut::try_update] etc.
    /// ```rust,ignore
    /// txn.find_and_update(User {
    ///     email: new_email,
    ///     ..user.dummy()
    /// })
    /// .unwrap();
    /// ```
    /// Note that all fields of the dummy have type [Column], so if you want to change the value to something that is not
    /// a [Column], then you need to do one of the following:
    /// - Turn the value into a [Column] with [IntoColumn::into_column].
    /// - Use `#![feature(type_changing_struct_update)]`.
    fn dummy<'t>(val: impl IntoColumn<'t, Self::Schema, Typ = Self>) -> Self::Dummy<'t>;

    /// The type of error when a delete fails due to a foreign key constraint.
    type Referer;

    #[doc(hidden)]
    fn get_referer_unchecked() -> Self::Referer;

    // used for the first join (useful for pragmas)
    #[doc(hidden)]
    fn name(&self) -> String {
        Self::NAME.to_owned()
    }
    #[doc(hidden)]
    fn typs(f: &mut TypBuilder<Self::Schema>);

    #[doc(hidden)]
    const ID: &'static str = "";
    #[doc(hidden)]
    const NAME: &'static str = "";
}

#[test]
fn compile_tests() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile/*.rs");
    t.pass("examples/*.rs");
}
