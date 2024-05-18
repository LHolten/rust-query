
use std::collections::BTreeMap;

use heck::{ToSnekCase, ToUpperCamelCase};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Attribute, Ident, ItemEnum, Token, Type};

#[proc_macro_attribute]
pub fn schema(
    attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    assert!(attr.is_empty());
    let item = syn::parse_macro_input!(item as ItemEnum);

    match generate(item) {
        Ok(x) => {x},
        Err(e) => {e.into_compile_error()},
    }.into()
}

#[derive(Clone)]
struct Column {
    name: Ident,
    typ: Type,
}


struct Range {
    start: u32,
    end: Option<u32>
}

impl Range {
    pub fn includes(&self, idx: u32) -> bool {
        if idx < self.start {
            return false
        }
        if let Some(end) = self.end {
            return idx < end
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
            start: start.map(|x|x.base10_parse().unwrap()).unwrap_or_default(),
            end: end.map(|x|x.base10_parse().unwrap()),
        };
        Ok(res)
    }
}

fn parse_version(attrs: &[Attribute]) -> syn::Result<Range> {
    if attrs.is_empty() {
        return Ok(Range {start:0, end: None})
    }
    let [versions] = attrs else {
        panic!()
    };
    assert!(versions.path().is_ident("version"));
    versions.parse_args()
}

fn generate(item: ItemEnum) -> syn::Result<TokenStream> {
    let range = parse_version(&item.attrs)?;
    let schema = &item.ident ;
    
    let make_generic = |name: &Ident| {
        let normalized = name.to_string().to_upper_camel_case();
        format_ident!("_{normalized}")
    };
    let to_lower = |name: &Ident| {
        let normalized = name.to_string().to_snek_case();
        format_ident!("{normalized}")
    };
    
    let mut output = TokenStream::new();
    let mut prev_tables: BTreeMap<usize, (BTreeMap<usize, Column>, Ident)> = BTreeMap::new();
    for version in range.start..range.end.unwrap() {     
        let mut new_tables: BTreeMap<usize, (BTreeMap<usize, Column>, Ident)> = BTreeMap::new();
        
        let mut mod_output = TokenStream::new();
        for (i, table) in item.variants.iter().enumerate() {
            let range = parse_version(&table.attrs)?;
            if !range.includes(version) {
                continue;
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
        
            let table_ident = &table.ident;
            
            let defs = columns.values().map(|col| {
                let ident = &col.name;
                let generic = make_generic(&col.name);
                quote!(pub #ident: #generic)
            });
            
            let typs = columns.values().map(|col| {
                let typ = &col.typ;
                quote!(::rust_query::value::Db<'t, #typ>)
            });
        
            let typ_asserts = columns.values().map(|col| {
                let typ = &col.typ;
                quote!(::rust_query::valid_in_schema::<#schema, #typ>();)
            });
        
            let generics = columns.values().map(|col| {
                let generic = make_generic(&col.name);
                quote!(#generic)
            });
        
            let generics_defs = columns.values().map(|col| {
                let generic = make_generic(&col.name);
                quote!(#generic)
            });
        
            let read_bounds = columns.values().map(|col| {
                let typ = &col.typ;
                let generic = make_generic(&col.name);
                quote!(#generic: ::rust_query::value::Value<'t, Typ=#typ>)
            });
        
            let inits = columns.values().map(|col| {
                let ident = &col.name;
                let name: &String = &col.name.to_string();
                quote!(#ident: f.col(#name))
            });
        
            let reads = columns.values().map(|col| {
                let ident = &col.name;
                let name: &String = &col.name.to_string();
                quote!(f.col(#name, self.#ident))
            });

            let def_typs = columns.values().map(|col| {
                let name: &String = &col.name.to_string();
                let typ = &col.typ;
                quote!(f.col::<#typ>(#name))
            });
        
            let dummy_ident = format_ident!("{}Dummy", table_ident);
        
            let table_name: &String = &table_ident.to_string();
            let has_id = quote!(
                impl ::rust_query::HasId for #table_ident {
                    const ID: &'static str = "id";
                    const NAME: &'static str = #table_name;
                }
            );
        
            mod_output.extend(quote! {
                pub struct #table_ident;
        
                pub struct #dummy_ident<#(#generics_defs),*> {
                    #(#defs,)*
                }
        
                impl ::rust_query::Table for #table_ident {
                    type Dummy<'t> = #dummy_ident<#(#typs),*>;
                    type Schema = #schema;
        
                    fn name(&self) -> String {
                        #table_name.to_owned()
                    }
        
                    fn build(f: ::rust_query::Builder<'_>) -> Self::Dummy<'_> {
                        #dummy_ident {
                            #(#inits,)*
                        }
                    }

                    fn typs(f: &mut ::rust_query::TypBuilder) {
                        #(#def_typs;)*
                    }
                }
        
                impl<'t, #(#read_bounds),*> ::rust_query::insert::Writable<'t> for #dummy_ident<#(#generics),*> {
                    type T = #table_ident;
                    fn read(self: Box<Self>, f: ::rust_query::insert::Reader<'_, 't>) {
                        #(#reads;)*
                    }
                }
        
                const _: fn() = || {
                    #(#typ_asserts)*
                };
        
                #has_id

            });

            new_tables.insert(i, (columns, table.ident.clone()));
        }

        let mod_ident = format_ident!("v{version}");
        let mod_prev_ident = format_ident!("v{}", version.wrapping_sub(1));
        let prev_schema = if version == 0 {
            quote! {()}
        } else {
            quote! {super::#mod_prev_ident::#schema}
        };

        let mut table_defs = vec![];
        let mut table_generics: Vec<Ident> = vec![];
        let mut table_constraints: Vec<TokenStream> = vec![];
        let mut tables = vec![];
        for (i, (table, _)) in &new_tables {
            if let Some((prev_columns, table_name)) = prev_tables.get(i) {

                let mut defs = vec![];
                let mut generics = vec![];
                let mut constraints = vec![];
                let mut into_new = vec![];

                for (i, col) in table {
                    let name = &col.name;
                    let name_str = col.name.to_string();
                    if prev_columns.get(i).is_none() {
                        let generic = make_generic(name);
                        let typ = &col.typ;

                        defs.push(quote!{pub #name: #generic});
                        constraints.push(quote!{#generic: ::rust_query::value::Value<'a, Typ = #typ>});
                        generics.push(generic);
                        into_new.push(quote!{reader.col(#name_str, self.#name.clone())});
                    } else {
                        into_new.push(quote!{reader.col(#name_str, prev.#name.clone())});
                    }
                }

                if defs.is_empty() {
                    continue;
                }

                let prev_table_name = quote! {super::#mod_prev_ident::#table_name};

                let migration_name = format_ident!("M{table_name}");
                mod_output.extend(quote!{
                    pub struct #migration_name<#(#generics),*> {
                        #(#defs,)*
                    }

                    impl<'a, #(#constraints),*> ::rust_query::migrate::TableMigration<'a, #prev_table_name> for #migration_name<#(#generics)*> {
                        type T = #table_name;

                        fn into_new(self: Box<Self>, prev: ::rust_query::value::Db<'a, #prev_table_name>, reader: ::rust_query::insert::Reader<'_, 'a>) {
                            #(#into_new;)*
                        }
                    }
                });

                let table_generic = make_generic(table_name);
                let table_lower = to_lower(table_name);
                table_defs.push(quote! {
                    pub #table_lower: #table_generic
                });
                table_constraints.push(quote! {
                    #table_generic: for<'x, 'a> FnMut(::rust_query::Row<'x, 'a>, ::rust_query::value::Db<'a, #prev_table_name>) -> Box<dyn ::rust_query::migrate::TableMigration<'a, #prev_table_name, T = #table_name>>
                });
                table_generics.push(table_generic);
                tables.push(quote!{b.migrate_table(self.#table_lower)});
            }
        }
    
        let version_i64 = version as i64;
        output.extend(quote! {
            pub mod #mod_ident {
                pub struct #schema (());

                impl ::rust_query::migrate::Schema for #schema {
                    const VERSION: i64 = #version_i64;
                    fn new() -> Self {
                        #schema (())
                    }
                }

                pub struct M<#(#table_constraints),*> {
                    #(#table_defs,)*
                }

                impl<#(#table_constraints),*> ::rust_query::migrate::Migration<#prev_schema> for M<#(#table_generics),*> {
                    type S = #schema;
                    
                    fn tables(self, b: &mut ::rust_query::migrate::SchemaBuilder) -> Self::S {
                        #(#tables;)*
                        #schema (())
                    }
                }

                // impl ::rust_query::migrate::Schema for #schema {}

                #mod_output
            }
        });
    
        prev_tables = new_tables;
    }

    Ok(output)
}
