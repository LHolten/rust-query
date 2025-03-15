use std::{collections::BTreeMap, ops::Not};

use dummy::{dummy_impl, from_expr};
use heck::{ToSnekCase, ToUpperCamelCase};
use multi::{SingleVersionColumn, SingleVersionTable, VersionedSchema};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Ident, ItemEnum, ItemStruct};
use table::define_table;

mod dummy;
pub(crate) mod multi;
mod parse;
mod table;

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
/// For example:
/// ```
/// #[rust_query::migration::schema]
/// #[version(0..=0)]
/// enum Schema {
///     #[unique_email(email)]
///     #[unique_username(username)]
///     User {
///         email: String,
///         username: String,
///     },
/// }
/// # fn main() {}
/// ```
/// This will create a single schema with a single table called `user` and two columns.
/// The table will also have two unique contraints.
///
/// To define a unique constraint on a column, you need to add an attribute to the table.
/// The attribute needs to start with `unique` and can have any suffix.
/// Within a table, the different unique constraints must have different suffixes.
///
/// Optional types are not allowed in unique constraints.
///
/// ## Multiple versions
/// The macro uses enum syntax, but it generates multiple modules of types.
///
/// Note that the schema version range is `0..=0` so there is only a version 0.
/// The generated code will have a structure like this:
/// ```rust,ignore
/// mod v0 {
///     pub struct Schema;
///     pub struct User(..);
///     // a bunch of other stuff
/// }
/// ```
///
/// # Adding tables
/// At some point you might want to add a new table.
/// ```
/// #[rust_query::migration::schema]
/// #[version(0..=1)]
/// enum Schema {
///     #[unique_email(email)]
///     #[unique_username(username)]
///     User {
///         email: String,
///         username: String,
///     },
///     #[version(1..)] // <-- note that `Game`` has a version range
///     Game {
///         name: String,
///         size: i64,
///     }
/// }
/// # fn main() {}
/// ```
/// We now have two schema versions which generates two modules `v0` and `v1`.
/// They look something like this:
/// ```rust,ignore
/// mod v0 {
///     pub struct Schema;
///     pub struct User(..);
///     // a bunch of other stuff
/// }
/// mod v1 {
///     pub struct Schema;
///     pub struct User(..);
///     pub struct Game(..);
///     // a bunch of other stuff
/// }
/// ```
///
/// # Changing columns
/// Changing columns is very similar to adding and removing structs.
/// ```
/// use rust_query::migration::{schema, Config};
/// use rust_query::{IntoSelectExt, LocalClient, Database};
/// #[schema]
/// #[version(0..=1)]
/// enum Schema {
///     #[unique_email(email)]
///     #[unique_username(username)]
///     User {
///         email: String,
///         username: String,
///         #[version(1..)] // <-- here
///         score: i64,
///     },
/// }
/// // In this case it is required to provide a value for each row that already exists.
/// // This is done with the `v1::update::UserMigration`:
/// pub fn migrate(client: &mut LocalClient) -> Database<v1::Schema> {
///     let m = client.migrator(Config::open_in_memory()) // we use an in memory database for this test
///         .expect("database version is before supported versions");
///     let m = m.migrate(|_, _| v1::update::Schema {
///         user: Box::new(|user| v1::update::UserMigration {
///             score: user.email().map_select(|x| x.len() as i64) // use the email length as the new score
///         }),
///     });
///     m.finish().expect("database version is after supported versions")
/// }
/// # fn main() {}
/// ```
/// The `migrate` function first creates an empty database if it does not exists.
/// Then it migrates the database if necessary, where it initializes every user score to the length of their email.
#[proc_macro_attribute]
pub fn schema(
    attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    assert!(attr.is_empty());
    let item = syn::parse_macro_input!(item as ItemEnum);

    match generate(item) {
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
/// #[rust_query::migration::schema]
/// pub enum Schema {
///     Thing {
///         details: Details,
///         beta: f64,
///         seconds: i64,
///     },
///     Details {
///         name: String
///     },
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
/// pub fn do_query(db: &Transaction<Schema>) -> Vec<MyData> {
///     db.query(|rows| {
///         let thing = Thing::join(rows);
///
///         rows.into_vec(MyDataSelect {
///             seconds: thing.seconds(),
///             is_it_real: true,
///             name: thing.details().name(),
///             other: OtherDataSelect {
///                 alpha: 0.5,
///                 beta: thing.beta(),
///             },
///         })
///     })
/// }
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

/// This macro also supports some helper attributes on the struct.
///
/// - `#[rust_query(From = Thing)]`
///   This will automatically derive `FromExpr` for the specified column type.
/// - `#[rust_query(lt = 't)]`
///   Can be used to specify the transaction lifetime for structs that contain `TableRow` fields.
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

impl SingleVersionTable {
    pub fn migration_name(&self) -> Ident {
        let table_name = &self.name;
        format_ident!("{table_name}Migration")
    }
}

// prev_table is only used for the columns
fn define_table_migration(
    prev_columns: &BTreeMap<usize, SingleVersionColumn>,
    table: &SingleVersionTable,
) -> syn::Result<Option<TokenStream>> {
    let mut col_new = vec![];
    let mut col_str = vec![];
    let mut alter_ident = vec![];
    let mut alter_typ = vec![];

    for (i, col) in &table.columns {
        let name = &col.name;
        if prev_columns.contains_key(i) {
            col_new.push(quote! {prev.#name()});
        } else {
            let mut unique_columns = table.uniques.iter().flat_map(|u| &u.columns);
            if unique_columns.any(|c| c == name) {
                return Err(syn::Error::new_spanned(name, "It is not possible to modify unique constraints with a column migration.
Please re-create the table with the new unique constraints and use the migration transaction to copy over all the data."));
            }
            col_new.push(quote! {val.#name});

            alter_ident.push(name);
            alter_typ.push(&col.typ);
        }
        col_str.push(name.to_string());
    }

    // check that nothing was added or removed
    // we don't need input if only stuff was removed, but it still needs migrating
    if alter_ident.is_empty() && table.columns.len() == prev_columns.len() {
        return Ok(None);
    }

    if alter_ident.is_empty() {
        panic!("Migrations that only remove columns are not supported yet")
    }

    let table_name = &table.name;
    let migration_name = table.migration_name();
    let prev_typ = quote! {#table_name};

    let migration = quote! {
        pub struct #migration_name<'column, 't> {#(
            pub #alter_ident: ::rust_query::Select<'column, 't, _PrevSchema, <#alter_typ as ::rust_query::private::MyTyp>::Out<'t>>,
        )*}

        impl ::rust_query::private::EasyMigratable for super::#table_name {
            type From = #prev_typ;
            type Migration<'column, 't> = #migration_name<'column, 't>;

            fn prepare<'column, 't>(
                val: Self::Migration<'column, 't>,
                prev: ::rust_query::Expr<'column, _PrevSchema, Self::From>,
                cacher: &mut ::rust_query::private::CacheAndRead<'column, 't, _PrevSchema>,
            ) {#(
                cacher.col(#col_str, #col_new);
            )*}
        }
    };
    Ok(Some(migration))
}

fn define_table_creation(table: &SingleVersionTable) -> TokenStream {
    let mut col_str = vec![];
    let mut col_ident = vec![];
    let mut col_typ = vec![];

    for col in table.columns.values() {
        col_str.push(col.name.to_string());
        col_ident.push(&col.name);
        col_typ.push(&col.typ);
    }

    let table_name = &table.name;
    let (conflict_type, conflict_expr) = table.conflict(quote! {super::}, quote! {_PrevSchema});

    quote! {
        impl<'t> ::rust_query::private::TableCreation<'t> for super::#table_name<#(<#col_typ as ::rust_query::private::MyTyp>::Out<'t>),*> {
            type FromSchema = _PrevSchema;
            type Conflict = #conflict_type;
            type T = super::#table_name;

            fn read(&self, f: ::rust_query::private::Reader<'_, 't, Self::FromSchema>) {
                #(f.col(#col_str, &self.#col_ident);)*
            }
            fn get_conflict_unchecked(&self) -> ::rust_query::Select<'t, 't, Self::FromSchema, Option<Self::Conflict>> {
                #conflict_expr
            }
        }
    }
}

fn generate(item: ItemEnum) -> syn::Result<TokenStream> {
    let schema_name = item.ident.clone();
    let schema = VersionedSchema::parse(item)?;

    let mut output = TokenStream::new();
    let mut prev_tables: BTreeMap<usize, SingleVersionTable> = BTreeMap::new();
    let mut prev_mod = None;
    for version in schema.versions.clone() {
        let new_tables = schema.get(version)?;

        let mut mod_output = TokenStream::new();
        for table in new_tables.values() {
            mod_output.extend(define_table(table, &schema_name));
        }

        let mut schema_table_typs = vec![];
        let mut tables = vec![];
        let mut create_table_name = vec![];
        let mut create_table_lower = vec![];

        let mut table_migrations = TokenStream::new();

        // loop over all new table and see what changed
        for (i, table) in &new_tables {
            let table_name = &table.name;

            let table_lower = to_lower(table_name);

            schema_table_typs.push(quote! {b.table::<#table_name>()});

            if let Some(prev_table) = prev_tables.remove(i) {
                // a table already existed, so we need to define a migration

                let Some(migration) = define_table_migration(&prev_table.columns, table)? else {
                    continue;
                };
                table_migrations.extend(migration);

                table_migrations.extend(define_table_creation(table));
                create_table_lower.push(table_lower);
                create_table_name.push(table_name);
            } else if table.prev.is_some() {
                table_migrations.extend(define_table_creation(table));
                create_table_lower.push(table_lower);
                create_table_name.push(table_name);
            } else {
                tables.push(quote! {b.create_empty::<super::#table_name>()})
            }
        }
        for prev_table in prev_tables.into_values() {
            // a table was removed, so we drop it

            let table_ident = &prev_table.name;
            tables.push(quote! {b.drop_table::<super::super::#prev_mod::#table_ident>()})
        }

        let version_i64 = version as i64;
        mod_output.extend(quote! {
            pub struct #schema_name;
            impl ::rust_query::private::Schema for #schema_name {
                const VERSION: i64 = #version_i64;

                fn typs(b: &mut ::rust_query::private::TableTypBuilder<Self>) {
                    #(#schema_table_typs;)*
                }
            }
        });

        let new_mod = format_ident!("v{version}");

        let migrations = prev_mod.map(|prev_mod| {
            let prelude = prelude(&new_tables, &prev_mod, &schema_name);

            let lifetime = create_table_name.is_empty().not().then_some(quote! {'t,});
            let create_lt = create_table_name.is_empty().not().then_some(quote! {'x, 't,});
            quote! {
                pub mod update {
                    #prelude

                    #table_migrations

                    pub struct #schema_name<#lifetime> {
                        #(pub #create_table_lower: ::rust_query::migration::Migrate<'t, super::#create_table_name>,)*
                    }
                    pub struct Args<#create_lt> {
                        #(pub #create_table_lower: ::std::collections::BTreeMap<
                            ::rust_query::TableRow<'t, #create_table_name>,
                            ::rust_query::migration::MigrateRow<'x, 't, _PrevSchema, super::#create_table_name>
                        >,)*
                    }

                    impl<'t> ::rust_query::private::Migration<'t> for #schema_name<#lifetime> {
                        type From = _PrevSchema;
                        type To = super::#schema_name;
                        type Args<'x> = Args<#create_lt>;

                        fn tables(self, b: &mut ::rust_query::private::SchemaBuilder<'t>) {
                            #(#tables;)*
                            #(self.#create_table_lower.apply(b);)*
                        }

                        fn new_tables<'x>(_b: &mut ::rust_query::private::SchemaBuilder<'t>) -> Self::Args<'x> {
                            #(let #create_table_lower = _b.get_migrate_list::<#create_table_name, super::#create_table_name>();)*
                            Args {
                                #(#create_table_lower,)*
                            }
                        }
                    }
                }
            }
        });

        output.extend(quote! {
            mod #new_mod {
                #mod_output

                #migrations
            }
        });

        prev_tables = new_tables;
        prev_mod = Some(new_mod);
    }

    Ok(output)
}

fn prelude(
    new_tables: &BTreeMap<usize, SingleVersionTable>,
    prev_mod: &Ident,
    schema: &Ident,
) -> TokenStream {
    let mut prelude = vec![];
    for table in new_tables.values() {
        let Some(old_name) = &table.prev else {
            continue;
        };
        let new_name = &table.name;
        prelude.push(quote! {
            #old_name as #new_name
        });
    }
    prelude.push(quote! {#schema as _PrevSchema});
    let mut prelude = quote! {
        #[allow(unused_imports)]
        use super::super::#prev_mod::{#(#prelude,)*};
    };
    for table in new_tables.values() {
        if table.prev.is_none() {
            let new_name = &table.name;
            prelude.extend(quote! {
                #[allow(unused_imports)]
                use super::#new_name;
            })
        }
    }
    prelude
}
