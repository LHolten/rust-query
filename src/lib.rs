#![allow(private_bounds, private_interfaces)]
#![doc = include_str!("../README.md")]

extern crate self as rust_query;

#[macro_use]
extern crate static_assertions;

mod aggregate;
mod alias;
mod ast;
mod client;
mod db;
mod dummy_impl;
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

pub use aggregate::aggregate;
pub use client::LocalClient;
pub use db::TableRow;
pub use dummy_impl::{IntoSelect, IntoSelectExt, Select};
use hash::TypBuilder;
use private::TableInsert;
use ref_cast::RefCast;
use rows::Rows;
pub use rust_query_macros::{FromExpr, Select};
pub use transaction::{Database, Transaction, TransactionMut, TransactionWeak};
pub use value::trivial::FromExpr;
pub use value::{Expr, IntoExpr, UnixEpoch, optional::optional};
pub use writable::Update;

/// Types that are used as closure arguments.
///
/// You generally don't need to import these types.
pub mod args {
    pub use crate::aggregate::Aggregate;
    pub use crate::query::Query;
    pub use crate::rows::Rows;
    pub use crate::value::optional::Optional;
}

/// Types to declare schemas and migrations.
///
/// A good starting point is too look at [crate::migration::schema].
pub mod migration {
    #[cfg(feature = "dev")]
    pub use crate::hash::dev::hash_schema;
    pub use crate::migrate::{Config, Entry, Migrator};
    pub use rust_query_macros::schema;
}

/// These items are only exposed for use by the proc macros.
/// Direct use is unsupported.
#[doc(hidden)]
pub mod private {
    pub use crate::db::Col;
    pub use crate::hash::TypBuilder;
    pub use crate::migrate::{
        CacheAndRead, Migration, Schema, SchemaBuilder, TableCreation, TableMigration,
        TableTypBuilder,
    };
    pub use crate::query::show_sql;
    pub use crate::value::{MyTyp, Typed, ValueBuilder, into_owned, new_column, new_dummy};
    pub use crate::writable::{Reader, TableInsert};

    pub use ref_cast::RefCast;
    pub use sea_query::SimpleExpr;
}

/// This trait is implemented for all table types as generated by the [crate::migration::schema] macro.
///
/// **You can not implement this trait yourself!**
pub trait Table: Sized + 'static {
    /// The associated type [Table::Ext] is used as the deref target by several types that implement [IntoExpr].
    /// This adds convenient methods to access related tables that have a foreign key constraint.
    #[doc(hidden)]
    type Ext<T>: RefCast<From = T>;

    /// The schema that this table is a part of.
    type Schema;

    /// Please refer to [Rows::join].
    fn join<'inner>(rows: &mut Rows<'inner, Self::Schema>) -> Expr<'inner, Self::Schema, Self> {
        rows.join()
    }

    type Conflict<'t>;
    type Update<'t>;
    type TryUpdate<'t>;

    #[doc(hidden)]
    fn update_into_try_update<'t>(val: Self::Update<'t>) -> Self::TryUpdate<'t>;

    #[doc(hidden)]
    fn apply_try_update<'t>(
        val: Self::TryUpdate<'t>,
        old: Expr<'t, Self::Schema, Self>,
    ) -> impl TableInsert<'t, T = Self, Schema = Self::Schema, Conflict = Self::Conflict<'t>>;

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
    const ID: &'static str;
    #[doc(hidden)]
    const NAME: &'static str;
}

#[test]
fn compile_tests() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile/*.rs");
}
