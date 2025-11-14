use proc_macro2::TokenStream;
use quote::{format_ident, quote, quote_spanned, ToTokens};
use syn::{spanned::Spanned, GenericParam, ItemStruct, Lifetime};

use crate::make_generic;

struct CommonInfo {
    name: syn::Ident,
    original_generics: Vec<Lifetime>,
    fields: Vec<(syn::Ident, syn::Type)>,
}

impl CommonInfo {
    fn from_item(item: ItemStruct) -> syn::Result<Self> {
        let name = item.ident;
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
            original_generics: original_generics.collect::<Result<_, _>>()?,
            fields: fields.collect::<Result<_, _>>()?,
        })
    }
}

pub fn wrap(parts: &[impl ToTokens]) -> TokenStream {
    match parts {
        [] => quote! {()},
        [typ] => typ.to_token_stream(),
        [a, b @ ..] => {
            let rest = wrap(b);
            quote! {(#a, #rest)}
        }
    }
}

pub fn dummy_impl(item: ItemStruct) -> syn::Result<TokenStream> {
    let CommonInfo {
        name,
        original_generics,
        fields,
    } = CommonInfo::from_item(item)?;
    let dummy_name = format_ident!("{name}Select");

    let mut generics = vec![];
    let mut dummies = vec![];
    let mut typs = vec![];
    let mut names = vec![];
    for (name, typ) in &fields {
        let generic = make_generic(name);

        generics.push(quote! {#generic});
        dummies.push(quote! {self.#name});
        names.push(quote! {#name});
        typs.push(quote! {#typ});
    }

    let parts_name = wrap(&names);
    let parts_dummies = wrap(&dummies);

    Ok(quote! {
        struct #dummy_name<#(#generics),*> {
            #(#names: #generics),*
        }

        impl<'_t #(,#original_generics)*, S
            #(,#generics: ::rust_query::IntoSelect<'_t, S, Out = #typs>)*> ::rust_query::IntoSelect<'_t, S> for #dummy_name<#(#generics),*>
        where #name<#(#original_generics),*>: 'static {
            type Out = (#name<#(#original_generics),*>);

            fn into_select(self) -> ::rust_query::Select<'_t, S, Self::Out> {
                ::rust_query::IntoSelect::into_select(#parts_dummies).map(|#parts_name| #name {
                    #(#names,)*
                })
            }
        }

    })
}

pub fn from_expr(item: ItemStruct) -> syn::Result<TokenStream> {
    let mut trivial = vec![];
    for attr in &item.attrs {
        if attr.path().is_ident("rust_query") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("From") {
                    let path: syn::Path = meta.value()?.parse()?;
                    trivial.push(path);
                    return Ok(());
                }
                Err(meta.error("unrecognized rust-query attribute"))
            })?;
        }
    }

    let CommonInfo {
        name,
        original_generics,
        fields,
    } = CommonInfo::from_item(item)?;

    let mut names = vec![];
    for (name, _) in &fields {
        names.push(quote! {#name});
    }

    let trivial = trivial.into_iter().map(|trivial| {
        let schema = quote! {<#trivial as ::rust_query::Table>::Schema};
        let mut trivial_prepared = vec![];
        for (name, typ) in &fields {
            let span = typ.span();
            trivial_prepared
                .push(quote_spanned! {span=> <#typ as ::rust_query::FromExpr<_, _>>::from_expr(&col.#name)});
        }
        let parts_dummies = wrap(&trivial_prepared);
        let parts_name = wrap(&names);

        quote! {
            impl<#(#original_generics),*> ::rust_query::FromExpr<#schema, #trivial> for #name<#(#original_generics),*>
            {
                fn from_expr<'_t>(col: impl ::rust_query::IntoExpr<'_t, #schema, Typ = #trivial>) -> ::rust_query::Select<'_t, #schema, Self> {
                    let col = ::rust_query::IntoExpr::into_expr(col);
                    ::rust_query::IntoSelect::into_select(#parts_dummies).map(|#parts_name| #name {
                        #(#names,)*
                    })
                }
            }
        }
    });

    Ok(quote! {
        #(#trivial)*
    })
}
