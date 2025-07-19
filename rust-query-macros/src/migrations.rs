use std::{collections::BTreeMap, ops::Not};

use crate::{
    multi::{SingleVersionColumn, SingleVersionTable},
    to_lower,
};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::Ident;

pub fn migrations(
    schema_name: &Ident,
    mut prev_tables: BTreeMap<usize, SingleVersionTable>,
    new_tables: &BTreeMap<usize, SingleVersionTable>,
    prev_mod: TokenStream,
    new_mod: TokenStream,
) -> Result<TokenStream, syn::Error> {
    let mut tables = vec![];
    let mut create_table_name = vec![];
    let mut create_table_lower = vec![];
    let mut table_migrations = TokenStream::new();
    // loop over all new table and see what changed
    for (i, table) in new_tables {
        let table_name = &table.name;

        let table_lower = to_lower(table_name);

        if let Some(prev_table) = prev_tables.remove(i) {
            // a table already existed, so we need to define a migration

            let Some(migration) =
                define_table_migration(&prev_table.columns, table, false, &new_mod)?
            else {
                continue;
            };
            table_migrations.extend(migration);

            create_table_lower.push(table_lower);
            create_table_name.push(table_name);

            tables.push(quote! {b.drop_table::<#prev_mod::#table_name>()})
        } else if table.prev.is_some() {
            let migration =
                define_table_migration(&BTreeMap::new(), table, true, &new_mod).unwrap();

            table_migrations.extend(migration);
            create_table_lower.push(table_lower);
            create_table_name.push(table_name);
        } else {
            tables.push(quote! {b.create_empty::<#new_mod::#table_name>()})
        }
    }
    for prev_table in prev_tables.into_values() {
        // a table was removed, so we drop it

        let table_ident = &prev_table.name;
        tables.push(quote! {b.drop_table::<#prev_mod::#table_ident>()})
    }

    let lifetime = create_table_name.is_empty().not().then_some(quote! {'t,});
    Ok(quote! {
        #table_migrations

        pub struct #schema_name<#lifetime> {
            #(pub #create_table_lower: ::rust_query::migration::Migrated<'t, #prev_mod::#schema_name, #new_mod::#create_table_name>,)*
        }

        impl<'t> ::rust_query::private::SchemaMigration<'t> for #schema_name<#lifetime> {
            type From = #prev_mod::#schema_name;
            type To = #new_mod::#schema_name;

            fn tables(self, b: &mut ::rust_query::private::SchemaBuilder<'t, Self::From>) {
                #(#tables;)*
                #(self.#create_table_lower.apply(b);)*
            }
        }
    })
}

// prev_table is only used for the columns
fn define_table_migration(
    prev_columns: &BTreeMap<usize, SingleVersionColumn>,
    table: &SingleVersionTable,
    always_migrate: bool,
    new_mod: &TokenStream,
) -> syn::Result<Option<TokenStream>> {
    let mut col_new = vec![];
    let mut col_ident = vec![];
    let mut alter_ident = vec![];
    let mut alter_typ = vec![];
    let mut alter_tmp = vec![];

    let mut migration_conflict = quote! {::std::convert::Infallible};
    let mut conflict_from = quote! {::std::unreachable!()};

    for (i, col) in &table.columns {
        let name = &col.name;
        if prev_columns.contains_key(i) {
            col_new.push(quote! {&prev.#name});
        } else {
            let mut unique_columns = table.uniques.iter().flat_map(|u| &u.columns);
            if unique_columns.any(|c| c == name) {
                migration_conflict = quote! {::rust_query::TableRow<'t, Self::From>};
                conflict_from = quote! {val};
            }
            col_new.push(quote! {val.#name});

            alter_ident.push(name);
            alter_typ.push(&col.typ);
            alter_tmp.push(format_ident!("Tmp{i}"))
        }
        col_ident.push(name);
    }

    // check that nothing was added or removed
    // we don't need input if only stuff was removed, but it still needs migrating
    if !always_migrate && alter_ident.is_empty() && table.columns.len() == prev_columns.len() {
        return Ok(None);
    }

    let table_ident = &table.name;
    let typs_mod = format_ident!("_{table_ident}");

    let migration = quote! {
        mod #typs_mod {
            use super::#new_mod::*;
            #(
                pub type #alter_tmp = <<#alter_typ as ::rust_query::private::MyTyp>::Prev as ::rust_query::private::MyTyp>::Out;
            )*
        }

        pub struct #table_ident {#(
            pub #alter_ident: #typs_mod::#alter_tmp,
        )*}

        impl<'t> ::rust_query::private::Migration<'t> for #table_ident {
            type To = #new_mod::#table_ident;
            type FromSchema = <Self::From as ::rust_query::Table>::Schema;
            type From = <Self::To as ::rust_query::Table>::MigrateFrom;
            type Conflict = #migration_conflict;

            fn prepare(
                val: Self,
                prev: ::rust_query::Expr<'t, Self::FromSchema, Self::From>,
            ) -> <Self::To as ::rust_query::Table>::Insert<'t> {
                #new_mod::#table_ident {#(
                    #col_ident: ::rust_query::Expr::_migrate::<Self::FromSchema>(#col_new),
                )*}
            }

            fn map_conflict(val: ::rust_query::TableRow<'t, Self::From>) -> Self::Conflict {
                #conflict_from
            }
        }
    };
    Ok(Some(migration))
}
