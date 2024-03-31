use std::collections::HashMap;

use heck::{ToSnekCase, ToUpperCamelCase};
use proc_macro2::TokenStream;
use quote::{format_ident, quote, ToTokens};

use crate::{
    new_query, pragma,
    value::{Const, Value},
};

pub fn generate() -> String {
    let mut output = TokenStream::new();
    output.extend(quote! {
        use rust_query::{value::Db, Builder, HasId, Table};
        use std::marker::PhantomData;
    });

    let tables = new_query(|q| {
        let table = q.flat_table(pragma::TableList);
        q.filter(table.schema.eq(Const("main".to_owned())));
        q.filter(table.r#type.eq(Const("table".to_owned())));
        q.filter(table.name.eq(Const("sqlite_schema".to_owned())).not());
        q.into_vec(u32::MAX, |row| row.get(q.select(&table.name)))
    });

    for table in &tables {
        let columns = new_query(|q| {
            let table = q.flat_table(pragma::TableInfo(table.to_owned()));

            q.into_vec(u32::MAX, |row| {
                let name = row.get(q.select(table.name));
                let typ = row.get(q.select(table.r#type));
                let pk = row.get(q.select(table.pk)) != 0;
                let notnull = row.get(q.select(table.notnull)) != 0;
                (name, typ, pk, notnull)
            })
        });

        let fks: HashMap<_, _> = new_query(|q| {
            let fk = q.flat_table(pragma::ForeignKeyList(table.to_owned()));
            q.into_vec(u32::MAX, |row| {
                (row.get(q.select(fk.from)), row.get(q.select(fk.table)))
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
                x if x.starts_with("NVARCHAR") => quote!(String),
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

        let inits = columns.iter().filter_map(|(name, typ, pk, _notnull)| {
            if has_id.is_some() && *pk {
                return None;
            }
            make_type(typ, name)?;
            let ident = make_field(name);
            Some(quote!(#ident: f.col(#name)))
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

        output.extend(quote! {
            pub struct #table_ident;

            pub struct #dummy_ident<'t> {
                _phantom: PhantomData<dyn Fn(&'t ()) -> &'t ()>,
                #(#defs,)*
            }

            impl Table for #table_ident {
                type Dummy<'t> = #dummy_ident<'t>;

                fn name(&self) -> String {
                    #table.to_owned()
                }

                fn build(f: Builder<'_>) -> Self::Dummy<'_> {
                    #dummy_ident {
                        _phantom: PhantomData,
                        #(#inits,)*
                    }
                }
            }

            #has_id
        })
    }

    output.to_string()
}
