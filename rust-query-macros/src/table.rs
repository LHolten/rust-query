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
            inits.push(quote! {
                #col: ::rust_query::private::into_owned(#col)
            });
        }

        unique_typs.push(quote! {f.unique(&[#(#column_strs),*])});

        unique_funcs.push(quote! {
            pub fn #unique_name<'a #(,#constraints)*>(#(#args),*) -> ::rust_query::Column<'a, #schema, Option<#table_ident>> {
                ::rust_query::private::new_column(#table_mod::#unique_type {
                    #(#inits),*
                })
            }
        });
        unique_defs.push(define_unique(unique, table_ident, schema));
    }

    let (conflict_type, conflict_dummy, conflict_dummy_insert) = match &*table.uniques {
        [] => (
            quote! {::std::convert::Infallible},
            quote! {
                let x = ::rust_query::IntoColumn::into_column(&0i64);
                ::rust_query::IntoDummy::map_dummy(x, |_| unreachable!())
            },
            quote! {
                let x = ::rust_query::IntoColumn::into_column(&0i64);
                ::rust_query::IntoDummy::map_dummy(x, |_| unreachable!())
            },
        ),
        [unique] => {
            let unique_name = &unique.name;
            let mut parts = vec![];
            let mut parts_insert = vec![];
            for field in &unique.columns {
                parts.push(quote! {&self.#field.apply(old.#field())});
                parts_insert.push(quote! {&self.#field});
            }
            (
                quote! {::rust_query::TableRow<'t, #table_ident>},
                quote! {
                    #table_ident::#unique_name(#(#parts),*)
                },
                quote! {
                    #table_ident::#unique_name(#(#parts_insert),*)
                },
            )
        }
        _ => (
            quote! {()},
            quote! {
                let x = ::rust_query::IntoColumn::into_column(&0i64);
                ::rust_query::IntoDummy::map_dummy(x, |_| Some(()))
            },
            quote! {
                let x = ::rust_query::IntoColumn::into_column(&0i64);
                ::rust_query::IntoDummy::map_dummy(x, |_| Some(()))
            },
        ),
    };

    let mut defs = vec![];
    let mut reads = vec![];
    let mut read_insert = vec![];
    let mut def_typs = vec![];
    let mut col_defs = vec![];
    let mut generic_defaults = vec![];
    let mut update_columns = vec![];
    let mut update_columns_safe = vec![];
    let mut dummy_inits = vec![];
    let mut reads_safe = vec![];
    let mut insert_bound = vec![];
    let mut generics = vec![];

    for col in table.columns.values() {
        let typ = &col.typ;
        let ident = &col.name;
        let ident_str = ident.to_string();
        let generic = make_generic(ident);
        defs.push(quote! {
            pub fn #ident(&self) -> ::rust_query::Column<'t, #schema, #typ> {
                ::rust_query::private::new_column((::rust_query::private::Col::new(#ident_str, ::rust_query::private::into_owned(&self.0))))
            }
        });
        reads.push(quote!(f.col(#ident_str, &self.#ident.apply(old.#ident()))));
        read_insert.push(quote!(f.col(#ident_str, &self.#ident)));
        def_typs.push(quote!(f.col::<#typ>(#ident_str)));
        let mut unique_columns = table.uniques.iter().flat_map(|x| &x.columns);
        if unique_columns.any(|x| x == ident) {
            def_typs.push(quote!(f.check_unique_compatible::<#typ>()));
            update_columns_safe.push(quote! {()});
        } else {
            reads_safe.push(quote!(f.col(#ident_str, &self.#ident.apply(old.#ident()))));
            update_columns_safe.push(quote! {::rust_query::Update<'t, #schema, #typ>});
        }
        col_defs.push(quote! {pub #ident: #generic});
        dummy_inits.push(quote! {#ident: Default::default()});
        generic_defaults.push(quote! {#generic = ()});
        update_columns.push(quote! {::rust_query::Update<'t, #schema, #typ>});
        insert_bound.push(quote! {#generic: ::rust_query::IntoColumn<'t, #schema, Typ = #typ>});
        generics.push(generic);
    }

    let safe_writable = (!table.uniques.is_empty()).then_some(quote! {
        impl<'t> ::rust_query::private::TableConflict<'t> for #table_ident<#(#update_columns_safe),*> {
            type Schema = #schema;
            type T = #table_ident;
            type Conflict = ::std::convert::Infallible;
        }
        impl<'t> ::rust_query::private::TableUpdate<'t> for #table_ident<#(#update_columns_safe),*> {
            fn read(&self,
                old: ::rust_query::Column<'t, Self::Schema, Self::T>,
                f: ::rust_query::private::Reader<'_, 't, Self::Schema>
            ) {
                #(#reads_safe;)*
            }
            fn get_conflict_unchecked(&self, old: ::rust_query::Column<'t, Self::Schema, Self::T>)
                -> impl ::rust_query::IntoDummy<'t, 't, Self::Schema, Out = Option<Self::Conflict>>
            {
                let x = ::rust_query::IntoColumn::into_column(&0i64);
                ::rust_query::IntoDummy::map_dummy(x, |_| unreachable!())
            }
        }
    });

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

            type Update<'t> = #table_ident<#(#update_columns_safe),*>;
            type TryUpdate<'t> = #table_ident<#(#update_columns),*>;

            fn update<'t>() -> Self::Update<'t> {
                #table_ident {
                    #(#dummy_inits,)*
                }
            }
            fn try_update<'t>() -> Self::TryUpdate<'t> {
                #table_ident {
                    #(#dummy_inits,)*
                }
            }

            type Referer = #referer;
            fn get_referer_unchecked() -> Self::Referer {
                #referer_expr
            }
        }

        impl<'t #(, #insert_bound)*> ::rust_query::private::TableConflict<'t> for #table_ident<#(#generics),*> {
            type Schema = #schema;
            type T = #table_ident;
            type Conflict = #conflict_type;
        }
        impl<'t #(, #insert_bound)*> ::rust_query::private::TableInsert<'t> for #table_ident<#(#generics),*> {
            fn read(&self, f: ::rust_query::private::Reader<'_, 't, Self::Schema>) {
                #(#read_insert;)*
            }
            fn get_conflict_unchecked(&self) -> impl ::rust_query::IntoDummy<'t, 't, Self::Schema, Out = Option<Self::Conflict>>
            {
                #conflict_dummy_insert
            }
        }
        impl<'t> ::rust_query::private::TableConflict<'t> for #table_ident<#(#update_columns),*> {
            type Schema = #schema;
            type T = #table_ident;
            type Conflict = #conflict_type;
        }
        impl<'t> ::rust_query::private::TableUpdate<'t> for #table_ident<#(#update_columns),*> {
            fn read(&self,
                old: ::rust_query::Column<'t, Self::Schema, Self::T>,
                f: ::rust_query::private::Reader<'_, 't, Self::Schema>
            ) {
                #(#reads;)*
            }
            fn get_conflict_unchecked(&self, old: ::rust_query::Column<'t, Self::Schema, Self::T>)
                -> impl ::rust_query::IntoDummy<'t, 't, Self::Schema, Out = Option<Self::Conflict>>
            {
                #conflict_dummy
            }
        }
        #safe_writable

        #[allow(unused)]
        impl #table_ident {
            #(#unique_funcs)*
        }

        mod #table_mod {
            #(#unique_defs)*
        }
    })
}

fn define_unique(unique: &Unique, table_typ: &Ident, schema: &Ident) -> TokenStream {
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
                b.get_unique::<super::#table_typ>(vec![#(#conds),*])
            }
        }
    }
}
