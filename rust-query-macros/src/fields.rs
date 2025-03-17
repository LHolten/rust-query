use std::collections::HashMap;

use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse::Parse, punctuated::Punctuated, Ident, Token};

struct Field {
    name: Ident,
    typ: Option<(Token![:], TokenStream)>,
}

impl Parse for Field {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(Self {
            name: input.parse()?,
            typ: input
                .peek(Token![:])
                .then(|| Ok::<_, syn::Error>((input.parse()?, input.parse()?)))
                .transpose()?,
        })
    }
}

pub struct Spec {
    path: syn::Path,
    _brace_token1: syn::token::Brace,
    required: Punctuated<Field, Token![,]>,
    _brace_token2: syn::token::Brace,
    all: Punctuated<Ident, Token![,]>,
}

impl Parse for Spec {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let content1;
        let content2;
        Ok(Spec {
            path: input.parse()?,
            _brace_token1: syn::braced!(content1 in input),
            required: content1.parse_terminated(Field::parse, Token![,])?,
            _brace_token2: syn::braced!(content2 in input),
            all: content2.parse_terminated(Ident::parse, Token![,])?,
        })
    }
}

pub fn generate(spec: Spec) -> syn::Result<TokenStream> {
    let mut m = HashMap::new();
    for r in spec.required {
        if m.insert(r.name.clone(), r.typ).is_some() {
            return Err(syn::Error::new_spanned(r.name, "duplicate name"));
        }
    }

    let path = spec.path;
    // let last = path
    //     .segments
    //     .last_mut()
    //     .expect("thenere should be at least one path segment");
    // let lt = match &mut last.arguments {
    //     syn::PathArguments::None => None,
    //     syn::PathArguments::AngleBracketed(args) => {
    //         if let Some(arg) = args.args.pop() {
    //             match arg.into_value() {
    //                 GenericArgument::Lifetime(lt) => {
    //                     if !args.args.is_empty() {
    //                         return Err(syn::Error::new_spanned(args, "only one argument allowed"));
    //                     }
    //                     Some(lt)
    //                 }
    //                 e @ _ => return Err(syn::Error::new_spanned(e, "only lifetime is allowed")),
    //             }
    //         } else {
    //             None
    //         }
    //     }
    //     syn::PathArguments::Parenthesized(args) => {
    //         return Err(syn::Error::new_spanned(args, "only lifetime is allowed"))
    //     }
    // };
    // last.arguments = syn::PathArguments::None;

    let mut out_typs = vec![];
    for x in spec.all {
        if let Some(typ) = m.remove(&x) {
            if let Some((_, custom)) = typ {
                out_typs.push(quote! {::rust_query::private::Custom<#custom>});
            } else {
                out_typs.push(quote! {::rust_query::private::Native<'t>});
            }
        } else {
            out_typs.push(quote! {::rust_query::private::Ignore});
        }
    }

    if let Some(name) = m.keys().next() {
        return Err(syn::Error::new_spanned(name, "unknown field name"));
    }

    Ok(quote! {#path<#(#out_typs),*>})
}
