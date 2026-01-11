use dummy::{dummy_impl, from_expr};
use heck::{ToSnekCase, ToUpperCamelCase};
use multi::{SingleVersionTable, VersionedSchema};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Ident, ItemMod, ItemStruct};
use table::define_all_tables;

mod dummy;
mod migrations;
mod multi;
mod parse;
mod table;
mod unique;

#[proc_macro_attribute]
pub fn schema(
    attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let name = syn::parse_macro_input!(attr as syn::Ident);
    let item = syn::parse_macro_input!(item as ItemMod);

    match generate(name, item) {
        Ok(x) => x,
        Err(e) => e.into_compile_error(),
    }
    .into()
}

#[proc_macro_derive(Select)]
pub fn from_row(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let item = syn::parse_macro_input!(item as ItemStruct);
    match dummy_impl(item) {
        Ok(x) => x,
        Err(e) => e.into_compile_error(),
    }
    .into()
}

#[proc_macro_derive(FromExpr, attributes(rust_query))]
pub fn from_expr_macro(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let item = syn::parse_macro_input!(item as ItemStruct);
    match from_expr(item) {
        Ok(x) => x,
        Err(e) => e.into_compile_error(),
    }
    .into()
}

fn make_generic(name: &Ident) -> Ident {
    let normalized = name.to_string().to_upper_camel_case();
    format_ident!("_{normalized}")
}

fn to_lower(name: &Ident) -> Ident {
    let normalized = name.to_string().to_snek_case();
    format_ident!("{normalized}")
}

fn generate(schema_name: Ident, item: syn::ItemMod) -> syn::Result<TokenStream> {
    let schema = VersionedSchema::parse(item)?;

    let mut output = quote! {};
    let mut prev_mod = None;

    let mut iter = schema
        .versions
        .clone()
        .map(|version| Ok((version, schema.get(version)?)))
        .collect::<syn::Result<Vec<_>>>()?
        .into_iter()
        .peekable();

    while let Some((version, mut new_tables)) = iter.next() {
        let next_mod = iter
            .peek()
            .map(|(peek_version, _)| format_ident!("v{peek_version}"));
        let mut mod_output =
            define_all_tables(&schema_name, &prev_mod, &next_mod, version, &mut new_tables)?;

        let new_mod = format_ident!("v{version}");

        if let Some((peek_version, peek_tables)) = iter.peek() {
            let peek_mod = format_ident!("v{peek_version}");
            let m = migrations::migrations(
                &schema_name,
                new_tables,
                peek_tables,
                quote! {super},
                quote! {super::super::#peek_mod},
            )?;
            mod_output.extend(quote! {
                pub mod migrate {
                    #m
                }
            });
        }

        output.extend(quote! {
            pub mod #new_mod {
                #mod_output
            }
        });

        prev_mod = Some(new_mod);
    }

    Ok(output)
}
