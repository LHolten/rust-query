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
/// ## Supported data types:
/// - `i64` (sqlite `integer`)
/// - `f64` (sqlite `real`)
/// - `String` (sqlite `text`)
/// - `Vec<u8>` (sqlite `blob`)
/// - Any table in the same schema (sqlite `integer` with foreign key constraint)
/// - `Option<T>` where `T` is not an `Option` (sqlite nullable)
///
/// Booleans are not supported in schemas yet.
///
/// ## Unique constraints
///
/// To define a unique constraint on a column, you need to add an attribute to the table or field.
///
/// For example:
/// ```
/// #[rust_query::migration::schema(Schema)]
/// #[version(0..=0)]
/// pub mod vN {
///     pub struct User {
///         #[unique]
///         pub email: String,
///         #[unique]
///         pub username: String,
///     }
/// }
/// # fn main() {}
/// ```
/// This will create a single schema with a single table called `user` and two columns.
/// The table will also have two unique contraints.
/// Note that optional types are not allowed in unique constraints.
///
/// ## Multiple versions
/// The macro must be applied to a module named `vN`. This is because the module
/// is a template that is used to generate multiple modules called `v0`, `v1` etc.
/// Each module corresponds to a schema version and contains the types to work with that schema.
///
/// Note in the previous example that the schema version range is `0..=0` so there is only a version 0.
/// The generated code will have a structure like this:
/// ```rust,ignore
/// pub mod v0 {
///     pub struct Schema;
///     pub struct User{..};
///     // a bunch of other stuff
/// }
/// ```
///
/// # Adding tables
/// At some point you might want to add a new table.
/// ```
/// #[rust_query::migration::schema(Schema)]
/// #[version(0..=1)]
/// pub mod vN {
///     pub struct User {
///         #[unique]
///         pub email: String,
///         #[unique]
///         pub username: String,
///     }
///     #[version(1..)] // <-- note that `Game`` has a version range
///     pub struct Game {
///         pub name: String,
///         pub size: i64,
///     }
/// }
/// # fn main() {}
/// ```
/// We now have two schema versions which generates two modules `v0` and `v1`.
/// They look something like this:
/// ```rust,ignore
/// pub mod v0 {
///     pub struct Schema;
///     pub struct User{..};
///     pub mod migrate {..}
///     // a bunch of other stuff
/// }
/// pub mod v1 {
///     pub struct Schema;
///     pub struct User{..};
///     pub struct Game{..};
///     // a bunch of other stuff
/// }
/// ```
///
/// # Changing columns
/// Changing columns is very similar to adding and removing structs.
/// ```
/// use rust_query::migration::{schema, Config};
/// use rust_query::Database;
/// #[schema(Schema)]
/// #[version(0..=1)]
/// pub mod vN {
///     pub struct User {
///         #[unique]
///         pub email: String,
///         #[unique]
///         pub username: String,
///         #[version(1..)] // <-- here
///         pub score: i64,
///     }
/// }
/// // In this case it is required to provide a value for each row that already exists.
/// // This is done with the `v0::migrate::User` struct:
/// pub fn migrate() -> Database<v1::Schema> {
///     let m = Database::migrator(Config::open_in_memory()) // we use an in memory database for this test
///         .expect("database version is before supported versions");
///     let m = m.migrate(|txn| v0::migrate::Schema {
///         user: txn.migrate_ok(|old: v0::User!(email)| v0::migrate::User {
///             score: old.email.len() as i64 // use the email length as the new score
///         }),
///     });
///     m.finish().expect("database version is after supported versions")
/// }
/// # fn main() {}
/// ```
/// The `migrate` function first creates an empty database if it does not exists.
///
/// # `#[from]` Attribute
/// You can use this attribute when renaming or splitting a table.
/// This will make it clear that data in the table should have the
/// same row ids as the `from` table.
///
/// For example:
///
/// ```
/// # use rust_query::migration::schema;
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
/// ```
/// In this example the `Book` table exists in both `v0` and `v1`,
/// however `User` only exists in `v0` and `Author` only exist in `v1`.
/// Note that the `pub author: Author` field only specifies the latest version
/// of the table, it will use the `#[from]` attribute to find previous versions.
///
/// This will work correctly and will let you migrate data from `User` to `Author` with code like this:
///
/// ```rust
/// # use rust_query::migration::{schema, Config};
/// # use rust_query::Database;
/// # fn main() {}
/// # #[schema(Schema)]
/// # #[version(0..=1)]
/// # pub mod vN {
/// #     #[version(..1)]
/// #     pub struct User {
/// #         pub name: String,
/// #     }
/// #     #[version(1..)]
/// #     #[from(User)]
/// #     pub struct Author {
/// #         pub name: String,
/// #     }
/// #     pub struct Book {
/// #         pub author: Author,
/// #     }
/// # }
/// # pub fn migrate() -> Database<v1::Schema> {
/// #     let m = Database::migrator(Config::open_in_memory()) // we use an in memory database for this test
/// #         .expect("database version is before supported versions");
/// let m = m.migrate(|txn| v0::migrate::Schema {
///     author: txn.migrate_ok(|old: v0::User!(name)| v0::migrate::Author {
///         name: old.name,
///     }),
/// });
/// #     m.finish().expect("database version is after supported versions")
/// # }
/// ```
///
/// # `#[no_reference]` Attribute
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
            mod #new_mod {
                #mod_output
            }
        });

        prev_mod = Some(new_mod);
    }

    Ok(output)
}
