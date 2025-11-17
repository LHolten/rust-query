use std::collections::BTreeMap;

use crate::{dummy::wrap, SingleVersionTable};

use super::make_generic;
use heck::ToSnekCase;
use quote::{format_ident, quote};

use proc_macro2::{Span, TokenStream};

use syn::{spanned::Spanned, Ident};

pub fn define_all_tables(
    schema_name: &Ident,
    prev_mod: &Option<Ident>,
    next_mod: &Option<Ident>,
    version: u32,
    new_tables: &mut BTreeMap<usize, SingleVersionTable>,
) -> syn::Result<TokenStream> {
    let local_file = schema_name.span().local_file();
    let source = &local_file
        .map(|path| std::fs::read_to_string(path).unwrap())
        .unwrap_or_default();

    let mut mod_output = TokenStream::new();
    let mut schema_table_typs = vec![];
    for table in new_tables.values_mut() {
        let table_def = define_table(
            source,
            table,
            schema_name,
            prev_mod.as_ref(),
            next_mod.as_ref(),
        )?;
        mod_output.extend(table_def);

        let table_name = &table.name;
        schema_table_typs.push(quote! {b.table::<#table_name>()});
    }

    // unwrap_or_default is used here because rust-analyzer sometimes doesn't give us the path
    let file = schema_name.span().local_file().unwrap_or_default();
    let file = file.file_name().unwrap_or_default().to_str().unwrap();
    let span = byte_range(source, schema_name.span());

    let version_i64 = version as i64;
    mod_output.extend(quote! {
        pub struct #schema_name;
        impl ::rust_query::private::Schema for #schema_name {
            const VERSION: i64 = #version_i64;
            const SOURCE: &str = include_str!(#file);
            const PATH: &str = file!();
            const SPAN: (usize, usize) = #span;

            fn typs(b: &mut ::rust_query::private::TableTypBuilder<Self>) {
                #(#schema_table_typs;)*
            }
        }
    });
    Ok(mod_output)
}

fn byte_from(source: &str, line: usize, col: usize) -> usize {
    source
        .lines()
        .skip(line - 1)
        .next()
        .unwrap()
        .as_ptr()
        .addr()
        - source.as_ptr().addr()
        + col
}

fn byte_range(source: &str, span: Span) -> TokenStream {
    if source.is_empty() {
        return quote! {(0, 0)};
    };
    let start = byte_from(source, span.start().line, span.start().column);
    let end = byte_from(source, span.end().line, span.end().column);
    quote! {(#start, #end)}
}

fn define_table(
    source: &str,
    table: &mut SingleVersionTable,
    schema: &Ident,
    prev_mod: Option<&Ident>,
    next_mod: Option<&Ident>,
) -> syn::Result<TokenStream> {
    let table_ident_with_span = table.name.clone();
    table.name.set_span(Span::call_site());
    let table_ident = &table.name;
    let table_name: &String = &table_ident.to_string().to_snek_case();
    let table_helper = format_ident!("{table_ident}Index");
    let table_span = byte_range(source, table_ident_with_span.span());

    let unique_tree = table.make_unique_tree();
    let unique_info = table.make_info(schema.clone());
    let unique_helpers =
        crate::unique::unique_tree(&table_helper, false, &unique_tree, &unique_info)?;

    let mut unique_typs = vec![];
    for index in &table.indices {
        let mut col_str = vec![];
        for col in &index.columns {
            col_str.push(col.to_string());
        }
        let is_unique = index.unique;
        let index_span = byte_range(source, index.span);
        unique_typs.push(quote! {f.index(&[#(#col_str),*], #is_unique, #index_span)});
    }

    let (conflict_type, conflict_dummy_insert) = table.conflict();

    let mut def_typs = vec![];
    let mut update_columns_safe = vec![];
    let mut generic = vec![];
    let mut try_from_update = vec![];
    let mut col_str = vec![];
    let mut col_ident = vec![];
    let mut col_doc = vec![];
    let mut col_typ = vec![];
    let mut col_span = vec![];
    let mut col_typ_original = vec![];
    let mut empty = vec![];
    let mut parts = vec![];

    for (i, col) in &table.columns {
        let ident = &col.name;
        let tmp = format_ident!("_{table_ident}{i}", span = col.typ.span());

        let mut unique_columns = table.indices.iter().flat_map(|u| &u.columns);
        if unique_columns.any(|x| x == ident) {
            def_typs.push(quote!(f.check_unique_compatible::<#tmp>()));
            update_columns_safe.push(quote! {::rust_query::private::Ignore});
            try_from_update.push(quote! {Default::default()});
        } else {
            update_columns_safe.push(quote! {::rust_query::private::AsUpdate});
            try_from_update.push(quote! {val.#ident});
        }
        parts.push(quote! {&col.#ident});
        generic.push(make_generic(ident));
        col_str.push(ident.to_string());
        col_ident.push(ident);
        col_doc.push(&col.doc_comments);

        if col.is_def {
            col_typ_original.push(col.typ.clone());
        } else {
            let next_mod = next_mod.unwrap();
            col_typ_original
                .push(quote! {<super::#next_mod::#tmp as ::rust_query::private::MyTyp>::Prev});
        }

        col_typ.push(tmp);
        col_span.push(byte_range(source, col.name.span()));
        empty.push(quote! {});
    }

    let alias_ident = format_ident!("{}Alias", table_ident);

    let (referer, referer_expr) = if table.referenceable {
        (quote! {()}, quote! {})
    } else {
        (quote! {::std::convert::Infallible}, quote! {unreachable!()})
    };

    let wrap_parts = wrap(&parts);
    let wrap_ident = wrap(&col_ident);

    // Default to the current table if there is no previous table.
    // This could change to another default type in the future.
    let migrate_from = if let Some(prev) = &table.prev {
        let prev_mod = prev_mod.unwrap();
        quote! {super::#prev_mod::#prev}
    } else {
        quote! {Self}
    };

    let table_doc_comments = &table.doc_comments;

    Ok(quote! {
        #(#table_doc_comments)*
        pub struct #table_ident_with_span<#(#generic = ()),*> {#(
            #(#col_doc)*
            pub #col_ident: #generic,
        )*}
        type #alias_ident<#(#generic),*> = #table_ident<#(<#generic as ::rust_query::private::Apply>::Out<#col_typ, #schema>),*>;

        #[doc(hidden)]
        pub struct #table_helper(());
        #[allow(non_upper_case_globals)]
        pub const #table_ident_with_span: #table_helper = #table_helper(());

        impl<'inner> ::rust_query::private::Joinable<'inner> for #table_helper {
            type Typ = #table_ident;
            fn conds(self) -> ::std::vec::Vec<(&'static str, ::rust_query::private::DynTypedExpr)> {
                ::std::vec::Vec::new()
            }
        }

        #(
            pub(super) type #col_typ = #col_typ_original;
        )*

        impl<#(#generic: ::rust_query::private::UpdateOrUnit< #schema, #col_typ>),*> Default for #table_ident<#(#generic),*> {
            fn default() -> Self {
                Self {#(
                    #col_ident: Default::default(),
                )*}
            }
        }

        const _: () = {
            impl ::rust_query::Table for #table_ident {
                type MigrateFrom = #migrate_from;

                type Ext2<'t> = #alias_ident<#(#empty ::rust_query::private::AsExpr<'t>),*>;

                fn covariant_ext<'x, 't>(val: &'x Self::Ext2<'static>) -> &'x Self::Ext2<'t> {
                    val
                }

                fn build_ext2<'t>(val: &::rust_query::Expr<'t, Self::Schema, Self>) -> Self::Ext2<'t> {
                    Self::Ext2 {
                        #(#col_ident: ::rust_query::private::new_column(val, #col_str),)*
                    }
                }

                type Schema = #schema;

                fn typs(f: &mut ::rust_query::private::TypBuilder<Self::Schema>) {
                    #(f.col::<#col_typ>(#col_str, #col_span);)*
                    #(#def_typs;)*
                    #(#unique_typs;)*
                }

                const ID: &'static str = "id";
                const NAME: &'static str = #table_name;
                const SPAN: (usize, usize) = #table_span;

                type Conflict = #conflict_type;
                type UpdateOk = (#alias_ident<#(#update_columns_safe),*>);
                type Update = (#alias_ident<#(#empty ::rust_query::private::AsUpdate),*>);
                type Insert = (#alias_ident<#(#empty ::rust_query::private::AsExpr<'static>),*>);
                type Lazy<'t> = (#alias_ident<#(#empty ::rust_query::private::Lazy<'t>),*>);

                fn read(val: &Self::Insert, f: &mut ::rust_query::private::Reader<Self::Schema>) {
                    #(f.col(#col_str, &val.#col_ident);)*
                }

                fn get_conflict_unchecked(
                    txn: &::rust_query::Transaction<Self::Schema>,
                    val: &Self::Insert
                ) -> Self::Conflict {
                    #conflict_dummy_insert
                }

                fn update_into_try_update(val: Self::UpdateOk) -> Self::Update {
                    #table_ident {#(
                        #col_ident: #try_from_update,
                    )*}
                }

                fn apply_try_update(
                    val: Self::Update,
                    old: ::rust_query::Expr<'static, Self::Schema, Self>,
                ) -> Self::Insert {
                    #table_ident {#(
                        #col_ident: val.#col_ident.apply(&old.#col_ident),
                    )*}
                }

                type Referer = #referer;
                fn get_referer_unchecked() -> Self::Referer {
                    #referer_expr
                }

                fn get_lazy<'t>(txn: &'t ::rust_query::Transaction<Self::Schema>, row: ::rust_query::TableRow<Self>) -> Self::Lazy<'t> {
                    let col = ::rust_query::IntoExpr::<'_, #schema>::into_expr(row);
                    let #wrap_ident = txn.query_one(#wrap_parts);
                    #table_ident {
                        #(#col_ident: <#col_typ as ::rust_query::private::MyTyp>::out_to_lazy(#col_ident),)*
                    }
                }
            }
        };

        impl<#(#generic),*> ::rust_query::private::TableInsert for #table_ident<#(#generic),*>
        where
            #(#generic: ::rust_query::IntoExpr<'static, #schema, Typ = #col_typ>,)*
        {
            type T = #table_ident;
            fn into_insert(self) -> <Self::T as ::rust_query::Table>::Insert {
                #table_ident {#(
                    #col_ident: ::rust_query::IntoExpr::into_expr(self.#col_ident),
                )*}
            }
        }

        const _: () = {
            #unique_helpers
        };
    })
}

impl SingleVersionTable {
    pub fn conflict(&self) -> (TokenStream, TokenStream) {
        let unique_indices: Vec<_> = self.indices.iter().filter(|index| index.unique).collect();
        match *unique_indices {
            [] => (quote! {::std::convert::Infallible}, quote! {unreachable!()}),
            [unique] => {
                let table_ident = &self.name;

                let col = &unique.columns;
                (
                    quote! {::rust_query::TableRow<#table_ident>},
                    quote! {
                        txn.query_one(#table_ident #(.#col(&val.#col))*).unwrap()
                    },
                )
            }
            _ => (quote! {()}, quote! {()}),
        }
    }
}
