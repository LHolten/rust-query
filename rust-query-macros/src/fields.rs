use std::collections::HashMap;

use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::{parse::Parse, punctuated::Punctuated, Ident, LitInt, Token};

struct Field {
    name: Ident,
    typ: Option<(Token![as], TokenStream)>,
}

impl Parse for Field {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(Self {
            name: input.parse()?,
            typ: input
                .peek(Token![as])
                .then(|| Ok::<_, syn::Error>((input.parse()?, input.parse()?)))
                .transpose()?,
        })
    }
}

pub struct Spec {
    struct_id: LitInt,
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
            struct_id: input.parse()?,
            _brace_token1: syn::braced!(content1 in input),
            required: content1.parse_terminated(Field::parse, Token![,])?,
            _brace_token2: syn::braced!(content2 in input),
            all: content2.parse_terminated(Ident::parse, Token![,])?,
        })
    }
}

pub fn generate(spec: Spec) -> syn::Result<TokenStream> {
    let mut m = HashMap::new();
    for r in &spec.required {
        if m.insert(r.name.clone(), r.typ.clone()).is_some() {
            return Err(syn::Error::new_spanned(&r.name, "duplicate name"));
        }
    }

    let struct_id = spec.struct_id;

    let mut out_typs = vec![];
    for x in spec.all {
        if let Some(typ) = m.remove(&x) {
            if let Some((_, custom)) = typ {
                out_typs.push(quote! {::rust_query::private::Custom<#custom>});
            } else {
                out_typs.push(quote! {::rust_query::private::Native<'_>});
            }
        } else {
            out_typs.push(quote! {::rust_query::private::Ignore});
        }
    }

    if let Some(name) = m.keys().next() {
        return Err(syn::Error::new_spanned(name, "unknown field name"));
    }

    if let Some(first) = spec.required.first() {
        let span = first.name.span();
        let macro_root = quote_spanned! {span=> MacroRoot};
        Ok(
            quote! {<#macro_root as ::rust_query::private::Instantiate<#struct_id, (#(#out_typs),*)>>::Out},
        )
    } else {
        Ok(quote! {()})
    }
}
