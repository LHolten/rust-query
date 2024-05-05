use std::collections::HashMap;

use heck::{ToSnekCase, ToUpperCamelCase};
use proc_macro2::TokenStream;
use quote::{format_ident, quote, ToTokens};

use crate::{client::Client, pragma, value::Value};

#[derive(Clone)]
struct Column {
    name: String,
    typ: String,
    pk: bool,
    notnull: bool,
}

pub fn generate(client: Client) -> String {
    let mut output = TokenStream::new();
    output.extend(quote! {
        use rust_query::{value::{Db, Value}, Builder, HasId, Table, insert::{Reader, Writable}};
    });

    let tables = client.new_query(|q| {
        let table = q.flat_table(pragma::TableList);
        q.filter(table.schema.eq("main"));
        q.filter(table.r#type.eq("table"));
        q.filter(table.name.eq("sqlite_schema").not());
        q.into_vec(u32::MAX, |row| row.get(&table.name))
    });

    for table in &tables {
        let mut columns = client.new_query(|q| {
            let table = q.flat_table(pragma::TableInfo(table.to_owned()));

            q.into_vec(u32::MAX, |row| Column {
                name: row.get(table.name),
                typ: row.get(table.r#type),
                pk: row.get(table.pk) != 0,
                notnull: row.get(table.notnull) != 0,
            })
        });

        let fks: HashMap<_, _> = client
            .new_query(|q| {
                let fk = q.flat_table(pragma::ForeignKeyList(table.to_owned()));
                q.into_vec(u32::MAX, |row| {
                    // we just assume that the to column is the primary key..
                    (row.get(fk.from), row.get(fk.table))
                })
            })
            .into_iter()
            .collect();

        let mut ids = columns.iter().filter(|x| x.pk);
        let mut has_id = ids.next().cloned();
        if ids.next().is_some() {
            has_id = None;
        }

        let make_field = |name: &str| {
            let mut normalized = &*name.to_snek_case();
            if fks.contains_key(name) {
                normalized = normalized.trim_end_matches("_id");
            }
            format_ident!("{normalized}")
        };

        let make_generic = |name: &str| {
            let mut normalized = &*name.to_upper_camel_case();
            if fks.contains_key(name) {
                normalized = normalized.trim_end_matches("Id");
            }
            format_ident!("_{normalized}")
        };

        let make_type = |col: &Column| {
            let mut typ = match col.typ.as_str() {
                "INTEGER" => {
                    if let Some(other) = fks.get(&col.name) {
                        let other_ident = format_ident!("{}", other.to_upper_camel_case());
                        other_ident.to_token_stream()
                    } else {
                        quote!(i64)
                    }
                }
                "TEXT" => quote!(String),
                "REAL" => quote!(f64),
                _ => return None,
            };
            if !col.notnull {
                typ = quote!(Option<#typ>);
            }
            Some(typ)
        };

        // we only care about columns that are not a unique id and for which we know the type
        columns.retain(|col| {
            if has_id.is_some() && col.pk {
                return false;
            }
            if make_type(col).is_none() {
                return false;
            }
            true
        });

        let defs = columns.iter().map(|col| {
            let ident = make_field(&col.name);
            let generic = make_generic(&col.name);
            quote!(pub #ident: #generic)
        });

        let typs = columns.iter().map(|col| {
            let typ = make_type(col).unwrap();
            quote!(Db<'t, #typ>)
        });

        let generics = columns.iter().map(|col| {
            let generic = make_generic(&col.name);
            quote!(#generic)
        });

        let generics_defs = columns.iter().map(|col| {
            let generic = make_generic(&col.name);
            quote!(#generic)
        });

        let read_bounds = columns.iter().map(|col| {
            let typ = make_type(col).unwrap();
            let generic = make_generic(&col.name);
            quote!(#generic: Value<'t, Typ=#typ>)
        });

        let inits = columns.iter().map(|col| {
            let ident = make_field(&col.name);
            let name: &String = &col.name;
            quote!(#ident: f.col(#name))
        });

        let reads = columns.iter().map(|col| {
            let ident = make_field(&col.name);
            let name: &String = &col.name;
            quote!(f.col(#name, self.#ident))
        });

        let table_ident = format_ident!("{}", table.to_upper_camel_case());
        let dummy_ident = format_ident!("{}Dummy", table.to_upper_camel_case());

        let has_id = has_id.as_ref().map(|col| {
            let name: &String = &col.name;
            quote!(
                impl HasId for #table_ident {
                    const ID: &'static str = #name;
                    const NAME: &'static str = #table;
                }
            )
        });

        output.extend(quote! {
            pub struct #table_ident;

            pub struct #dummy_ident<#(#generics_defs),*> {
                #(#defs,)*
            }

            impl Table for #table_ident {
                type Dummy<'t> = #dummy_ident<#(#typs),*>;

                fn name(&self) -> String {
                    #table.to_owned()
                }

                fn build(f: Builder<'_>) -> Self::Dummy<'_> {
                    #dummy_ident {
                        #(#inits,)*
                    }
                }
            }

            impl<'t, #(#read_bounds),*> Writable<'t> for #dummy_ident<#(#generics),*> {
                type T = #table_ident;
                fn read(self, f: Reader<'t>) {
                    #(#reads;)*
                }
            }

            #has_id
        })
    }

    output.to_string()
}
