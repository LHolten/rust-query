use std::collections::HashMap;

use heck::{ToSnekCase, ToUpperCamelCase};
use proc_macro2::TokenStream;
use quote::{format_ident, quote, ToTokens};

use crate::{client::Client, pragma, value::Value};

pub fn generate(client: Client) -> String {
    let mut output = TokenStream::new();
    output.extend(quote! {
        use rust_query::{value::Db, Builder, HasId, Table, insert::{Reader, Writable}};
    });

    let tables = client.new_query(|q| {
        let table = q.flat_table(pragma::TableList);
        q.filter(table.schema.eq("main"));
        q.filter(table.r#type.eq("table"));
        q.filter(table.name.eq("sqlite_schema").not());
        q.into_vec(u32::MAX, |row| row.get(&table.name))
    });

    for table in &tables {
        let columns = client.new_query(|q| {
            let table = q.flat_table(pragma::TableInfo(table.to_owned()));

            q.into_vec(u32::MAX, |row| {
                let name = row.get(table.name);
                let typ = row.get(table.r#type);
                let pk = row.get(table.pk) != 0;
                let notnull = row.get(table.notnull) != 0;
                (name, typ, pk, notnull)
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

        let mut ids = columns.iter().filter(|x| x.2);
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

        let make_type = |typ: &str, name: &str| {
            Some(match typ {
                "INTEGER" => {
                    if let Some(other) = fks.get(name) {
                        let other_ident = format_ident!("{}", other.to_upper_camel_case());
                        other_ident.to_token_stream()
                    } else {
                        quote!(i64)
                    }
                }
                "TEXT" => quote!(String),
                "REAL" => quote!(f64),
                _ => return None,
            })
        };

        let defs = columns.iter().filter_map(|(name, typ, pk, notnull)| {
            if has_id.is_some() && *pk {
                return None;
            }
            let mut typ = make_type(typ, name)?;
            if !notnull {
                typ = quote!(Option<#typ>);
            }
            let ident = make_field(name);
            Some(quote!(pub #ident: Db<'t, #typ>))
        });
        let mut defs = defs.peekable();

        let inits = columns.iter().filter_map(|(name, typ, pk, _notnull)| {
            if has_id.is_some() && *pk {
                return None;
            }
            make_type(typ, name)?;
            let ident = make_field(name);
            Some(quote!(#ident: f.col(#name)))
        });

        let reads = columns.iter().flat_map(|(name, _typ, pk, _notnull)| {
            if has_id.is_some() && *pk {
                return None;
            }
            let ident = make_field(name);
            Some(quote!(f.col(#name, self.#ident)))
        });

        let table_ident = format_ident!("{}", table.to_upper_camel_case());
        let dummy_ident = format_ident!("{}Dummy", table.to_upper_camel_case());

        let has_id = has_id.as_ref().map(|(name, _typ, _pk, _notnull)| {
            quote!(
                impl HasId for #table_ident {
                    const ID: &'static str = #name;
                    const NAME: &'static str = #table;
                }
            )
        });

        let lifetime_def = defs.peek().map(|_| quote!(<'t>));
        output.extend(quote! {
            pub struct #table_ident;

            pub struct #dummy_ident #lifetime_def {
                #(#defs,)*
            }

            impl Table for #table_ident {
                type Dummy<'t> = #dummy_ident #lifetime_def;

                fn name(&self) -> String {
                    #table.to_owned()
                }

                fn build(f: Builder<'_>) -> Self::Dummy<'_> {
                    #dummy_ident {
                        #(#inits,)*
                    }
                }
            }

            impl<'a> Writable<'a> for #dummy_ident<'a> {
                type T = #table_ident;
                fn read(self, f: Reader<'a>) {
                    #(#reads;)*
                }
            }

            #has_id
        })
    }

    output.to_string()
}
