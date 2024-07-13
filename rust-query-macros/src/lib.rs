use std::collections::BTreeMap;

use heck::{ToSnekCase, ToUpperCamelCase};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{punctuated::Punctuated, Attribute, Ident, ItemEnum, Token, Type};

mod table;


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
            start: start.map(|x| x.base10_parse().unwrap()).unwrap_or_default(),
            end: end.map(|x| x.base10_parse().unwrap()),
        };
        Ok(res)
    }
}

fn parse_version(attrs: &[Attribute]) -> syn::Result<Range> {
    if attrs.is_empty() {
        return Ok(Range {
            start: 0,
            end: None,
        });
    }
    let [versions] = attrs else {
        panic!("got unexpected attribute")
    };
    assert!(versions.path().is_ident("version"));
    versions.parse_args()
}

fn make_generic(name: &Ident) -> Ident {
    let normalized = name.to_string().to_upper_camel_case();
    format_ident!("_{normalized}")
}

fn to_lower(name: &Ident) -> Ident {
    let normalized = name.to_string().to_snek_case();
    format_ident!("{normalized}")
}

#[derive(Clone)]
struct Table {
    uniques: Vec<TokenStream>,
    prev: Option<Ident>,
    name: Ident,
    columns: BTreeMap<usize, Column>,
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
            into_new.push(quote! {reader.col(#name_str, prev.#name.clone())});
        } else {
            let generic = make_generic(name);
            let typ = &col.typ;

            defs.push(quote! {pub #name: #generic});
            constraints.push(quote! {#generic: ::rust_query::Value<'a, Typ = #typ>});
            generics.push(generic);
            into_new.push(quote! {reader.col(#name_str, self.#name.clone())});
        }
    }

    if defs.is_empty() {
        return None;
    }

    let table_name = &table.name;
    let migration_name = format_ident!("{table_name}Migration");
    let prev_typ = quote! {#table_name};

    Some(quote! {
        pub struct #migration_name<#(#generics),*> {
            #(#defs,)*
        }

        impl<'a, #(#constraints),*> ::rust_query::private::TableMigration<'a, #prev_typ> for #migration_name<#(#generics),*> {
            type T = super::#table_name;

            fn into_new(self: Box<Self>, prev: ::rust_query::Db<'a, #prev_typ>, reader: ::rust_query::private::Reader<'_, 'a>) {
                #(#into_new;)*
            }
        }
    })
}

fn generate(item: ItemEnum) -> syn::Result<TokenStream> {
    let range = parse_version(&item.attrs)?;
    let schema = &item.ident;

    let mut output = TokenStream::new();
    let mut prev_tables: BTreeMap<usize, Table> = BTreeMap::new();
    for version in range.start..range.end.unwrap() {
        let mut new_tables: BTreeMap<usize, Table> = BTreeMap::new();
        let prev_mod = format_ident!("v{}", version.saturating_sub(1));

        let mut mod_output = TokenStream::new();
        for (i, table) in item.variants.iter().enumerate() {
            let mut other_attrs = vec![];
            let mut uniques = vec![];
            let mut prev = None;
            for attr in &table.attrs {
                if attr.path().is_ident("unique") {
                    let idents = attr
                        .parse_args_with(Punctuated::<Ident, Token![,]>::parse_separated_nonempty)
                        .unwrap();
                    let idents = idents.into_iter().map(|x| x.to_string());
                    uniques.push(quote! {f.unique(&[#(#idents),*])});
                } else if attr.path().is_ident("create_from") {
                    let none = prev.replace(attr.parse_args().unwrap()).is_none();
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
                let range = parse_version(&field.attrs)?;
                if !range.includes(version) {
                    continue;
                }
                let col = Column {
                    name: field.ident.clone().unwrap(),
                    typ: field.ty.clone(),
                };
                columns.insert(i, col);
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

            if let Some(prev_table) = prev_tables.remove(i) {
                // a table already existed, so we need to define a migration

                let Some(migration) = define_table_migration(&prev_table.columns, table) else {
                    continue;
                };
                table_migrations.extend(migration);
                let table_generic = make_generic(table_name);
                table_defs.push(quote! {
                    pub #table_lower: #table_generic
                });
                table_constraints.push(quote! {
                    #table_generic: for<'x, 'a> FnMut(::rust_query::args::Row<'x, 'a>, ::rust_query::Db<'a, #table_name>) ->
                        Box<dyn ::rust_query::private::TableMigration<'a, #table_name, T = super::#table_name> + 'a>
                });
                table_generics.push(table_generic);
                tables.push(quote! {b.migrate_table(self.#table_lower)});
            } else if table.prev.is_some() {
                // no table existed, but the previous table is specified, make a filter migration

                let Some(migration) = define_table_migration(&BTreeMap::new(), table) else {
                    panic!("can not use `create_from` on an empty table");
                };
                table_migrations.extend(migration);
                let table_generic = make_generic(table_name);
                table_defs.push(quote! {
                    pub #table_lower: #table_generic
                });
                table_constraints.push(quote! {
                    #table_generic: for<'x, 'a> FnMut(::rust_query::args::Row<'x, 'a>, ::rust_query::Db<'a, #table_name>) ->
                        Option<Box<dyn ::rust_query::private::TableMigration<'a, #table_name, T = super::#table_name> + 'a>>
                });
                table_generics.push(table_generic);
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

        let prelude = prelude(&new_tables, &prev_mod);
        let new_mod = format_ident!("v{version}");
            output.extend(quote! {
            pub mod #new_mod {
                #mod_output
    
                pub mod up {
                    #prelude

                    #table_migrations
    
                    pub struct #schema<#(#table_constraints),*> {
                            #(#table_defs,)*
                        }
        
                    impl<#(#table_constraints),*> ::rust_query::private::Migration<super::super::#prev_mod::#schema> for #schema<#(#table_generics),*> {
                        type S = super::#schema;
        
                            fn tables(self, b: &mut ::rust_query::private::SchemaBuilder) {
                                #(#tables;)*
                            }
                        }
                    }
                }
            });

        prev_tables = new_tables;
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
