use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse::Parse, GenericParam, ItemStruct, Lifetime};

use crate::make_generic;

struct CommonInfo {
    name: syn::Ident,
    dummy_name: syn::Ident,
    original_generics: Vec<Lifetime>,
    fields: Vec<(syn::Ident, syn::Type)>,
}

impl CommonInfo {
    fn from_item(item: ItemStruct) -> syn::Result<Self> {
        let name = item.ident;
        let dummy_name = format_ident!("{name}Dummy");
        let original_generics = item.generics.params.into_iter().map(|x| {
            let GenericParam::Lifetime(lt) = x else {
                return Err(syn::Error::new_spanned(
                    x,
                    "Only lifetime generics are supported.",
                ));
            };
            Ok(lt.lifetime)
        });
        let fields = item.fields.into_iter().map(|field| {
            let Some(name) = field.ident else {
                return Err(syn::Error::new_spanned(
                    field,
                    "Tuple structs are not supported (yet).",
                ));
            };
            Ok((name, field.ty))
        });
        Ok(Self {
            name,
            dummy_name,
            original_generics: original_generics.collect::<Result<_, _>>()?,
            fields: fields.collect::<Result<_, _>>()?,
        })
    }
}

pub fn from_row_impl(item: ItemStruct) -> syn::Result<TokenStream> {
    let mut trivial = None;
    for attr in &item.attrs {
        if attr.path().is_ident("trivial") {
            if trivial
                .replace(attr.parse_args_with(syn::Path::parse)?)
                .is_some()
            {
                return Err(syn::Error::new_spanned(
                    attr,
                    "Can not have multiple `trivial` attributes.",
                ));
            }
        }
    }

    let CommonInfo {
        name,
        dummy_name,
        original_generics,
        fields,
    } = CommonInfo::from_item(item)?;

    let mut defs = vec![];
    let mut generics = vec![];
    let mut constraints = vec![];
    let mut constraints_prepared = vec![];
    let mut prepared_typ = vec![];
    let mut prepared = vec![];
    let mut inits = vec![];
    for (name, typ) in &fields {
        let generic = make_generic(name);

        defs.push(quote! {#name: #generic});
        constraints.push(quote! {#generic: ::rust_query::private::Dummy<'_t, '_a, S, Out = #typ>});
        constraints_prepared
            .push(quote! {#generic: ::rust_query::private::Prepared<'_i, '_a, Out = #typ>});
        prepared_typ.push(quote! {#generic::Prepared<'_i>});
        generics.push(generic);
        prepared.push(quote! {#name: ::rust_query::private::Dummy::prepare(self.#name, cacher)});
        inits.push(quote! {#name: self.#name.call(row)});
    }

    let trivial = trivial.map(|trivial| {
        let schema = quote! {<#trivial as ::rust_query::Table>::Schema};

        let mut trivial_types = vec![];
        let mut trivial_prepared = vec![];
        for (name, typ) in fields {
            trivial_types.push(
                quote! {<#typ as ::rust_query::private::FromColumn<'_a, #schema>>::Prepared<'_i>},
            );
            trivial_prepared
            .push(quote! {#name: <#typ as ::rust_query::private::FromColumn<#schema>>::prepare(col.#name(), cacher)}); 
        }
        quote! {
            impl<'_a #(,#original_generics)*> ::rust_query::private::FromColumn<'_a, #schema> for #name<#(#original_generics),*> {
                type From = #trivial;
                type Prepared<'_i> = #dummy_name<#(#trivial_types),*>;
    
                fn prepare<'_i, '_t>(
                    col: ::rust_query::Column<'_t, #schema, Self::From>,
                    cacher: &mut ::rust_query::private::Cacher<'_t, '_i, #schema>,
                ) -> Self::Prepared<'_i> {
                    #dummy_name {
                        #(#trivial_prepared,)*
                    }
                }
            }
        }
    });

    Ok(quote! {
        struct #dummy_name<#(#generics),*> {
            #(#defs),*
        }

        impl<'_i, '_a #(,#original_generics)* #(,#constraints_prepared)*> ::rust_query::private::Prepared<'_i, '_a> for #dummy_name<#(#generics),*> {
            type Out = #name<#(#original_generics),*>;

            fn call(&mut self, row: ::rust_query::private::Row<'_, '_i, '_a>) -> Self::Out {
                #name {
                    #(#inits,)*
                }
            }
        }


        impl<'_t, '_a #(,#original_generics)*, S #(,#constraints)*> ::rust_query::private::Dummy<'_t, '_a, S> for #dummy_name<#(#generics),*> {
            type Out = #name<#(#original_generics),*>;
            type Prepared<'_i> = #dummy_name<#(#prepared_typ),*>;

            fn prepare<'_i>(self, mut cacher: &mut ::rust_query::private::Cacher<'_t, '_i, S>) -> Self::Prepared<'_i> {
                #dummy_name {
                    #(#prepared,)*
                }
            }
        }

        #trivial
    })
}
