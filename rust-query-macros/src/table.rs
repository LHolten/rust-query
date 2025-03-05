use crate::Unique;

use super::make_generic;
use heck::ToSnekCase;
use quote::{format_ident, quote};

use proc_macro2::TokenStream;

use syn::Ident;

use super::Table;

pub(crate) fn define_table(table: &Table, schema: &Ident) -> syn::Result<TokenStream> {
    let table_ident = &table.name;
    let table_name: &String = &table_ident.to_string().to_snek_case();
    let table_mod = format_ident!("{table_name}");

    let mut unique_typs = vec![];
    let mut unique_funcs = vec![];
    let mut unique_defs = vec![];
    for unique in &table.uniques {
        let column_strs = unique.columns.iter().map(|x| x.to_string());
        let unique_name = &unique.name;
        let unique_type = make_generic(unique_name);

        let col = &unique.columns;
        let mut col_typ = vec![];
        let mut generic = vec![];
        for col in col {
            let typ = &table
                .columns
                .values()
                .find(|x| &x.name == col)
                .ok_or_else(|| {
                    syn::Error::new_spanned(
                        col,
                        "Expected a column to exists for every name in the unique constraint.",
                    )
                })?
                .typ;
            generic.push(make_generic(col));
            col_typ.push(typ);
        }

        unique_typs.push(quote! {f.unique(&[#(#column_strs),*])});

        unique_funcs.push(quote! {
            pub fn #unique_name<'a #(,#generic)*>(#(#col: #generic),*) -> ::rust_query::Expr<'a, #schema, Option<#table_ident>>
            where
                #(#generic: ::rust_query::IntoExpr<'a, #schema, Typ = #col_typ>,)*
            {
                ::rust_query::private::new_column(#table_mod::#unique_type {#(
                    #col: ::rust_query::private::into_owned(#col),
                )*})
            }
        });
        unique_defs.push(define_unique(unique, table_ident));
    }

    let (conflict_type, conflict_dummy_insert) = table.conflict(quote! {}, quote! {#schema});

    let mut def_typs = vec![];
    let mut update_columns_safe = vec![];
    let mut generic = vec![];
    let mut try_from_update = vec![];
    let mut col_str = vec![];
    let mut col_ident = vec![];
    let mut col_typ = vec![];

    for col in table.columns.values() {
        let typ = &col.typ;
        let ident = &col.name;

        let mut unique_columns = table.uniques.iter().flat_map(|u| &u.columns);
        if unique_columns.any(|x| x == ident) {
            def_typs.push(quote!(f.check_unique_compatible::<#typ>()));
            update_columns_safe.push(quote! {()});
            try_from_update.push(quote! {Default::default()});
        } else {
            update_columns_safe.push(quote! {::rust_query::Update<'t, #schema, #typ>});
            try_from_update.push(quote! {val.#ident});
        }
        generic.push(make_generic(ident));
        col_str.push(ident.to_string());
        col_ident.push(ident);
        col_typ.push(typ);
    }

    let mut safe_default = None;
    if !table.uniques.is_empty() {
        safe_default = Some(quote! {
            impl<'t> Default for <#table_ident as ::rust_query::Table>::Update<'t> {
                fn default() -> Self {
                    Self {#(
                        #col_ident: Default::default(),
                    )*}
                }
            }
        })
    }

    let ext_ident = format_ident!("{}Ext", table_ident);

    let (referer, referer_expr) = if table.referer {
        (quote! {()}, quote! {()})
    } else {
        (quote! {::std::convert::Infallible}, quote! {unreachable!()})
    };

    Ok(quote! {
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

        pub struct #table_ident<#(#generic = ()),*> {#(
            pub #col_ident: #generic,
        )*}

        impl<'t> Default for <#table_ident as ::rust_query::Table>::TryUpdate<'t> {
            fn default() -> Self {
                Self {#(
                    #col_ident: Default::default(),
                )*}
            }
        }

        #safe_default

        impl ::rust_query::Table for #table_ident {
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
            type Update<'t> = (#table_ident<#(#update_columns_safe),*>);
            type TryUpdate<'t> = (#table_ident<#(::rust_query::Update<'t, #schema, #col_typ>),*>);

            fn update_into_try_update<'t>(val: Self::Update<'t>) -> Self::TryUpdate<'t> {
                #table_ident {#(
                    #col_ident: #try_from_update,
                )*}
            }

            fn apply_try_update<'t>(
                val: Self::TryUpdate<'t>,
                old: ::rust_query::Expr<'t, Self::Schema, Self>,
            ) -> impl ::rust_query::private::TableInsert<'t, T = Self, Schema = Self::Schema, Conflict = Self::Conflict<'t>> {
                #table_ident {#(
                    #col_ident: val.#col_ident.apply(old.#col_ident()),
                )*}
            }

            type Referer = #referer;
            fn get_referer_unchecked() -> Self::Referer {
                #referer_expr
            }
        }
        impl<'t #(, #generic)*> ::rust_query::private::TableInsert<'t> for #table_ident<#(#generic),*>
        where
            #(#generic: ::rust_query::IntoExpr<'t, #schema, Typ = #col_typ>,)*
        {
            type Schema = #schema;
            type T = #table_ident;
            type Conflict = #conflict_type;
            fn read(&self, f: ::rust_query::private::Reader<'_, 't, Self::Schema>) {
                #(f.col(#col_str, &self.#col_ident);)*
            }
            fn get_conflict_unchecked(&self) -> ::rust_query::Select<'t, 't, Self::Schema, Option<Self::Conflict>>
            {
                #conflict_dummy_insert
            }
        }

        #[allow(unused)]
        impl #table_ident {
            #(#unique_funcs)*
        }

        mod #table_mod {
            #(#unique_defs)*
        }
    })
}

fn define_unique(unique: &Unique, table_typ: &Ident) -> TokenStream {
    let name = &unique.name;
    let typ_name = make_generic(name);

    let col = &unique.columns;
    let mut generic = vec![];
    let mut col_str = vec![];
    for col in col {
        generic.push(make_generic(col));
        col_str.push(col.to_string());
    }

    quote! {
        pub struct #typ_name<#(#generic),*> {#(
            pub(super) #col: #generic,
        )*}

        impl<#(#generic: ::rust_query::private::Typed),*> ::rust_query::private::Typed for #typ_name<#(#generic),*> {
            type Typ = Option<super::#table_typ>;
            fn build_expr(&self, b: ::rust_query::private::ValueBuilder) -> ::rust_query::private::SimpleExpr {
                b.get_unique::<super::#table_typ>(vec![#(
                    (#col_str, self.#col.build_expr(b)),
                )*])
            }
        }
    }
}

impl Table {
    pub fn conflict(&self, prefix: TokenStream, schema: TokenStream) -> (TokenStream, TokenStream) {
        match &*self.uniques {
            [] => (
                quote! {::std::convert::Infallible},
                quote! {{
                    let x = ::rust_query::IntoExpr::into_expr(&0i64);
                    ::rust_query::IntoSelectExt::map_dummy(x, |_| unreachable!())
                }},
            ),
            [unique] => {
                let unique_name = &unique.name;
                let unique_type = make_generic(unique_name);

                let table_ident = &self.name;
                let table_name: &String = &table_ident.to_string().to_snek_case();
                let table_mod = format_ident!("{table_name}");

                let col = &unique.columns;
                (
                    quote! {::rust_query::TableRow<'t, #prefix #table_ident>},
                    quote! {
                        ::rust_query::private::new_dummy(#prefix #table_mod::#unique_type {#(
                            #col: ::rust_query::private::into_owned::<#schema, _>(&self.#col),
                        )*})
                    },
                )
            }
            _ => (
                quote! {()},
                quote! {{
                    let x = ::rust_query::IntoExpr::into_expr(&0i64);
                    ::rust_query::IntoSelectExt::map_dummy(x, |_| Some(()))
                }},
            ),
        }
    }
}
