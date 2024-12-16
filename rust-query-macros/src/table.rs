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

        let mut args = vec![];
        let mut generics = vec![];
        let mut constraints = vec![];
        let mut inits = vec![];
        for col in &unique.columns {
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
            let generic = make_generic(col);

            args.push(quote! {#col: #generic});
            constraints.push(quote! {#generic: ::rust_query::IntoColumn<'a, #schema, Typ = #typ>});
            generics.push(generic);
            inits.push(col.clone());
        }

        unique_typs.push(quote! {f.unique(&[#(#column_strs),*])});

        unique_funcs.push(quote! {
            pub fn #unique_name<'a #(,#constraints)*>(#(#args),*) -> ::rust_query::Column<'a, #schema, Option<#table_ident>> {
                ::rust_query::IntoColumn::into_column(#table_mod::#unique_type {
                    #(#inits),*
                })
            }
        });
        unique_defs.push(define_unique(unique, table_name, table_ident, schema));
    }

    let (conflict_type, conflict_dummy) = match &*table.uniques {
        [] => (
            quote! {::std::convert::Infallible},
            quote! {
                let x = ::rust_query::IntoColumn::into_column(&0i64);
                ::rust_query::Dummy::map_dummy(x, |_| unreachable!())
            },
        ),
        [unique] => {
            let unique_name = &unique.name;
            let mut parts = vec![];
            for field in &unique.columns {
                parts.push(quote! {&self.#field});
            }
            (
                quote! {::rust_query::TableRow<'t, #table_ident>},
                quote! {
                    #table_ident::#unique_name(#(#parts),*)
                },
            )
        }
        _ => (
            quote! {()},
            quote! {
                let x = ::rust_query::IntoColumn::into_column(&0i64);
                ::rust_query::Dummy::map_dummy(x, |_| Some(()))
            },
        ),
    };

    let mut defs = vec![];
    let mut reads = vec![];
    let mut def_typs = vec![];
    let mut col_defs = vec![];
    let mut generics = vec![];
    let mut generic_defaults = vec![];
    let mut bounds = vec![];
    let mut dummy_columns = vec![];
    let mut dummy_inits = vec![];

    for col in table.columns.values() {
        let typ = &col.typ;
        let ident = &col.name;
        let ident_str = ident.to_string();
        let generic = make_generic(ident);
        defs.push(quote! {
            pub fn #ident(&self) -> ::rust_query::Column<'t, #schema, #typ> {
                ::rust_query::IntoColumn::into_column(::rust_query::private::Col::new(#ident_str, self.0.clone()))
            }
        });
        reads.push(quote!(f.col(#ident_str, &self.#ident)));
        def_typs.push(quote!(f.col::<#typ>(#ident_str)));
        let mut unique_columns = table.uniques.iter().flat_map(|x| &x.columns);
        if unique_columns.any(|x| x == ident) {
            def_typs.push(quote!(f.check_unique_compatible::<#typ>()));
        }
        col_defs.push(quote! {pub #ident: #generic});
        bounds.push(quote! {#generic: ::rust_query::IntoColumn<'t, #schema, Typ = #typ>});
        dummy_columns.push(quote! {::rust_query::Column<'t, #schema, #typ>});
        dummy_inits.push(quote! {#ident: val.#ident()});
        generic_defaults.push(quote! {#generic = ()});
        generics.push(generic);
    }

    let ext_ident = format_ident!("{}Ext", table_ident);

    Ok(quote! {
        #[repr(transparent)]
        pub struct #ext_ident<T>(T);
        ::rust_query::unsafe_impl_ref_cast! {#ext_ident}

        impl<'t, T> #ext_ident<T>
            where T: ::rust_query::IntoColumn<'t, #schema, Typ = #table_ident>
        {
            #(#defs)*
        }

        pub struct #table_ident<#(#generic_defaults),*> {
            #(#col_defs),*
        }

        impl ::rust_query::Table for #table_ident {
            type Ext<T> = #ext_ident<T>;
            type Schema = #schema;

            fn typs(f: &mut ::rust_query::private::TypBuilder<Self::Schema>) {
                #(#def_typs;)*
                #(#unique_typs;)*
            }

            const ID: &'static str = "id";
            const NAME: &'static str = #table_name;

            type Dummy<'t> = #table_ident<#(#dummy_columns),*>;

            fn dummy<'t>(val: impl ::rust_query::IntoColumn<'t, Self::Schema, Typ = Self>) -> Self::Dummy<'t> {
                let val = val.into_column();
                #table_ident {
                    #(#dummy_inits,)*
                }
            }

            type Referer = ();
        }

        impl<'t #(,#bounds)*> ::rust_query::private::Writable<'t> for #table_ident<#(#generics),*> {
            type Schema = #schema;
            type T = #table_ident;
            fn read(&self, f: ::rust_query::private::Reader<'_, 't, Self::Schema>) {
                #(#reads;)*
            }

            type Conflict = #conflict_type;
            fn get_conflict_unchecked(&self) -> impl ::rust_query::Dummy<'t, 't, Self::Schema, Out = Option<Self::Conflict>> {
                #conflict_dummy
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

fn define_unique(
    unique: &Unique,
    table_str: &str,
    table_typ: &Ident,
    schema: &Ident,
) -> TokenStream {
    let name = &unique.name;
    let typ_name = make_generic(name);

    let mut generics = vec![];
    let mut generics_owned = vec![];
    let mut fields = vec![];
    let mut fields_owned = vec![];
    let mut constraints_typed = vec![];
    let mut constraints = vec![];
    let mut conds = vec![];
    for col in &unique.columns {
        let col_str = col.to_string();

        let generic = make_generic(col);
        fields.push(quote! {pub(super) #col: #generic});
        fields_owned.push(quote! {#col: self.#col.into_owned()});
        constraints.push(quote! {#generic: ::rust_query::IntoColumn<'t, super::#schema>});
        constraints_typed.push(quote! {#generic: ::rust_query::private::Typed});
        conds.push(quote! {(#col_str, self.#col.build_expr(b))});
        generics_owned.push(quote! {#generic::Owned});
        generics.push(generic);
    }

    quote! {
        #[derive(Clone, Copy)]
        pub struct #typ_name<#(#generics),*> {
            #(#fields),*
        }

        impl<#(#constraints_typed),*> ::rust_query::private::Typed for #typ_name<#(#generics),*> {
            type Typ = Option<super::#table_typ>;
            fn build_expr(&self, b: ::rust_query::private::ValueBuilder) -> ::rust_query::private::SimpleExpr {
                b.get_unique(#table_str, vec![#(#conds),*])
            }
        }
        impl<'t, #(#constraints),*> ::rust_query::IntoColumn<'t, super::#schema> for #typ_name<#(#generics),*> {
            type Owned = #typ_name<#(#generics_owned),*>;
            fn into_owned(self) -> Self::Owned {
                #typ_name{
                    #(#fields_owned,)*
                }
            }
        }
    }
}
