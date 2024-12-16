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
        inits.push(quote! {#name: (#name_prepared)(row)});
    }

    Ok(quote! {
        struct #dummy_name<#(#generics),*> {
            #(#defs),*
        }

        impl<'_t, '_a #(,#original_generics)*, S #(,#constraints)*> ::rust_query::private::Dummy<'_t, '_a, S> for #dummy_name<#(#generics),*> {
            type Out = #name<#(#original_generics),*>;

            fn prepare(self, mut cacher: ::rust_query::private::Cacher<'_, '_t, S>) -> impl FnMut(::rust_query::private::Row<'_, '_t, '_a>) -> Self::Out + '_t {
                #(#prepared;)*
                move |row| #name {
                    #(#inits,)*
                }
            }
        }
    })
}
