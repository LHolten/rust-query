use crate::{dummy::wrap, multi::Unique, SingleVersionTable};

use super::make_generic;
use heck::ToSnekCase;
use quote::{format_ident, quote};

use proc_macro2::{Span, TokenStream};

use syn::Ident;

pub(crate) fn define_table(
    table: &mut SingleVersionTable,
    schema: &Ident,
    struct_id: usize,
) -> syn::Result<TokenStream> {
    let table_ident_with_span = table.name.clone();
    table.name.set_span(Span::call_site());
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
        unique_defs.push(define_unique(unique, &table_ident));
    }

    let (conflict_type, conflict_dummy_insert) = table.conflict(quote! {}, quote! {#schema});

    let mut def_typs = vec![];
    let mut update_columns_safe = vec![];
    let mut generic = vec![];
    let mut try_from_update = vec![];
    let mut col_str = vec![];
    let mut col_ident = vec![];
    let mut col_typ = vec![];
    let mut empty = vec![];
    let mut parts = vec![];

    for col in table.columns.values() {
        let typ = &col.typ;
        let ident = &col.name;

        let mut unique_columns = table.uniques.iter().flat_map(|u| &u.columns);
        if unique_columns.any(|x| x == ident) {
            def_typs.push(quote!(f.check_unique_compatible::<#typ>()));
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
        col_typ.push(typ);
        empty.push(quote! {})
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
                type TryUpdate<'t> = (#table_ident<#(#empty ::rust_query::private::Update<'t>),*>);
                type Insert<'t> = (#table_ident<#(#empty ::rust_query::private::AsExpr<'t>),*>);

                fn read<'t>(val: &Self::Insert<'t>, f: &::rust_query::private::Reader<'t, Self::Schema>) {
                    #(f.col(#col_str, &val.#col_ident);)*
                }

                fn get_conflict_unchecked<'t>(val: &Self::Insert<'t>) -> ::rust_query::Select<'t, 't, Self::Schema, Option<Self::Conflict<'t>>>
                {
                    #conflict_dummy_insert
                }

                fn update_into_try_update<'t>(val: Self::Update<'t>) -> Self::TryUpdate<'t> {
                    #table_ident {#(
                        #col_ident: #try_from_update,
                    )*}
                }

                fn apply_try_update<'t>(
                    val: Self::TryUpdate<'t>,
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

        mod #table_mod {
            #(#unique_defs)*
        }
    })
}

fn define_unique(unique: &Unique, table_ident: &Ident) -> TokenStream {
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
            type Typ = Option<super::#table_ident>;
            fn build_expr(&self, b: ::rust_query::private::ValueBuilder) -> ::rust_query::private::SimpleExpr {
                b.get_unique::<super::#table_ident>(vec![#(
                    (#col_str, self.#col.build_expr(b)),
                )*])
            }
        }
    }
}

impl SingleVersionTable {
    pub fn conflict(&self, prefix: TokenStream, schema: TokenStream) -> (TokenStream, TokenStream) {
        match &*self.uniques {
            [] => (
                quote! {::std::convert::Infallible},
                quote! {{
                    let x = ::rust_query::IntoExpr::into_expr(&0i64);
                    ::rust_query::IntoSelectExt::map_select(x, |_| unreachable!())
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
                            #col: ::rust_query::private::into_owned::<#schema, _>(&val.#col),
                        )*})
                    },
                )
            }
            _ => (
                quote! {()},
                quote! {{
                    let x = ::rust_query::IntoExpr::into_expr(&0i64);
                    ::rust_query::IntoSelectExt::map_select(x, |_| Some(()))
                }},
            ),
        }
    }
}
