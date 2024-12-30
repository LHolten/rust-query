use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{GenericParam, ItemStruct};

use crate::make_generic;

pub fn from_row_impl(item: ItemStruct) -> syn::Result<TokenStream> {
    let name = item.ident;
    let dummy_name = format_ident!("{name}Dummy");
    let original_generics = item.generics.params.iter().map(|x| {
        let GenericParam::Lifetime(lt) = x else {
            return Err(syn::Error::new_spanned(
                x,
                "Only lifetime generics are supported.",
            ));
        };
        Ok(lt.lifetime.clone())
    });
    let original_generics: Vec<_> = original_generics.collect::<Result<_, _>>()?;

    let mut defs = vec![];
    let mut generics = vec![];
    let mut constraints = vec![];
    let mut prepared = vec![];
    let mut inits = vec![];
    for field in item.fields {
        let Some(name) = field.ident else {
            return Err(syn::Error::new_spanned(
                field,
                "Tuple structs are not supported (yet).",
            ));
        };
        let name_prepared = format_ident!("{name}_prepared");
        let generic = make_generic(&name);
        let typ = field.ty;

        defs.push(quote! {#name: #generic});
        constraints.push(quote! {#generic: ::rust_query::private::Dummy<'_t, '_a, S, Out = #typ>});
        generics.push(generic);
        prepared.push(quote! {let mut #name_prepared = ::rust_query::private::Dummy::prepare(self.#name, cacher)});
        inits.push(quote! {#name: #name_prepared.call(row).0});
    }

    Ok(quote! {
        struct #dummy_name<#(#generics),*> {
            #(#defs),*
        }

        impl<'_t, '_a #(,#original_generics)*, S #(,#constraints)*> ::rust_query::private::Dummy<'_t, '_a, S> for #dummy_name<#(#generics),*> {
            type Out = #name<#(#original_generics),*>;

            fn prepare<'_i>(self, mut cacher: &mut ::rust_query::private::Cacher<'_t, '_i, S>) ->
                    ::rust_query::private::Prepared<'_i, '_a, Self::Out> {
                #(#prepared;)*
                ::rust_query::private::Prepared::new(move |row| ::rust_query::private::Wrapped::new(#name {
                    #(#inits,)*
                }))
            }
        }
    })
}
