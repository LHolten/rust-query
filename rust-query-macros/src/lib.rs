use dummy::{dummy_impl, from_expr};
use heck::{ToSnekCase, ToUpperCamelCase};
use multi::{SingleVersionTable, VersionedSchema};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Ident, ItemMod, ItemStruct};
use table::define_all_tables;

mod dummy;
mod migrations;
mod multi;
mod parse;
mod table;
mod unique;

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
#[proc_macro_attribute]
pub fn schema(
    attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let name = syn::parse_macro_input!(attr as syn::Ident);
    let item = syn::parse_macro_input!(item as ItemMod);

    match generate(name, item) {
        Ok(x) => x,
        Err(e) => e.into_compile_error(),
    }
    .into()
}

/// Derive [Select] to create a new `*Select` struct.
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
#[proc_macro_derive(Select)]
pub fn from_row(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let item = syn::parse_macro_input!(item as ItemStruct);
    match dummy_impl(item) {
        Ok(x) => x,
        Err(e) => e.into_compile_error(),
    }
    .into()
}

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
#[proc_macro_derive(FromExpr, attributes(rust_query))]
pub fn from_expr_macro(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let item = syn::parse_macro_input!(item as ItemStruct);
    match from_expr(item) {
        Ok(x) => x,
        Err(e) => e.into_compile_error(),
    }
    .into()
}

fn make_generic(name: &Ident) -> Ident {
    let normalized = name.to_string().to_upper_camel_case();
    format_ident!("_{normalized}")
}

fn to_lower(name: &Ident) -> Ident {
    let normalized = name.to_string().to_snek_case();
    format_ident!("{normalized}")
}

fn generate(schema_name: Ident, item: syn::ItemMod) -> syn::Result<TokenStream> {
    let schema = VersionedSchema::parse(item)?;

    let mut output = quote! {};
    let mut prev_mod = None;

    let mut iter = schema
        .versions
        .clone()
        .map(|version| Ok((version, schema.get(version)?)))
        .collect::<syn::Result<Vec<_>>>()?
        .into_iter()
        .peekable();

    while let Some((version, mut new_tables)) = iter.next() {
        let next_mod = iter
            .peek()
            .map(|(peek_version, _)| format_ident!("v{peek_version}"));
        let mut mod_output =
            define_all_tables(&schema_name, &prev_mod, &next_mod, version, &mut new_tables)?;

        let new_mod = format_ident!("v{version}");

        if let Some((peek_version, peek_tables)) = iter.peek() {
            let peek_mod = format_ident!("v{peek_version}");
            let m = migrations::migrations(
                &schema_name,
                new_tables,
                peek_tables,
                quote! {super},
                quote! {super::super::#peek_mod},
            )?;
            mod_output.extend(quote! {
                pub mod migrate {
                    #m
                }
            });
        }

        output.extend(quote! {
            pub mod #new_mod {
                #mod_output
            }
        });

        prev_mod = Some(new_mod);
    }

    Ok(output)
}
