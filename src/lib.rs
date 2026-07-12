#![allow(private_bounds, private_interfaces)]
#![doc = include_str!("../README.md")]
#![cfg_attr(not(docsrs), cfg(feature = "base0"))]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate self as rust_query;

#[macro_use]
extern crate static_assertions;

#[cfg(doc)]
#[doc = include_str!("_guide.md")]
pub mod _guide {}
// mod ast;
mod async_db;
mod db;
mod error;
mod joinable;
mod lazy;
mod lower;
mod migrate;
mod mutable;
#[cfg(feature = "__mutants")]
mod mutants;
mod pool;
mod query;
mod rows;
mod schema;
mod select;
mod transaction;
mod value;
mod writable;

use private::Reader;
use schema::from_macro::TypBuilder;
use std::ops::Deref;

pub use async_db::DatabaseAsync;
pub use db::TableRow;
pub use error::Conflict;
pub use lazy::Lazy;
pub use mutable::Mutable;
pub use select::{IntoSelect, Select};
pub use transaction::{Database, Transaction, TransactionWeak};
pub use value::aggregate::aggregate;
pub use value::from_expr::FromExpr;
pub use value::{Expr, into_expr::IntoExpr, optional::optional};

/// Derive [derive@Select] to create a new `*Select` struct.
///
/// This `*Select` struct will implement the [IntoSelect] trait and can be used
/// with [args::Query::into_iter], [Transaction::query_one] etc.
///
/// Usage can also be nested.
///
/// ```
/// #[rust_query::migration::schema(Schema)]
/// pub mod vN {
///     pub struct Thing {
///         pub details: rust_query::TableRow<Details>,
///         pub beta: f64,
///         pub seconds: i64,
///     }
///     pub struct Details {
///         pub name: String,
///     }
/// }
/// use v0::*;
/// use rust_query::{Table, Select, Transaction};
///
/// #[derive(Select)]
/// struct MyData {
///     seconds: i64,
///     is_it_real: bool,
///     name: String,
///     other: OtherData
/// }
///
/// #[derive(Select)]
/// struct OtherData {
///     alpha: f64,
///     beta: f64,
/// }
///
/// fn do_query(db: &Transaction<Schema>) -> Vec<MyData> {
///     db.query(|rows| {
///         let thing = rows.join(Thing);
///
///         rows.into_vec(MyDataSelect {
///             seconds: &thing.seconds,
///             is_it_real: thing.seconds.lt(100),
///             name: &thing.details.name,
///             other: OtherDataSelect {
///                 alpha: thing.beta.add(2.0),
///                 beta: &thing.beta,
///             },
///         })
///     })
/// }
/// # fn main() {}
/// ```
pub use rust_query_macros::Select;

/// Use in combination with `#[rust_query(From = Thing)]` to specify which tables
/// this struct should implement [trait@FromExpr] for.
///
/// The implementation of [trait@FromExpr] will initialize every field from the column with
/// the corresponding name. It is also possible to change the type of each field
/// as long as the new field type implements [trait@FromExpr].
///
/// ```
/// # use rust_query::migration::schema;
/// # use rust_query::{TableRow, FromExpr};
/// #[schema(Example)]
/// pub mod vN {
///     pub struct User {
///         pub name: String,
///         pub score: i64,
///         pub best_game: Option<rust_query::TableRow<Game>>,
///     }
///     pub struct Game;
/// }
///
/// #[derive(FromExpr)]
/// #[rust_query(From = v0::User)]
/// struct MyUserFields {
///     name: String,
///     best_game: Option<TableRow<v0::Game>>,
/// }
/// # fn main() {}
/// ```
pub use rust_query_macros::FromExpr;

use crate::error::FromConflict;

/// Types that are used as closure arguments.
///
/// You generally don't need to import these types.
pub mod args {
    pub use crate::query::{OrderBy, Query};
    pub use crate::rows::Rows;
    pub use crate::value::aggregate::Aggregate;
    pub use crate::value::optional::Optional;
}

/// Types to declare schemas and migrations.
///
/// A good starting point is too look at [crate::migration::schema].
pub mod migration {
    pub use crate::migrate::{
        Migrator,
        config::{Config, ForeignKeys, Synchronous},
        migration::{Migrated, TransactionMigrate},
    };
    #[cfg(feature = "dev")]
    pub use crate::schema::dev::hash_schema;

    #[doc = include_str!("schema/_schema.md")]
    pub use rust_query_macros::schema;
}

/// These items are only exposed for use by the proc macros.
/// Direct use is unsupported.
#[doc(hidden)]
pub mod private {

    pub use crate::joinable::{IntoJoinable, Joinable};
    pub use crate::migrate::{
        Schema, SchemaMigration, TableTypBuilder,
        migration::{Migration, SchemaBuilder},
        with_test_renderer,
    };
    pub use crate::query::get_plan;
    pub use crate::schema::from_macro::{SchemaType, TypBuilder};
    pub use crate::schema::tokenizer::{Token, get_token};
    pub use crate::value::{DbTyp, adhoc_expr, new_column, unique_from_joinable};
    pub use crate::writable::Reader;

    // pub trait Apply {
    //     type Out<T: MigrateTyp>;
    // }

    // pub struct AsNormal;
    // impl Apply for AsNormal {
    //     type Out<T: MigrateTyp> = T;
    // }

    // struct AsExpr<'x, S>(PhantomData<(&'x (), S)>);
    // impl<'x, S> Apply for AsExpr<'x, S> {
    //     type Out<T: MigrateTyp> = crate::Expr<'x, S, T::ExprTyp>;
    // }

    // struct AsLazy<'x>(PhantomData<&'x ()>);
    // impl<'x> Apply for AsLazy<'x> {
    //     type Out<T: MigrateTyp> = T::Lazy<'x>;
    // }

    pub mod doctest_aggregate {
        #[crate::migration::schema(M)]
        pub mod vN {
            pub struct Val {
                pub x: i64,
            }
        }
        pub use crate::aggregate;
        pub use v0::*;

        #[cfg_attr(false, mutants::skip)]
        pub fn get_txn(f: impl Send + FnOnce(&'static mut crate::Transaction<M>)) {
            crate::Database::new(rust_query::migration::Config::open_in_memory())
                .transaction_mut_ok(f)
        }
    }

    pub mod doctest {
        use crate::{Database, Transaction, migrate::config::Config, migration};

        #[migration::schema(Empty)]
        pub mod vN {
            pub struct User {
                #[unique]
                pub name: String,
            }
        }
        pub use v0::*;

        #[cfg_attr(false, mutants::skip)]
        pub fn get_txn(f: impl Send + FnOnce(&'static mut Transaction<Empty>)) {
            let db = Database::new(Config::open_in_memory());
            db.transaction_mut_ok(|txn| {
                txn.insert(User {
                    name: "Alice".to_owned(),
                })
                .unwrap();
                f(txn)
            })
        }
    }
}

/// This trait is implemented for all table types as generated by the [crate::migration::schema] macro.
///
/// **You can not implement this trait yourself!**
pub trait Table: Sized + 'static {
    #[doc(hidden)]
    type Ext2<'t>;

    #[doc(hidden)]
    fn covariant_ext<'x, 't>(val: &'x Self::Ext2<'static>) -> &'x Self::Ext2<'t>;

    #[doc(hidden)]
    fn build_ext2<'t>(val: &Expr<'t, Self::Schema, TableRow<Self>>) -> Self::Ext2<'t>;

    /// The schema that this table is a part of.
    type Schema;

    #[doc(hidden)]
    /// The table that this table can be migrated from.
    type MigrateFrom: Table;

    /// The type of conflict that can result from inserting a row in this table.
    /// This is the same type that is used for row updates too.
    type Conflict: FromConflict;
    /// The type of error when a delete fails due to a foreign key constraint.
    type Referer;

    #[doc(hidden)]
    type Mutable: Deref;
    #[doc(hidden)]
    type Lazy<'t>;

    #[doc(hidden)]
    fn read(&self, f: &mut Reader);

    #[doc(hidden)]
    type Select;

    #[doc(hidden)]
    fn into_select(
        val: Expr<'_, Self::Schema, TableRow<Self>>,
    ) -> Select<'_, Self::Schema, Self::Select>;

    #[doc(hidden)]
    fn select_mutable(select: Self::Select) -> Self::Mutable;

    #[doc(hidden)]
    fn select_lazy<'t>(select: Self::Select) -> Self::Lazy<'t>;

    #[doc(hidden)]
    fn mutable_as_unique(val: &mut Self::Mutable) -> &mut <Self::Mutable as Deref>::Target;

    #[doc(hidden)]
    fn mutable_into_insert(val: Self::Mutable) -> Self
    where
        Self: Sized;

    #[doc(hidden)]
    fn get_referer_unchecked() -> Self::Referer;

    #[doc(hidden)]
    fn typs(f: &mut TypBuilder<Self::Schema>);

    #[doc(hidden)]
    const SPAN: (usize, usize);

    #[doc(hidden)]
    const ID: &'static str;
    #[doc(hidden)]
    const NAME: &'static str;
}

trait CustomJoin: Table {
    fn name(&self) -> lower::JoinableTable;
    fn main_column(&self) -> &'static str;
}

#[test]
#[cfg(feature = "jiff-02")]
fn compile_tests() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile/*.rs");
}
