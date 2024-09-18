use std::{collections::BTreeMap, ops::Not};

use from_row::from_row_impl;
use heck::{ToSnekCase, ToUpperCamelCase};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    punctuated::Punctuated, Attribute, Ident, ItemEnum, ItemStruct, Meta, Path, Token, Type,
};

mod from_row;
mod table;

/// Use this macro to define your schema.
///
/// ## Supported data types:
/// - `i64` (sqlite `integer`)
/// - `f64` (sqlite `real`)
/// - `String` (sqlite `text`)
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
///     User {
///         #[unique_email]
///         email: String,
///         #[unique_username]
///         username: String,
///     }
/// }
/// # fn main() {}
/// ```
/// This will create a single schema with a single table called `user` and two columns.
/// The table will also have two unique contraints.
///
/// ## Multiple versions
/// The macro uses enum syntax, but it generates multiple modules of types.
///
/// Note that the schema version range is `0..=0` so there is only a version 0.
/// The generated code will have a structure like this:
/// ```rust,ignore
/// mod v0 {
///     struct User(..);
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
///     User {
///         #[unique_email]
///         email: String,
///         #[unique_username]
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
///     struct User(..);
///     // a bunch of other stuff
/// }
/// mod v1 {
///     struct User(..);
///     struct Game(..);
///     // a bunch of other stuff
/// }
/// ```
///
/// # Changing columns
/// Changing columns is very similar to adding and removing structs.
/// ```
/// #[rust_query::migration::schema]
/// #[version(0..=1)]
/// enum Schema {
///     User {
///         #[unique_email]
///         email: String,
///         #[unique_username]
///         username: String,
///         #[version(1..)] // <-- here
///         score: i64,
///     },
/// }
/// // In this case it is required to provide a value for each row that already exists.
/// // This is done with the `v1::update::UserMigration`:
/// pub fn migrate(t: &mut rust_query::ThreadToken) -> rust_query::Database<v1::Schema> {
///     let m = rust_query::migration::Prepare::open_in_memory(); // we use an in memory database for this test
///     let m = m.create_db_empty().expect("database is version is before supported versions");
///     let m = m.migrate(t, |db| v1::update::Schema {
///         user: Box::new(|user| v1::update::UserMigration {
///             score: db.query_one(user.email()).len() as i64 // use the email length as the new score
///         }),
///     });
///     m.finish(t).expect("database version is after supported versions")
/// }
/// # fn main() {}
/// ```
/// The `migrate` function first creates an empty database if it does not exists.
/// Then it migrates the database if necessary, where it initializes every user score to the length of their email.
///
/// # Other features
/// You can delete columns and tables by specifying the version range end.
/// ```rust,ignore
/// #[version(..3)]
/// ```
/// You can make a multi column unique constraint by specifying it before the table.
/// ```rust,ignore
/// #[unique(user, game)]
/// UserGameStats {
///     user: User,
///     game: Game,
///     score: i64,
/// }
/// ```
/// You can create a table from another table with the `create_from` attribute
/// ```rust,ignore
/// #[version(1..)]
/// #[create_from(user)]
/// UserAlias {
///     user: User,
///     name: String,
/// }
/// ```
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

/// Derive [FromDummy] to create a new `*Dummy` struct.
///
/// This `*Dummy` struct can then be used with [Query::into_vec] or [Transaction::query_one].
/// Usage can also be nested.
///
/// Example:
/// ```
/// #[derive(FromDummy)]
/// struct MyData {
///     seconds: i64,
///     is_it_real: bool,
///     name: String,
///     other: OtherData
/// }
///
/// #[derive(FromDummy)]
/// struct OtherData {
///     alpha: f64,
///     beta: f64,
/// }
///
/// db.query(|rows| {
///     let thing = Thing::join(rows);
///
///     rows.into_vec(MyDataDummy {
///         seconds: thing.seconds(),
///         is_it_real: true,
///         name: thing.details().name(),
///         other: OtherDataDummy {
///             alpha: 0.5,
///             beta: thing.beta(),
///         },
///     })
/// })
/// ```
#[proc_macro_derive(FromDummy)]
pub fn from_row(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let item = syn::parse_macro_input!(item as ItemStruct);
    match from_row_impl(item) {
        Ok(x) => x,
        Err(e) => e.into_compile_error(),
    }
    .into()
}

#[derive(Clone)]
struct Table {
    uniques: Vec<Unique>,
    prev: Option<Ident>,
    name: Ident,
    columns: BTreeMap<usize, Column>,
}

#[derive(Clone)]
struct Unique {
    name: Ident,
    columns: Vec<Ident>,
}

#[derive(Clone)]
struct Column {
    name: Ident,
    typ: Type,
}

#[derive(Clone)]
struct Range {
    start: u32,
    end: Option<RangeEnd>,
}

#[derive(Clone)]
struct RangeEnd {
    inclusive: bool,
    num: u32,
}

impl RangeEnd {
    pub fn end_exclusive(&self) -> u32 {
        match self.inclusive {
            true => self.num + 1,
            false => self.num,
        }
    }
}

impl Range {
    pub fn includes(&self, idx: u32) -> bool {
        if idx < self.start {
            return false;
        }
        if let Some(end) = &self.end {
            return idx < end.end_exclusive();
        }
        true
    }
}

impl syn::parse::Parse for Range {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let start: Option<syn::LitInt> = input.parse()?;
        let _: Token![..] = input.parse()?;
        let end: Option<RangeEnd> = input.is_empty().not().then(|| input.parse()).transpose()?;

        let res = Range {
            start: start
                .map(|x| x.base10_parse())
                .transpose()?
                .unwrap_or_default(),
            end,
        };
        Ok(res)
    }
}

impl syn::parse::Parse for RangeEnd {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let equals: Option<Token![=]> = input.parse()?;
        let end: syn::LitInt = input.parse()?;

        let res = RangeEnd {
            inclusive: equals.is_some(),
            num: end.base10_parse()?,
        };
        Ok(res)
    }
}

fn parse_version(attrs: &[Attribute]) -> syn::Result<Range> {
    let mut version = None;
    for attr in attrs {
        if attr.path().is_ident("version") {
            if version.is_some() {
                return Err(syn::Error::new_spanned(
                    attr,
                    "there should be only one version",
                ));
            }
            version = Some(attr.parse_args()?);
        } else {
            return Err(syn::Error::new_spanned(attr, "unexpected attribute"));
        }
    }
    Ok(version.unwrap_or(Range {
        start: 0,
        end: None,
    }))
}

fn make_generic(name: &Ident) -> Ident {
    let normalized = name.to_string().to_upper_camel_case();
    format_ident!("_{normalized}")
}

fn to_lower(name: &Ident) -> Ident {
    let normalized = name.to_string().to_snek_case();
    format_ident!("{normalized}")
}

// prev_table is only used for the columns
fn define_table_migration(
    prev_columns: &BTreeMap<usize, Column>,
    table: &Table,
) -> Option<TokenStream> {
    let mut defs = vec![];
    let mut into_new = vec![];
    let mut into_tmp = vec![];
    let mut tmp_defs = vec![];

    for (i, col) in &table.columns {
        let name = &col.name;
        let name_str = col.name.to_string();
        let typ = &col.typ;
        if prev_columns.contains_key(i) {
            into_tmp.push(quote! {#name: prev.#name()});
        } else {
            defs.push(quote! {pub #name: ::rust_query::DynValue<'t, _PrevSchema, #typ>});
            into_tmp.push(quote! {#name: migrated.#name});
        }
        tmp_defs.push(quote! {#name: ::rust_query::DynValue<'y, _PrevSchema, #typ>});
        into_new.push(quote! {reader.col::<_PrevSchema>(#name_str, self.#name)});
    }

    // check that nothing was added or removed
    // we don't need input if only stuff was removed, but it still needs migrating
    if defs.is_empty() && table.columns.len() == prev_columns.len() {
        return None;
    }

    let table_name = &table.name;
    let migration_name = format_ident!("{table_name}Migration");
    let prev_typ = quote! {#table_name};

    Some(quote! {
        pub struct #migration_name<'t> {
            #(#defs,)*
        }

        impl<'t> ::rust_query::private::TableMigration<'t, #prev_typ> for
            Box<dyn for<'a> FnOnce(::rust_query::DynValue<'a, _PrevSchema, #prev_typ>) -> #migration_name<'a>>
        {
            type T = super::#table_name;
            fn into_new(self, prev: ::rust_query::DynValue<'t, _PrevSchema, #prev_typ>)
                -> impl ::rust_query::private::Writable<'t, T = Self::T>
            {
                let migrated = (self)(prev.clone());
                struct _Temp<'y> {
                    #(#tmp_defs,)*
                }
                impl<'y> ::rust_query::private::Writable<'y> for _Temp<'y> {
                    type T = super::#table_name;

                    fn read(self, reader: ::rust_query::private::Reader<'y, super::Schema>) {
                        #(#into_new;)*
                    }
                }
                _Temp {
                    #(#into_tmp,)*
                }
            }
        }
    })
}

fn is_unique(path: &Path) -> Option<Ident> {
    path.get_ident().and_then(|ident| {
        ident
            .to_string()
            .starts_with("unique")
            .then(|| ident.clone())
    })
}

fn generate(item: ItemEnum) -> syn::Result<TokenStream> {
    let range = parse_version(&item.attrs)?;
    let schema = &item.ident;

    let mut output = TokenStream::new();
    let mut prev_tables: BTreeMap<usize, Table> = BTreeMap::new();
    let mut prev_mod = None;
    for version in range.start..range.end.map(|x| x.end_exclusive()).unwrap_or(1) {
        let mut new_tables: BTreeMap<usize, Table> = BTreeMap::new();

        let mut mod_output = TokenStream::new();
        for (i, table) in item.variants.iter().enumerate() {
            let mut other_attrs = vec![];
            let mut uniques = vec![];
            let mut prev = None;
            for attr in &table.attrs {
                if let Some(unique) = is_unique(attr.path()) {
                    let idents = attr.parse_args_with(
                        Punctuated::<Ident, Token![,]>::parse_separated_nonempty,
                    )?;
                    uniques.push(Unique {
                        name: unique,
                        columns: idents.into_iter().collect(),
                    })
                } else if attr.path().is_ident("create_from") {
                    let new_prev = attr.parse_args()?;
                    if prev.is_some() {
                        return Err(syn::Error::new_spanned(
                            attr,
                            "expected at most one `created_from`",
                        ));
                    }
                    prev = new_prev;
                } else {
                    other_attrs.push(attr.clone());
                }
            }

            let range = parse_version(&other_attrs)?;
            if !range.includes(version) {
                continue;
            }
            // if this is not the first schema version where this table exists
            if version != range.start {
                // the previous name of this table is the current name
                prev = Some(table.ident.clone());
            }

            let mut columns = BTreeMap::new();
            for (i, field) in table.fields.iter().enumerate() {
                let Some(name) = field.ident.clone() else {
                    return Err(syn::Error::new_spanned(
                        field,
                        "expected table columns to be named",
                    ));
                };
                let mut other_attrs = vec![];
                let mut unique = None;
                for attr in &field.attrs {
                    if let Some(unique_name) = is_unique(attr.path()) {
                        let Meta::Path(_) = &attr.meta else {
                            return Err(syn::Error::new_spanned(
                                attr,
                                "expected no arguments to field specific unique",
                            ));
                        };
                        unique = Some(Unique {
                            name: unique_name,
                            columns: vec![name.clone()],
                        })
                    } else {
                        other_attrs.push(attr.clone());
                    }
                }
                let range = parse_version(&other_attrs)?;
                if !range.includes(version) {
                    continue;
                }
                let col = Column {
                    name,
                    typ: field.ty.clone(),
                };
                columns.insert(i, col);
                uniques.extend(unique);
            }

            let table = Table {
                prev,
                name: table.ident.clone(),
                columns,
                uniques,
            };

            mod_output.extend(table::define_table(&table, schema)?);
            new_tables.insert(i, table);
        }

        let mut schema_table_defs = vec![];
        let mut schema_table_inits = vec![];
        let mut schema_table_typs = vec![];

        let mut table_defs = vec![];
        let mut tables = vec![];

        let mut table_migrations = TokenStream::new();

        // loop over all new table and see what changed
        for (i, table) in &new_tables {
            let table_name = &table.name;
            let migration_name = format_ident!("{table_name}Migration");

            let table_lower = to_lower(table_name);

            schema_table_defs.push(quote! {pub #table_lower: #table_name});
            schema_table_inits.push(quote! {#table_lower: #table_name(())});
            schema_table_typs.push(quote! {b.table::<#table_name>()});

            if let Some(prev_table) = prev_tables.remove(i) {
                // a table already existed, so we need to define a migration

                let Some(migration) = define_table_migration(&prev_table.columns, table) else {
                    continue;
                };
                table_migrations.extend(migration);

                table_defs.push(quote! {
                    pub #table_lower: Box<dyn 't + for<'a> FnOnce(::rust_query::DynValue<'a, _PrevSchema, #table_name>) -> #migration_name<'a>>
                });
                tables.push(quote! {b.migrate_table(self.#table_lower)});
            } else if table.prev.is_some() {
                // no table existed, but the previous table is specified, make a filter migration

                let Some(migration) = define_table_migration(&BTreeMap::new(), table) else {
                    return Err(syn::Error::new_spanned(
                        &table.name,
                        "can not use `create_from` on an empty table",
                    ));
                };
                table_migrations.extend(migration);

                table_defs.push(quote! {
                    pub #table_lower: Box<dyn 't + for<'a> FnOnce(::rust_query::DynValue<'a, _PrevSchema, #table_name>) -> #migration_name<'a>>
                });
                tables.push(quote! {b.create_from(self.#table_lower)});
            } else {
                // this is a new table

                tables.push(quote! {b.new_table::<super::#table_name>()})
            }
        }
        for prev_table in prev_tables.into_values() {
            // a table was removed, so we drop it

            let table_ident = &prev_table.name;
            tables.push(quote! {b.drop_table::<super::super::#prev_mod::#table_ident>()})
        }

        let version_i64 = version as i64;
        mod_output.extend(quote! {
            pub struct #schema {
                #(#schema_table_defs,)*
            }

            impl ::rust_query::private::Schema for #schema {
                const VERSION: i64 = #version_i64;
                fn new() -> Self {
                    #schema {
                        #(#schema_table_inits,)*
                    }
                }

                fn typs(b: &mut ::rust_query::private::TableTypBuilder) {
                    #(#schema_table_typs;)*
                }
            }

            pub fn assert_hash(expect: ::rust_query::private::Expect) {
                expect.assert_eq(&::rust_query::private::hash_schema::<#schema>())
            }
        });

        let new_mod = format_ident!("v{version}");

        let migrations = prev_mod.map(|prev_mod| {
            let prelude = prelude(&new_tables, &prev_mod, schema);

            let lifetime = table_defs.is_empty().not().then_some(quote! {'t,});
            quote! {
                pub mod update {
                    #prelude

                    #table_migrations

                    pub struct #schema<#lifetime> {
                        #(#table_defs,)*
                    }

                    impl<'t> ::rust_query::private::Migration<'t> for #schema<#lifetime> {
                        type From = _PrevSchema;
                        type To = super::#schema;

                        fn tables(self, b: &mut ::rust_query::private::SchemaBuilder<'t>) {
                            #(#tables;)*
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

fn prelude(new_tables: &BTreeMap<usize, Table>, prev_mod: &Ident, schema: &Ident) -> TokenStream {
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
                use ::rust_query::migration::NoTable as #new_name;
            })
        }
    }
    prelude
}
