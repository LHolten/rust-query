use super::make_generic;
use heck::ToSnekCase;
use quote::{format_ident, quote};

use proc_macro2::TokenStream;

use syn::Ident;

use super::Table;

pub(crate) fn define_table(table: &Table, schema: &Ident) -> TokenStream {
    let table_ident = &table.name;
    let columns = &table.columns;

    let mut unique_typs = vec![];
    let mut unique_funcs = vec![];
    for unique in &table.uniques {
        let column_strs = unique.columns.iter().map(|x| x.to_string());
        let unique_name = &unique.name;
        let args = unique.columns.iter().map(|col| {
            let typ = &columns
                .values()
                .find(|x| &x.name == col)
                .expect("a column exists for every name in the unique constraint")
                .typ;
            quote! {#col: impl ::rust_query::Value<'a, Typ=#typ>}
        });
        unique_typs.push(quote! {f.unique(&[#(#column_strs),*])});
        unique_funcs.push(quote! {
            pub fn #unique_name<'a>(&self, #(#args),*) -> ::rust_query::DbCol<'a, Option<#table_ident>> {
                todo!();
            }
        })
    }

    let mut defs = vec![];
    let mut typ_asserts = vec![];
    let mut read_bounds = vec![];
    let mut inits = vec![];
    let mut reads = vec![];
    let mut def_typs = vec![];

    for col in columns.values() {
        let typ = &col.typ;
        let ident = &col.name;
        let ident_str = ident.to_string();
        let generic = make_generic(ident);
        defs.push(quote! {
            pub fn #ident(&self) -> ::rust_query::Col<#typ, T> {
                ::rust_query::Col::new(#ident_str, self.0.clone())
            }
        });
        typ_asserts.push(quote!(::rust_query::valid_in_schema::<#schema, #typ>();));
        read_bounds.push(quote!(#generic: ::rust_query::Value<'t, Typ=#typ>));
        inits.push(quote!(#ident: f.col(#ident_str)));
        reads.push(quote!(f.col(#ident_str, self.#ident)));
        def_typs.push(quote!(f.col::<#typ>(#ident_str)))
    }

    let dummy_ident = format_ident!("{}Dummy", table_ident);

    let table_name: &String = &table_ident.to_string().to_snek_case();
    let has_id = quote!(
        impl ::rust_query::HasId for #table_ident {
            const ID: &'static str = "id";
            const NAME: &'static str = #table_name;
        }
    );

    quote! {
        pub struct #table_ident(());

        #[repr(transparent)]
        #[derive(::rust_query::private::RefCast)]
        pub struct #dummy_ident<T>(T);

        impl<T: Clone> #dummy_ident<T> {
            #(#defs)*
        }

        impl ::rust_query::Table for #table_ident {
            type Dummy<T> = #dummy_ident<T>;
            type Schema = #schema;

            fn name(&self) -> String {
                #table_name.to_owned()
            }

            fn typs(f: &mut ::rust_query::TypBuilder) {
                #(#def_typs;)*
                #(#unique_typs;)*
            }
        }

        // impl<'t, #(#read_bounds),*> ::rust_query::private::Writable<'t> for #dummy_ident<#(#generics),*> {
        //     type T = #table_ident;
        //     fn read(self: Box<Self>, f: ::rust_query::private::Reader<'_, 't>) {
        //         #(#reads;)*
        //     }
        // }

        #[allow(unused)]
        impl #table_ident {
            #(#unique_funcs)*
        }

        const _: fn() = || {
            #(#typ_asserts)*
        };

        #has_id
    }
}
