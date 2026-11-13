use heck::ToSnakeCase;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{ext::IdentExt, ItemEnum};

pub fn expand_refinement(item: ItemEnum) -> Result<TokenStream, syn::Error> {
    let item_ident = item.ident;
    let mut variant_names = Vec::new();
    let mut variant_idents = Vec::new();
    for variant in item.variants {
        let name = variant.ident.unraw().to_string();
        variant_names.push(name.to_snake_case());
        variant_idents.push(variant.ident);
        if !variant.fields.is_empty() {
            return Err(syn::Error::new_spanned(
                variant.fields,
                "fields are not supported",
            ));
        }
    }

    let mut check = variant_names.clone();
    check.sort();
    let check: Vec<_> = check.iter().map(|x| format!("'{x}'")).collect();
    let check = check.join(", ");
    let check = format!("{{col}} IN ({check})");

    Ok(quote! {
        ::rust_query::private::refinement_typ!(
            #item_ident,
            ::std::string::String,
            |val| match val {#(
                #item_ident::#variant_idents => #variant_names,
            )*}
            .to_owned(),
            |value| match value {
                #(
                    #variant_names => #item_ident::#variant_idents,
                )*
                _ => unreachable!(),
            },
            |col| format!(#check),
        );
    })
}
