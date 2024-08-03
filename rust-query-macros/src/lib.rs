use std::collections::BTreeMap;

use from_row::from_row_impl;
use heck::{ToSnekCase, ToUpperCamelCase};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{punctuated::Punctuated, Attribute, Ident, ItemEnum, ItemStruct, Meta, Path, Token, Type};

mod table;
mod from_row;

/// You can use this macro to define your schema.
/// The macro uses enum syntax, but it generates multiple modules of types.
///
/// For example:
/// ```
/// #[rust_query::schema]
/// #[version(0..1)]
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
/// Note that the schema version range is `0..1` so there is only a version 0.
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
/// #[rust_query::schema]
/// #[version(0..2)]
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
/// We now have two schema version which generates two modules `v0` and `v1`.
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
/// #[rust_query::schema]
/// #[version(0..2)]
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
/// // This is done with the `v1::up::UserMigration`:
/// pub fn migrate() -> (rust_query::Client, v1::Schema) {
///     let m = rust_query::Prepare::open_in_memory(); // we use an in memory database for this test
///     let (mut m, s) = m.create_db_empty();
///     let s = m.migrate(s, |_s, db| v1::up::Schema {
///         user: Box::new(|user| v1::up::UserMigration {
///             score: db.get(user.email()).len() as i64
///         }),
///     });
///     (m.finish(), s.unwrap())
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

#[proc_macro_derive(FromRow)]
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
    end: Option<u32>,
}

impl Range {
    pub fn includes(&self, idx: u32) -> bool {
        if idx < self.start {
            return false;
        }
        if let Some(end) = self.end {
            return idx < end;
        }
        true
    }
}

impl syn::parse::Parse for Range {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let start: Option<syn::LitInt> = input.parse()?;
        let _: Token![..] = input.parse()?;
        let end: Option<syn::LitInt> = input.parse()?;

        let res = Range {
            start: start
                .map(|x| x.base10_parse().expect("version start is a decimal"))
                .unwrap_or_default(),
            end: end.map(|x| x.base10_parse().expect("version end is a decimal")),
        };
        Ok(res)
    }
}

fn parse_version(attrs: &[Attribute]) -> syn::Result<Range> {
    let mut version = None;
    for attr in attrs {
        if attr.path().is_ident("version") {
            assert!(version.is_none(), "there should be only one version");
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
    let mut generics = vec![];
    let mut constraints = vec![];
    let mut into_new = vec![];

    for (i, col) in &table.columns {
        let name = &col.name;
        let name_str = col.name.to_string();
        if prev_columns.contains_key(i) {
            into_new.push(quote! {reader.col(#name_str, prev.#name())});
        } else {
            let generic = make_generic(name);
            let typ = &col.typ;

            defs.push(quote! {pub #name: #generic});
            constraints.push(quote! {#generic: for<'x> ::rust_query::Value<'x, Typ = #typ>});
            generics.push(generic);
            into_new.push(quote! {reader.col(#name_str, self.#name.clone())});
        }
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
        pub struct #migration_name<#(#generics),*> {
            #(#defs,)*
        }

        impl<'a #(,#constraints)*> ::rust_query::private::TableMigration<'a, #prev_typ> for #migration_name<#(#generics),*> {
            type T = super::#table_name;

            fn into_new(self, prev: ::rust_query::Just<'a, #prev_typ>, reader: ::rust_query::private::Reader<'_>) {
                #(#into_new;)*
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
    for version in range.start..range.end.expect("schema has a final version") {
        let mut new_tables: BTreeMap<usize, Table> = BTreeMap::new();

        let mut mod_output = TokenStream::new();
        for (i, table) in item.variants.iter().enumerate() {
            let mut other_attrs = vec![];
            let mut uniques = vec![];
            let mut prev = None;
            for attr in &table.attrs {
                if let Some(unique) = is_unique(attr.path()) {
                    let idents = attr
                        .parse_args_with(Punctuated::<Ident, Token![,]>::parse_separated_nonempty)
                        .expect("unique arguments are comma separated");
                    uniques.push(Unique {
                        name: unique,
                        columns: idents.into_iter().collect(),
                    })
                } else if attr.path().is_ident("create_from") {
                    let new_prev = attr
                        .parse_args()
                        .expect("create_from is used with single ident");
                    let none = prev.replace(new_prev).is_none();
                    assert!(none, "can not define multiple `created_from`");
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
                let name = field.ident.clone().expect("table columns are named");
                let mut other_attrs = vec![];
                let mut unique = None;
                for attr in &field.attrs {
                    if let Some(unique_name) = is_unique(attr.path()) {
                        let Meta::Path(_) = &attr.meta else {
                            panic!("expected no arguments to field specific unique");
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

            mod_output.extend(table::define_table(&table, schema));
            new_tables.insert(i, table);
        }

        let mut schema_table_defs = vec![];
        let mut schema_table_inits = vec![];
        let mut schema_table_typs = vec![];

        let mut table_defs = vec![];
        let mut table_generics: Vec<Ident> = vec![];
        let mut table_constraints: Vec<TokenStream> = vec![];
        let mut tables = vec![];

        let mut table_migrations = TokenStream::new();

        // loop over all new table and see what changed
        for (i, table) in &new_tables {
            let table_name = &table.name;

            let table_lower = to_lower(table_name);

            schema_table_defs.push(quote! {pub #table_lower: #table_name});
            schema_table_inits.push(quote! {#table_lower: #table_name(())});
            schema_table_typs.push(quote! {b.table::<#table_name>()});

            let normalized = table_name.to_string().to_upper_camel_case();
            // let table_generic = format_ident!("_{normalized}Func");
            let table_generic_out = format_ident!("_{normalized}Out");

            if let Some(prev_table) = prev_tables.remove(i) {
                // a table already existed, so we need to define a migration

                let Some(migration) = define_table_migration(&prev_table.columns, table) else {
                    continue;
                };
                table_migrations.extend(migration);

                table_defs.push(quote! {
                    pub #table_lower: Box<dyn 't + FnMut(::rust_query::Just<'t, #table_name>) -> #table_generic_out>
                });
                tables.push(quote! {b.migrate_table(self.#table_lower)});

                // table_constraints.push(quote! {
                //     #table_generic: FnMut(::rust_query::Just<'t, #table_name>) -> #table_generic_out
                // });
                table_constraints.push(quote! {
                    #table_generic_out: ::rust_query::private::TableMigration<'t, #table_name, T = super::#table_name>
                });
                // table_generics.push(table_generic);
                table_generics.push(table_generic_out);
            } else if table.prev.is_some() {
                // no table existed, but the previous table is specified, make a filter migration

                let Some(migration) = define_table_migration(&BTreeMap::new(), table) else {
                    panic!("can not use `create_from` on an empty table");
                };
                table_migrations.extend(migration);

                table_defs.push(quote! {
                    pub #table_lower: Box<dyn 't + FnMut(::rust_query::Just<'t, #table_name>) -> Option<#table_generic_out>>
                });
                tables.push(quote! {b.create_from(self.#table_lower)});

                // table_constraints.push(quote! {
                //     #table_generic: FnMut(::rust_query::Just<'t, #table_name>) -> Option<#table_generic_out>
                // });
                table_constraints.push(quote! {
                    #table_generic_out: ::rust_query::private::TableMigration<'t, #table_name, T = super::#table_name>
                });
                // table_generics.push(table_generic);
                table_generics.push(table_generic_out);
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
            let prelude = prelude(&new_tables, &prev_mod);
            
            // FIXME: remove the lifetime `'t` if there are no table_defs
            quote! {
                pub mod up {
                    #prelude

                    #table_migrations

                    pub struct #schema<'t, #(#table_generics),*> {
                        #(#table_defs,)*
                    }

                    impl<'t #(,#table_constraints)*> ::rust_query::private::Migration<'t, super::super::#prev_mod::#schema> for #schema<'t, #(#table_generics),*> {
                        type S = super::#schema;

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

fn prelude(new_tables: &BTreeMap<usize, Table>, prev_mod: &Ident) -> TokenStream {
    let mut prelude = vec![];
    for table in new_tables.values() {
        let Some(old_name) = &table.prev else {
            continue;
        };
        let new_name = &table.name;
        prelude.push(quote! {
            #old_name as #new_name
        })
    }
    let mut prelude = quote! {
        #[allow(unused_imports)]
        use super::super::#prev_mod::{#(#prelude,)*};
    };
    for table in new_tables.values() {
        if table.prev.is_none() {
            let new_name = &table.name;
            prelude.extend(quote! {
                #[allow(unused_imports)]
                use ::rust_query::NoTable as #new_name;
            })
        }
    }
    prelude
}
