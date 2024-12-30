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
    let mut constraints_prepared = vec![];
    let mut prepared_typ = vec![];
    let mut prepared = vec![];
    let mut inits = vec![];
    for field in item.fields {
        let Some(name) = field.ident else {
            return Err(syn::Error::new_spanned(
                field,
                "Tuple structs are not supported (yet).",
            ));
        };
        let generic = make_generic(&name);
        let typ = field.ty;

        defs.push(quote! {#name: #generic});
        constraints.push(quote! {#generic: ::rust_query::private::Dummy<'_t, '_a, S, Out = #typ>});
        constraints_prepared
            .push(quote! {#generic: ::rust_query::private::Prepared<'_i, '_a, Out = #typ>});
        prepared_typ.push(quote! {#generic::Prepared<'_i>});
        generics.push(generic);
        prepared.push(quote! {#name: ::rust_query::private::Dummy::prepare(self.#name, cacher)});
        inits.push(quote! {#name: self.#name.call(row)});
    }

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
    })
}
