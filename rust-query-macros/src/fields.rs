use std::collections::HashMap;

use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned};
use syn::{parse::Parse, punctuated::Punctuated, Ident, LitInt, Token};

use crate::{dummy::wrap, make_generic};

struct Field {
    name: Ident,
    lt: Option<(Token![<], syn::Lifetime, Token![>])>,
    typ: Option<(Token![as], syn::Type)>,
}

impl Parse for Field {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(Self {
            name: input.parse()?,
            lt: input
                .peek(Token![<])
                .then(|| Ok::<_, syn::Error>((input.parse()?, input.parse()?, input.parse()?)))
                .transpose()?,
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
    required_span: Span,
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
            required_span: content1.span(),
            required: content1.parse_terminated(Field::parse, Token![,])?,
            _brace_token2: syn::braced!(content2 in input),
            all: content2.parse_terminated(Ident::parse, Token![,])?,
        })
    }
}

pub fn generate(spec: Spec) -> syn::Result<TokenStream> {
    let struct_id = spec.struct_id;

    let col = quote! {col};

    let mut out_typ = vec![];
    let mut out_typ_inst = vec![];
    let mut out_name = vec![];
    let mut generic = vec![];
    let mut trivial_prepared = vec![];
    for r in &spec.required {
        let name = &r.name;
        let Some(num) = spec.all.iter().position(|x| x == name) else {
            return Err(syn::Error::new_spanned(name, "unknown field name"));
        };

        let value = quote! {<MacroRoot as ::rust_query::private::Field<#struct_id, #num>>::Out};
        if let Some((_, custom)) = &r.typ {
            out_typ_inst.push(quote! {#custom});
        } else {
            let lt = r.lt.as_ref().map(|x| x.1.clone());
            let lt = lt.unwrap_or(syn::Lifetime::new("'static", name.span()));
            out_typ_inst.push(quote! {<#value as ::rust_query::private::MyTyp>::Out<#lt>});
        };
        out_name.push(name.clone());
        let gen = make_generic(name);
        trivial_prepared
            .push(quote_spanned! {name.span()=> ::rust_query::FromExpr::from_expr(#col.#name())});
        generic.push(gen);
        out_typ.push(value);
    }

    let parts_dummies = wrap(&trivial_prepared);
    let parts_name = wrap(&out_name);

    Ok(quote! {
        <[(); {
            const IMPL_ID: usize = ::rust_query::private::file_line_col(file!(), line!(), column!());
            pub struct Tmp<#(#generic),*> {
                #(#out_name: #generic,)*
            }
            type From = <MacroRoot as ::rust_query::private::Instantiate<#struct_id>>::Out;
            type Schema = <From as ::rust_query::Table>::Schema;
            impl<'_t, #(#generic: ::rust_query::FromExpr<'_t, Schema, #out_typ>),*> ::rust_query::FromExpr<'_t, Schema, From> for Tmp<#(#generic),*>
            {
                fn from_expr<'columns>(col: impl ::rust_query::IntoExpr<'columns, Schema, Typ = From>) -> ::rust_query::Select<'columns, '_t, Schema, Self> {
                    let #col = ::rust_query::IntoExpr::into_expr(col);
                    ::rust_query::IntoSelectExt::map_select(#parts_dummies, |#parts_name| Self {
                        #(#out_name,)*
                    })
                }
            }

            #[allow(non_local_definitions)]
            impl<#(#generic),*> ::rust_query::private::Sneak<MacroRoot, (#(#generic),*)> for [(); IMPL_ID] {
                type Out = (Tmp<#(#generic),*>);
            }
            IMPL_ID
        }] as ::rust_query::private::Sneak<MacroRoot, (#(#out_typ_inst),*)>>::Out
    })
}
