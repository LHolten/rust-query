use std::ops::Not;

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
    let columns = &table.columns;

    let mut unique_typs = vec![];
    let mut unique_funcs = vec![];
    let mut unique_defs = vec![];
    for unique in &table.uniques {
        let column_strs = unique.columns.iter().map(|x| x.to_string());
        let unique_name = &unique.name;
        let unique_type = make_generic(unique_name);

        let mut args = vec![];
        let mut generics = vec![];
        let mut constraints = vec![];
        let mut inits = vec![];
        for col in &unique.columns {
            let typ = &columns
                .values()
                .find(|x| &x.name == col)
                .ok_or_else(|| {
                    syn::Error::new_spanned(
                        col,
                        "a column exists for every name in the unique constraint",
                    )
                })?
                .typ;
            let generic = make_generic(col);

            args.push(quote! {#col: #generic});
            constraints.push(quote! {#generic: ::rust_query::Value<'a, #schema, Typ = #typ>});
            generics.push(generic);
            inits.push(col.clone());
        }

        unique_typs.push(quote! {f.unique(&[#(#column_strs),*])});

        unique_funcs.push(quote! {
            pub fn #unique_name<'a #(,#constraints)*>(#(#args),*) -> #table_mod::#unique_type<#(#generics),*> {
                #table_mod::#unique_type {
                    #(#inits),*
                }
            }
        });
        unique_defs.push(define_unique(unique, table_name, table_ident, schema));
    }

    let mut defs = vec![];
    let mut typ_asserts = vec![];
    let mut reads = vec![];
    let mut def_typs = vec![];
    let mut col_defs = vec![];

    for col in columns.values() {
        let typ = &col.typ;
        let ident = &col.name;
        let ident_str = ident.to_string();
        defs.push(quote! {
            pub fn #ident<'t>(&self) -> ::rust_query::DynValue<'t, #schema, #typ>
            where
                T: ::rust_query::Value<'t, #schema, Typ = #table_ident> + 't
            {
                ::rust_query::Value::into_dyn(&::rust_query::ops::Col::new(#ident_str, self.0.clone()))
            }
        });
        typ_asserts.push(quote!(::rust_query::private::valid_in_schema::<#schema, #typ>();));
        reads.push(quote!(f.col::<#schema>(#ident_str, self.#ident)));
        def_typs.push(quote!(f.col::<#typ>(#ident_str)));
        col_defs.push(quote! {pub #ident: <#typ as ::rust_query::private::MyTyp>::Out<'a>});
    }

    let dummy_ident = format_ident!("{}Dummy", table_ident);

    let generic = col_defs.is_empty().not().then_some(quote! {'a});

    Ok(quote! {
        #[repr(transparent)]
        pub struct #table_ident<T = ()>(T);
        ::rust_query::unsafe_impl_ref_cast! {#table_ident}

        impl<T: Clone + ::rust_query::private::Typed<Typ = #table_ident>> #table_ident<T> {
            #(#defs)*
        }

        impl ::rust_query::Table for #table_ident {
            type Ext<T> = #table_ident<T>;
            type Schema = #schema;

            fn typs(f: &mut ::rust_query::private::TypBuilder) {
                #(#def_typs;)*
                #(#unique_typs;)*
            }

            const ID: &'static str = "id";
            const NAME: &'static str = #table_name;
        }

        pub struct #dummy_ident<#generic> {
            #(#col_defs),*
        }

        impl<'x, #generic> ::rust_query::private::Writable<'x> for #dummy_ident<#generic> {
            type T = #table_ident;
            fn read(self, f: ::rust_query::private::Reader<'x, #schema>) {
                #(#reads;)*
            }
        }

        #[allow(unused)]
        impl #table_ident {
            #(#unique_funcs)*
        }

        mod #table_mod {
            #(#unique_defs)*
        }

        const _: fn() = || {
            #(#typ_asserts)*
        };
    })
}

fn define_unique(
    unique: &Unique,
    table_str: &str,
    table_typ: &Ident,
    schema: &Ident,
) -> TokenStream {
    let name = &unique.name;
    let typ_name = make_generic(name);

    let mut generics = vec![];
    let mut fields = vec![];
    let mut constraints = vec![];
    let mut conds = vec![];
    for col in &unique.columns {
        let col_str = col.to_string();

        let generic = make_generic(col);
        fields.push(quote! {pub(super) #col: #generic});
        constraints.push(quote! {#generic: ::rust_query::Value<'t, super::#schema>});
        conds.push(quote! {(#col_str, self.#col.build_expr(b))});
        generics.push(generic);
    }

    quote! {
        #[derive(Clone, Copy)]
        pub struct #typ_name<#(#generics),*> {
            #(#fields),*
        }

        impl<#(#generics),*> ::rust_query::private::Typed for #typ_name<#(#generics),*> {
            type Typ = Option<super::#table_typ>;
        }
        impl<'t, #(#constraints),*> ::rust_query::Value<'t, super::#schema> for #typ_name<#(#generics),*> {
            fn build_expr(&self, b: ::rust_query::private::ValueBuilder) -> ::rust_query::private::SimpleExpr {
                b.get_unique(#table_str, vec![#(#conds),*])
            }
        }
    }
}
