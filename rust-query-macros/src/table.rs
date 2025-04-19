use std::collections::BTreeMap;

use crate::{dummy::wrap, SingleVersionTable};

use super::make_generic;
use heck::ToSnekCase;
use quote::{format_ident, quote};

use proc_macro2::{Span, TokenStream};

use syn::{spanned::Spanned, Ident};

pub fn define_all_tables(
    schema_name: &Ident,
    mut new_struct_id: impl FnMut() -> usize,
    prev_mod: &Option<Ident>,
    next_mod: &Option<Ident>,
    version: u32,
    new_tables: &mut BTreeMap<usize, SingleVersionTable>,
) -> TokenStream {
    let mut mod_output = TokenStream::new();
    let mut schema_table_typs = vec![];
    for table in new_tables.values_mut() {
        mod_output.extend(define_table(
            table,
            schema_name,
            prev_mod.as_ref(),
            next_mod.as_ref(),
            new_struct_id(),
        ));

        let table_name = &table.name;
        schema_table_typs.push(quote! {b.table::<#table_name>()});
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
    mod_output
}

fn define_table(
    table: &mut SingleVersionTable,
    schema: &Ident,
    prev_mod: Option<&Ident>,
    next_mod: Option<&Ident>,
    struct_id: usize,
) -> syn::Result<TokenStream> {
    let table_ident_with_span = table.name.clone();
    table.name.set_span(Span::call_site());
    let table_ident = &table.name;
    let table_name: &String = &table_ident.to_string().to_snek_case();

    let mut unique_typs = vec![];
    let mut unique_funcs = vec![];
    for unique in &table.uniques {
        let unique_name = &unique.name;

        let col = &unique.columns;
        let mut col_typ = vec![];
        let mut col_str = vec![];
        for col in col {
            let i = &table
                .columns
                .iter()
                .find_map(|(i, x)| (&x.name == col).then_some(i))
                .ok_or_else(|| {
                    syn::Error::new_spanned(
                        col,
                        "Expected a column to exists for every name in the unique constraint.",
                    )
                })?;
            let tmp = format_ident!("_{table_ident}{i}");

            col_typ.push(tmp);
            col_str.push(col.to_string());
        }

        unique_typs.push(quote! {f.unique(&[#(#col_str),*])});

        unique_funcs.push(quote! {
            pub fn #unique_name<'a>(#(#col: impl ::rust_query::IntoExpr<'a, #schema, Typ = #col_typ>),*) 
                -> ::rust_query::Expr<'a, #schema, Option<#table_ident>>
            {
                #(
                    let #col = ::rust_query::private::into_owned(#col);
                )*
                ::rust_query::private::adhoc_expr(move |b| {
                    b.get_unique::<#table_ident>(vec![#(
                        (#col_str, ::rust_query::private::Typed::build_expr(&#col, b)),
                    )*])
                })
            }
        });
    }

    let (conflict_type, conflict_dummy_insert) = table.conflict();

    let mut def_typs = vec![];
    let mut update_columns_safe = vec![];
    let mut generic = vec![];
    let mut try_from_update = vec![];
    let mut col_str = vec![];
    let mut col_ident = vec![];
    let mut col_typ = vec![];
    let mut col_typ_original = vec![];
    let mut empty = vec![];
    let mut parts = vec![];

    for (i, col) in &table.columns {
        let ident = &col.name;
        let tmp = format_ident!("_{table_ident}{i}", span = col.typ.span());

        let mut unique_columns = table.uniques.iter().flat_map(|u| &u.columns);
        if unique_columns.any(|x| x == ident) {
            def_typs.push(quote!(f.check_unique_compatible::<#tmp>()));
            update_columns_safe.push(quote! {::rust_query::private::Ignore});
            try_from_update.push(quote! {Default::default()});
        } else {
            update_columns_safe.push(quote! {::rust_query::private::Update<'t>});
            try_from_update.push(quote! {val.#ident});
        }
        parts.push(quote! {::rust_query::FromExpr::from_expr(col.#ident())});
        generic.push(make_generic(ident));
        col_str.push(ident.to_string());
        col_ident.push(ident);

        if col.is_def {
            col_typ_original.push(col.typ.clone());
        } else {
            let next_mod = next_mod.unwrap();
            col_typ_original
                .push(quote! {<super::#next_mod::#tmp as ::rust_query::private::MyTyp>::Prev});
        }

        col_typ.push(tmp);
        empty.push(quote! {});
    }

    let mut safe_default = None;
    if !table.uniques.is_empty() {
        safe_default = Some(quote! {
            impl<'t> Default for #table_ident<#(#update_columns_safe),*> {
                fn default() -> Self {
                    Self {#(
                        #col_ident: Default::default(),
                    )*}
                }
            }
        })
    }

    let ext_ident = format_ident!("{}Ext", table_ident);
    let macro_ident = format_ident!("{}Macro", table_ident);

    let (referer, referer_expr) = if table.referenceable {
        (quote! {()}, quote! {()})
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

    Ok(quote! {
        pub struct #table_ident_with_span<#(#generic: ::rust_query::private::Apply = ::rust_query::private::Ignore),*> {#(
            pub #col_ident: #generic::Out<#col_typ, #schema>,
        )*}

        impl<#(#generic: ::rust_query::private::Apply),*> ::rust_query::private::Instantiate<#struct_id, (#(#generic),*)> for super::MacroRoot {
            type Out = (#table_ident<#(#generic),*>);
        }

        impl<'transaction, #(#generic: ::rust_query::private::Apply + 'transaction),*> ::rust_query::FromExpr<'transaction, #schema, #table_ident>
            for #table_ident<#(#generic),*>
        where #(#generic::Out<#col_typ, #schema>: ::rust_query::FromExpr<'transaction, #schema, #col_typ>,)*
        {
            /// How to turn a column reference into a [Select].
            fn from_expr<'columns>(
                col: impl ::rust_query::IntoExpr<'columns, #schema, Typ = #table_ident>,
            ) -> ::rust_query::Select<'columns, 'transaction, #schema, Self> {
                let col = ::rust_query::IntoExpr::into_expr(col);
                ::rust_query::IntoSelectExt::map_select(#wrap_parts, |#wrap_ident| #table_ident {
                    #(#col_ident,)*
                })
            }
        }

        #(
            pub(super) type #col_typ = #col_typ_original;
        )*

        mod #macro_ident {
            #[allow(unused_macros)]
            macro_rules! #table_ident_with_span {
                ($($spec:tt)*) => {
                    ::rust_query::private::fields!{#struct_id {$($spec)*} {#(#col_ident),*}}
                };
            }
            pub(crate) use #table_ident;
        }
        #[allow(unused_imports)]
        pub(crate) use #macro_ident::#table_ident;

        impl<'t> Default for #table_ident<#(#empty ::rust_query::private::Update<'t>),*> {
            fn default() -> Self {
                Self {#(
                    #col_ident: Default::default(),
                )*}
            }
        }

        #safe_default

        const _: () = {
            #[repr(transparent)]
            pub struct #ext_ident<T>(T);
            ::rust_query::unsafe_impl_ref_cast! {#ext_ident}

            impl<'t, T> #ext_ident<T>
                where T: ::rust_query::IntoExpr<'t, #schema, Typ = #table_ident>
            {#(
                pub fn #col_ident(&self) -> ::rust_query::Expr<'t, #schema, #col_typ> {
                    ::rust_query::private::new_column(::rust_query::private::Col::new(#col_str, ::rust_query::private::into_owned(&self.0)))
                }
            )*}

            impl ::rust_query::Table for #table_ident {
                type MigrateFrom = #migrate_from;
                type Ext<T> = #ext_ident<T>;
                type Schema = #schema;

                fn typs(f: &mut ::rust_query::private::TypBuilder<Self::Schema>) {
                    #(f.col::<#col_typ>(#col_str);)*
                    #(#def_typs;)*
                    #(#unique_typs;)*
                }

                const ID: &'static str = "id";
                const NAME: &'static str = #table_name;

                type Conflict<'t> = #conflict_type;
                type UpdateOk<'t> = (#table_ident<#(#update_columns_safe),*>);
                type Update<'t> = (#table_ident<#(#empty ::rust_query::private::Update<'t>),*>);
                type Insert<'t> = (#table_ident<#(#empty ::rust_query::private::AsExpr<'t>),*>);

                fn read<'t>(val: &Self::Insert<'t>, f: &::rust_query::private::Reader<'t, Self::Schema>) {
                    #(f.col(#col_str, &val.#col_ident);)*
                }

                fn get_conflict_unchecked<'t>(
                    txn: &::rust_query::Transaction<'t, Self::Schema>,
                    val: &Self::Insert<'t>
                ) -> Self::Conflict<'t> {
                    #conflict_dummy_insert
                }

                fn update_into_try_update<'t>(val: Self::UpdateOk<'t>) -> Self::Update<'t> {
                    #table_ident {#(
                        #col_ident: #try_from_update,
                    )*}
                }

                fn apply_try_update<'t>(
                    val: Self::Update<'t>,
                    old: ::rust_query::Expr<'t, Self::Schema, Self>,
                ) -> Self::Insert<'t> {
                    #table_ident {#(
                        #col_ident: val.#col_ident.apply(old.#col_ident()),
                    )*}
                }

                type Referer = #referer;
                fn get_referer_unchecked() -> Self::Referer {
                    #referer_expr
                }
            }
        };

        impl<'t #(, #generic)*> ::rust_query::private::TableInsert<'t> for #table_ident<#(::rust_query::private::Custom<#generic>),*>
        where
            #(#generic: ::rust_query::IntoExpr<'t, #schema, Typ = #col_typ>,)*
        {
            type T = #table_ident;
            fn into_insert(self) -> <Self::T as ::rust_query::Table>::Insert<'t> {
                #table_ident {#(
                    #col_ident: ::rust_query::IntoExpr::into_expr(self.#col_ident),
                )*}
            }
        }

        #[allow(unused)]
        impl #table_ident {
            #(#unique_funcs)*
        }
    })
}

impl SingleVersionTable {
    pub fn conflict(&self) -> (TokenStream, TokenStream) {
        match &*self.uniques {
            [] => (quote! {::std::convert::Infallible}, quote! {unreachable!()}),
            [unique] => {
                let unique_name = &unique.name;
                let table_ident = &self.name;

                let col = &unique.columns;
                (
                    quote! {::rust_query::TableRow<'t, #table_ident>},
                    quote! {
                        txn.query_one(#table_ident::#unique_name(#(&val.#col),*)).unwrap()
                    },
                )
            }
            _ => (quote! {()}, quote! {()}),
        }
    }
}
