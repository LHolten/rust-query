use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{GenericParam, ItemStruct, Lifetime};

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

pub fn wrap(parts: &[TokenStream]) -> TokenStream {
    match parts {
        [] => quote! {()},
        [typ] => typ.clone(),
        [a, b @ ..] => {
            let rest = wrap(b);
            quote! {(#a, #rest)}
        }
    }
}

pub fn from_row_impl(item: ItemStruct) -> syn::Result<TokenStream> {
    let mut trivial = vec![];
    let mut transaction_lt = None;
    for attr in &item.attrs {
        if attr.path().is_ident("rust_query") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("From") {
                    let path: syn::Path = meta.value()?.parse()?;
                    trivial.push(path);
                    return Ok(());
                }
                if meta.path.is_ident("lt") {
                    let lt: syn::Lifetime = meta.value()?.parse()?;
                    if transaction_lt.replace(lt).is_some() {
                        return Err(syn::Error::new_spanned(
                            attr,
                            "Can not have multiple transaction lifetimes",
                        ));
                    }
                    return Ok(());
                }
                Err(meta.error("unrecognized rust-query attribute"))
            })?;
        }
        if attr.path().is_ident("trivial") {}
    }

    let CommonInfo {
        name,
        dummy_name,
        original_generics,
        fields,
    } = CommonInfo::from_item(item)?;

    let mut original_plus_transaction = original_generics.clone();
    let builtin_lt = syn::Lifetime::new("'_a", Span::mixed_site());
    if transaction_lt.is_none() {
        original_plus_transaction.push(builtin_lt.clone());
    }
    let transaction_lt = transaction_lt.unwrap_or(builtin_lt);

    let mut defs = vec![];
    let mut generics = vec![];
    let mut into_impl = vec![];
    let mut constraints = vec![];
    let mut dummies = vec![];
    let mut typs = vec![];
    let mut names = vec![];
    let mut from_impl = vec![];
    let mut from_conds = vec![];
    for (name, typ) in &fields {
        let generic = make_generic(name);

        defs.push(quote! {#name: #generic});
        constraints
            .push(quote! {#generic: ::rust_query::IntoDummy<'_t, #transaction_lt, S, Out = #typ>});
        generics.push(quote! {#generic});
        into_impl.push(quote! {#generic::Impl});
        dummies.push(quote! {self.#name});
        names.push(quote! {#name});
        typs.push(quote! {#typ});
        from_impl.push(quote! {<#typ as ::rust_query::dummy::FromDummy<#transaction_lt>>::Impl});
        from_conds.push(quote! {#typ: ::rust_query::dummy::FromDummy<#transaction_lt>});
    }

    let trivial = trivial.into_iter().map(|trivial| {
        let schema = quote! {<#trivial as ::rust_query::Table>::Schema};
        let mut trivial_prepared = vec![];
        for (name, typ) in &fields {
            trivial_prepared
                .push(quote! {#name: <#typ as ::rust_query::dummy::FromColumn<_, _>>::from_column(col.#name())});
        }
        quote! {
            impl<#(#original_plus_transaction),*> ::rust_query::dummy::FromColumn<#transaction_lt, #schema, #trivial> for #name<#(#original_generics),*>
            {
                fn from_column<'_t>(col: ::rust_query::Column<'_t, #schema, #trivial>) -> ::rust_query::Dummy<'_t, #schema, Self::Impl> {
                    ::rust_query::IntoDummy::into_dummy(#dummy_name {
                        #(#trivial_prepared,)*
                    })
                }
            }
        }
    });

    let parts_typ = wrap(&typs);
    let parts_name = wrap(&names);
    let parts_dummies = wrap(&dummies);
    let parts_from_impl = wrap(&from_impl);
    let parts_into_impl = wrap(&into_impl);

    Ok(quote! {
        struct #dummy_name<#(#generics),*> {
            #(#defs),*
        }

        impl<'_t #(,#original_plus_transaction)*, S #(,#constraints)*> ::rust_query::IntoDummy<'_t, #transaction_lt, S> for #dummy_name<#(#generics),*> {
            type Out = #name<#(#original_generics),*>;
            type Impl = ::rust_query::dummy::MapImpl<#parts_into_impl, fn(#parts_typ) -> Self::Out>;

            fn into_dummy(self) -> ::rust_query::Dummy<'_t, S, Self::Impl> {
                ::rust_query::IntoDummy::into_dummy(::rust_query::IntoDummy::map_dummy(#parts_dummies, (|#parts_name| #name {
                    #(#names,)*
                }) as fn(#parts_typ) -> Self::Out))
            }
        }

        impl<#(#original_plus_transaction),*> ::rust_query::dummy::FromDummy<#transaction_lt> for #name<#(#original_generics),*>
        where #(#from_conds,)*
        {
            type Impl = ::rust_query::dummy::MapImpl<#parts_from_impl, fn(#parts_typ) -> Self>;
        }

        #(#trivial)*
    })
}
