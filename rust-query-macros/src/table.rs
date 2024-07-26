use crate::Unique;

use super::make_generic;
use heck::ToSnekCase;
use quote::{format_ident, quote};

use proc_macro2::TokenStream;

use syn::Ident;

use super::Table;

pub(crate) fn define_table(table: &Table, schema: &Ident) -> TokenStream {
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
                .expect("a column exists for every name in the unique constraint")
                .typ;
            let generic = make_generic(col);

            args.push(quote! {#col: #generic});
            constraints.push(quote! {#generic: ::rust_query::Value<'a, Typ = #typ>});
            generics.push(generic);
            inits.push(col.clone());
        }

        unique_typs.push(quote! {f.unique(&[#(#column_strs),*])});

        unique_funcs.push(quote! {
            pub fn #unique_name<'a #(,#constraints)*>(&self, #(#args),*) -> #table_mod::#unique_type<#(#generics),*> {
                #table_mod::#unique_type {
                    #(#inits),*
                }
            }
        });
        unique_defs.push(define_unique(unique, table_name, table_ident));
    }

    let mut defs = vec![];
    let mut typ_asserts = vec![];
    let mut read_bounds = vec![];
    let mut inits = vec![];
    let mut reads = vec![];
    let mut def_typs = vec![];
    let mut col_defs = vec![];
    let mut generics = vec![];

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
        read_bounds.push(quote!(#generic: ::rust_query::Covariant<'t, Typ=#typ>));
        inits.push(quote!(#ident: f.col(#ident_str)));
        reads.push(quote!(f.col(#ident_str, self.#ident)));
        def_typs.push(quote!(f.col::<#typ>(#ident_str)));
        col_defs.push(quote! {pub #ident: #generic});
        generics.push(generic);
    }

    let dummy_ident = format_ident!("{}Dummy", table_ident);
    let dummy2_ident = format_ident!("{}Dummy2", table_ident);

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

        pub struct #dummy2_ident<#(#generics),*> {
            #(#col_defs),*
        }

        impl<'t, #(#read_bounds),*> ::rust_query::private::Writable<'t> for #dummy2_ident<#(#generics),*> {
            type T = #table_ident;
            fn read(self: Box<Self>, f: ::rust_query::private::Reader<'_, 't>) {
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

        #has_id
    }
}

fn define_unique(unique: &Unique, table_str: &str, table_typ: &Ident) -> TokenStream {
    let name = &unique.name;
    let typ_name = make_generic(name);

    let mut generics = vec![];
    let mut fields = vec![];
    let mut constraints = vec![];
    let mut constraints_covariant = vec![];
    let mut conds = vec![];
    for col in &unique.columns {
        let col_str = col.to_string();

        let generic = make_generic(col);
        fields.push(quote! {pub(super) #col: #generic});
        constraints.push(quote! {#generic: ::rust_query::Value<'t>});
        constraints_covariant.push(quote! {#generic: ::rust_query::Covariant<'t>});
        conds.push(quote! {(#col_str, self.#col.build_expr(b))});
        generics.push(generic);
    }

    quote! {
        #[derive(Clone, Copy)]
        pub struct #typ_name<#(#generics),*> {
            #(#fields),*
        }

        impl<'t, #(#constraints),*> ::rust_query::Value<'t> for #typ_name<#(#generics),*> {
            type Typ = Option<super::#table_typ>;

            fn build_expr(&self, b: ::rust_query::private::ValueBuilder) -> ::rust_query::private::SimpleExpr {
                b.get_unique(#table_str, vec![#(#conds),*])
            }
        }
        impl<'t, #(#constraints_covariant),*> ::rust_query::Covariant<'t> for #typ_name<#(#generics),*> {}
    }
}
