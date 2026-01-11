#![allow(private_bounds, private_interfaces)]
#![doc = include_str!("../README.md")]

extern crate self as rust_query;

#[macro_use]
extern crate static_assertions;

mod alias;
mod ast;
mod async_db;
mod db;
mod joinable;
mod lazy;
mod migrate;
mod mutable;
#[cfg(feature = "mutants")]
mod mutants;
mod mymap;
mod pool;
mod query;
mod rows;
mod schema;
mod select;
mod transaction;
mod value;
mod writable;

use alias::JoinableTable;
use private::Reader;
use schema::from_macro::TypBuilder;
use std::ops::Deref;
use value::MyTyp;

pub use async_db::DatabaseAsync;
pub use db::TableRow;
pub use lazy::Lazy;
pub use mutable::Mutable;
pub use select::{IntoSelect, Select};
pub use transaction::{Database, Transaction, TransactionWeak};
#[expect(deprecated)]
pub use value::UnixEpoch;
pub use value::aggregate::aggregate;
pub use value::trivial::FromExpr;
pub use value::{Expr, IntoExpr, optional::optional};
pub use writable::Update;

/// Derive [derive@Select] to create a new `*Select` struct.
///
/// This `*Select` struct will implement the `IntoSelect` trait and can be used with `Query::into_vec`
/// or `Transaction::query_one`.
///
/// Usage can also be nested.
///
/// Example:
/// ```
/// #[rust_query::migration::schema(Schema)]
/// pub mod vN {
///     pub struct Thing {
///         pub details: Details,
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
/// this struct should implement `FromExpr` for.
///
/// The implementation of `FromExpr` will initialize every field from the column with
/// the corresponding name. It is also possible to change the type of each field
/// as long as the new field type implements `FromExpr`.
///
/// ```
/// # use rust_query::migration::schema;
/// # use rust_query::{TableRow, FromExpr};
/// #[schema(Example)]
/// pub mod vN {
///     pub struct User {
///         pub name: String,
///         pub score: i64,
///         pub best_game: Option<Game>,
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

    /// Use this macro to define your schema.
    ///
    /// The macro must be applied to a module named `vN`. This is because the module
    /// is a template that can used to generate multiple modules called `v0`, `v1` etc.
    /// By default, only one module named `v0` is generated.
    ///
    /// ```
    /// #[rust_query::migration::schema(SchemaName)]
    /// pub mod vN {
    ///     pub struct TableName {
    ///         pub column_name: i64,
    ///     }
    /// }
    /// use v0::TableName; // the actual module name is `v0`
    /// # fn main() {}
    /// ```
    ///
    /// Note that the schema module, table structs and column fields all must be `pub`.
    /// The `id` column field is currently reserved for internal use and can not be used.
    ///
    /// Supported data types are:
    /// - `i64` (sqlite `integer`)
    /// - `f64` (sqlite `real`)
    /// - `String` (sqlite `text`)
    /// - `Vec<u8>` (sqlite `blob`)
    /// - `bool` (sqlite `integer` with `CHECK "col" IN (0, 1)`)
    /// - Any table in the same schema (sqlite `integer` with foreign key constraint)
    /// - `Option<T>` where `T` is not an `Option` (sqlite nullable)
    ///
    /// ## Unique constraints
    ///
    /// To define a unique constraint on a column, you need to add an attribute to the table or field.
    ///
    /// For example:
    /// ```
    /// #[rust_query::migration::schema(SchemaName)]
    /// pub mod vN {
    ///     #[unique(username, movie)] // <-- here
    ///     pub struct Rating {
    ///         pub movie: String,
    ///         pub username: String,
    ///         #[unique] // <-- or here
    ///         pub uuid: String,
    ///     }
    /// }
    /// ```
    /// This will create a single schema version with a single table called `rating` and three columns.
    /// The table will also have two unique contraints (one on the `uuid` column and one on the combination of `username` and `movie`).
    /// Note that optional types are not allowed in unique constraints.
    ///
    /// ## Indexes
    /// Indexes are very similar to unique constraints, but they don't require the columns to be unique.
    /// These are useful to prevent sqlite from having to scan a whole table.
    /// To incentivise creating indices you also get some extra sugar to use the index!
    ///
    /// ```
    /// #[rust_query::migration::schema(SchemaName)]
    /// pub mod vN {
    ///     pub struct Topic {
    ///         #[unique]
    ///         pub title: String,
    ///         #[index]
    ///         pub category: String,
    ///     }
    /// }
    /// fn test(txn: &rust_query::Transaction<v0::SchemaName>) {
    ///     let _ = txn.lazy_iter(v0::Topic.category("sports"));
    ///     let _ = txn.lazy(v0::Topic.title("star wars"));
    /// }
    /// ```
    ///
    /// The `TableName.column_name(value)` syntax is only allowed if `TableName` has an index or
    /// unique constraint that starts with `column_name`.
    ///
    /// Adding and removing indexes and changing the order of columns in indexes and unique constraints
    /// is considered backwards compatible and thus does not require a new schema version.
    ///
    /// # Multiple schema versions
    ///
    /// At some point you might want to change something substantial in your schema.
    /// It would be really sad if you had to throw away all the old data in your database.
    /// That is why [rust_query] allows us to define multiple schema versions and how to transition between them.
    ///
    /// ## Adding tables
    /// One of the simplest things to do is adding a new table.
    ///
    /// ```
    /// #[rust_query::migration::schema(SchemaName)]
    /// #[version(0..=1)]
    /// pub mod vN {
    ///     pub struct User {
    ///         #[unique]
    ///         pub username: String,
    ///     }
    ///     #[version(1..)] // <-- note that `Game`` has a version range
    ///     pub struct Game {
    ///         #[unique]
    ///         pub name: String,
    ///         pub size: i64,
    ///     }
    /// }
    ///
    /// // These are just examples of tables you can use
    /// use v0::SchemaName as _;
    /// use v1::SchemaName as _;
    /// use v0::User as _;
    /// use v1::User as _;
    /// // `v0::Game` does not exist
    /// use v1::Game as _;
    /// # fn main() {}
    ///
    /// fn migrate() -> rust_query::Database<v1::SchemaName> {
    ///     rust_query::Database::migrator(rust_query::migration::Config::open("test.db"))
    ///         .expect("database version is before supported versions")
    ///         .migrate(|_txn| v0::migrate::SchemaName {})
    ///         .finish()
    ///         .expect("database version is after supported versions")
    /// }
    /// ```
    /// The migration itself is not very interesting because new tables are automatically created
    /// without any data. To have some initial data, take a look at the `#[from]` attribute down below or use
    /// [crate::migration::Migrator::fixup].
    ///
    /// ## Changing columns
    /// Changing columns is very similar to adding and removing structs.
    /// ```
    /// use rust_query::migration::{schema, Config};
    /// use rust_query::{Database, Lazy};
    /// #[schema(Schema)]
    /// #[version(0..=1)]
    /// pub mod vN {
    ///     pub struct User {
    ///         #[unique]
    ///         pub username: String,
    ///         #[version(1..)] // <-- here
    ///         pub score: i64,
    ///     }
    /// }
    /// pub fn migrate() -> Database<v1::Schema> {
    ///     Database::migrator(Config::open_in_memory()) // we use an in memory database for this test
    ///         .expect("database version is before supported versions")
    ///         .migrate(|txn| v0::migrate::Schema {
    ///             // In this case it is required to provide a value for each row that already exists.
    ///             // This is done with the `v0::migrate::User` struct:
    ///             user: txn.migrate_ok(|old: Lazy<v0::User>| v0::migrate::User {
    ///                 score: old.username.len() as i64 // use the username length as the new score
    ///             }),
    ///         })
    ///         .finish()
    ///         .expect("database version is after supported versions")
    /// }
    /// # fn main() {}
    /// ```
    ///
    /// ## `#[from(TableName)]` Attribute
    /// You can use this attribute when renaming or splitting a table.
    /// This will make it clear that data in the table should have the
    /// same row ids as the `from` table.
    ///
    /// For example:
    ///
    /// ```
    /// # use rust_query::migration::{schema, Config};
    /// # use rust_query::{Database, Lazy};
    /// # fn main() {}
    /// #[schema(Schema)]
    /// #[version(0..=1)]
    /// pub mod vN {
    ///     #[version(..1)]
    ///     pub struct User {
    ///         pub name: String,
    ///     }
    ///     #[version(1..)]
    ///     #[from(User)]
    ///     pub struct Author {
    ///         pub name: String,
    ///     }
    ///     pub struct Book {
    ///         pub author: Author,
    ///     }
    /// }
    /// pub fn migrate() -> Database<v1::Schema> {
    ///     Database::migrator(Config::open_in_memory()) // we use an in memory database for this test
    ///         .expect("database version is before supported versions")
    ///         .migrate(|txn| v0::migrate::Schema {
    ///             author: txn.migrate_ok(|old: Lazy<v0::User>| v0::migrate::Author {
    ///                 name: old.name.clone(),
    ///             }),
    ///         })
    ///         .finish()
    ///         .expect("database version is after supported versions")
    /// }
    /// ```
    /// In this example the `Book` table exists in both `v0` and `v1`,
    /// however `User` only exists in `v0` and `Author` only exist in `v1`.
    /// Note that the `pub author: Author` field only specifies the latest version
    /// of the table, it will use the `#[from]` attribute to find previous versions.
    ///
    /// ## `#[no_reference]` Attribute
    /// You can put this attribute on your table definitions and it will make it impossible
    /// to have foreign key references to such table.
    /// This makes it possible to use `TransactionWeak::delete_ok`.
    pub use rust_query_macros::schema;
}

/// These items are only exposed for use by the proc macros.
/// Direct use is unsupported.
#[doc(hidden)]
pub mod private {
    use std::marker::PhantomData;

    pub use crate::joinable::{IntoJoinable, Joinable};
    pub use crate::migrate::{
        Schema, SchemaMigration, TableTypBuilder,
        migration::{Migration, SchemaBuilder},
    };
    pub use crate::query::get_plan;
    pub use crate::schema::from_macro::TypBuilder;
    pub use crate::value::{
        DynTypedExpr, MyTyp, Typed, ValueBuilder, adhoc_expr, new_column, unique_from_joinable,
    };
    pub use crate::writable::{Reader, TableInsert};

    pub struct Lazy<'t>(PhantomData<&'t ()>);
    pub struct Ignore;
    pub struct Custom<T>(PhantomData<T>);
    pub struct AsUpdate;
    pub struct AsExpr<'t>(PhantomData<&'t ()>);

    pub trait Apply {
        type Out<T: MyTyp, S>;
    }

    impl<'t> Apply for Lazy<'t> {
        type Out<T: MyTyp, S> = T::Lazy<'t>;
    }

    impl Apply for Ignore {
        type Out<T: MyTyp, S> = ();
    }

    impl<X> Apply for Custom<X> {
        type Out<T: MyTyp, S> = X;
    }

    impl Apply for AsUpdate {
        type Out<T: MyTyp, S> = crate::Update<S, T>;
    }

    impl<'t> Apply for AsExpr<'t> {
        type Out<T: MyTyp, S> = crate::Expr<'t, S, T>;
    }

    pub trait UpdateOrUnit<S, T>: Default {}
    impl<S, T: MyTyp> UpdateOrUnit<S, T> for crate::Update<S, T> {}
    impl<S, T> UpdateOrUnit<S, T> for () {}

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

        #[cfg_attr(test, mutants::skip)] // this function is only used in doc tests
        pub fn get_txn(f: impl Send + FnOnce(&'static mut Transaction<Empty>)) {
            let db = Database::new(Config::open_in_memory());
            db.transaction_mut_ok(|txn| {
                txn.insert(User { name: "Alice" }).unwrap();
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
    fn build_ext2<'t>(val: &Expr<'t, Self::Schema, Self>) -> Self::Ext2<'t>;

    /// The schema that this table is a part of.
    type Schema;

    #[doc(hidden)]
    /// The table that this table can be migrated from.
    type MigrateFrom: MyTyp;

    /// The type of conflict that can result from inserting a row in this table.
    /// This is the same type that is used for row updates too.
    type Conflict;

    /// The type of updates used by [Transaction::update_ok].
    type UpdateOk;
    /// The type of updates used by [Transaction::update].
    type Update;
    /// The type of error when a delete fails due to a foreign key constraint.
    type Referer;

    #[doc(hidden)]
    type Mutable: Deref;
    #[doc(hidden)]
    type Lazy<'t>;
    #[doc(hidden)]
    type Insert;

    #[doc(hidden)]
    fn read(val: &Self::Insert, f: &mut Reader<Self::Schema>);

    #[doc(hidden)]
    fn get_conflict_unchecked(
        txn: &Transaction<Self::Schema>,
        val: &Self::Insert,
    ) -> Self::Conflict;

    #[doc(hidden)]
    fn select_mutable(val: Expr<'_, Self::Schema, Self>)
    -> Select<'_, Self::Schema, Self::Mutable>;

    #[doc(hidden)]
    fn mutable_into_update(val: Self::Mutable) -> Self::Update;

    #[doc(hidden)]
    fn mutable_as_unique(val: &mut Self::Mutable) -> &mut <Self::Mutable as Deref>::Target;

    #[doc(hidden)]
    fn update_into_try_update(val: Self::UpdateOk) -> Self::Update;

    #[doc(hidden)]
    fn apply_try_update(val: Self::Update, old: Expr<'static, Self::Schema, Self>) -> Self::Insert;

    #[doc(hidden)]
    fn get_referer_unchecked() -> Self::Referer;

    #[doc(hidden)]
    fn get_lazy<'t>(txn: &'t Transaction<Self::Schema>, row: TableRow<Self>) -> Self::Lazy<'t>;

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
    fn name(&self) -> JoinableTable;
}

#[test]
fn compile_tests() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile/*.rs");
}
