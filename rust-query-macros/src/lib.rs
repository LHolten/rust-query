
use heck::{ToSnekCase, ToUpperCamelCase};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Attribute, Ident, ItemEnum, ItemStruct, Token, Type};

#[proc_macro_attribute]
pub fn schema(
    attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    assert!(attr.is_empty());
    let item = syn::parse_macro_input!(item as ItemEnum);

    match generate(item) {
        Ok(x) => {x},
        Err(e) => {e.into_compile_error()},
    }.into()
}

#[derive(Clone)]
struct Column {
    name: Ident,
    typ: Type,
}


struct Range {
    start: u32,
    end: Option<u32>
}

impl Range {
    pub fn includes(&self, idx: u32) -> bool {
        if idx < self.start {
            return false
        }
        if let Some(end) = self.end {
            return idx < end
        }
        true
    }
}


impl syn::parse::Parse for Range {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let start: Option<syn::LitInt> = input.parse()?;
        let _: Token![..] = input.parse()?;
        let end: Option<syn::LitInt> = input.parse()?;

        let res = Range {
            start: start.map(|x|x.base10_parse().unwrap()).unwrap_or_default(),
            end: end.map(|x|x.base10_parse().unwrap()),
        };
        Ok(res)
    }
}

fn parse_version(attrs: &[Attribute]) -> syn::Result<Range> {
    if attrs.is_empty() {
        return Ok(Range {start:0, end: None})
    }
    let [versions] = attrs else {
        panic!()
    };
    assert!(versions.path().is_ident("version"));
    versions.parse_args()
}

fn generate(item: ItemEnum) -> syn::Result<TokenStream> {
    let range = parse_version(&item.attrs)?;
    let schema = &item.ident ;

    let mut output = TokenStream::new();
    for version in range.start..range.end.unwrap() {
        
        let mut mod_output = TokenStream::new();
        for table in &item.variants {
            let columns: Vec<_> = table
                .fields
                .iter()
                .filter_map(|field| {
                    let range = parse_version(&field.attrs).unwrap();
                    range.includes(version).then_some(Column {
                        name: field.ident.clone().unwrap(),
                        typ: field.ty.clone(),
                    })})
                .collect();
        
            let table_ident = &table.ident;
        
            let make_generic = |name: &Ident| {
                let normalized = name.to_string().to_upper_camel_case();
                format_ident!("_{normalized}")
            };
        
            let defs = columns.iter().map(|col| {
                let ident = &col.name;
                let generic = make_generic(&col.name);
                quote!(pub #ident: #generic)
            });
        
            let typs = columns.iter().map(|col| {
                let typ = &col.typ;
                quote!(::rust_query::value::Db<'t, #typ>)
            });
        
            let typ_asserts = columns.iter().map(|col| {
                let typ = &col.typ;
                quote!(::rust_query::valid_in_schema::<#schema, #typ>();)
            });
        
            let generics = columns.iter().map(|col| {
                let generic = make_generic(&col.name);
                quote!(#generic)
            });
        
            let generics_defs = columns.iter().map(|col| {
                let generic = make_generic(&col.name);
                quote!(#generic)
            });
        
            let read_bounds = columns.iter().map(|col| {
                let typ = &col.typ;
                let generic = make_generic(&col.name);
                quote!(#generic: ::rust_query::value::Value<'t, Typ=#typ>)
            });
        
            let inits = columns.iter().map(|col| {
                let ident = &col.name;
                let name: &String = &col.name.to_string();
                quote!(#ident: f.col(#name))
            });
        
            let reads = columns.iter().map(|col| {
                let ident = &col.name;
                let name: &String = &col.name.to_string();
                quote!(f.col(#name, self.#ident))
            });
        
            let dummy_ident = format_ident!("{}Dummy", table_ident);
        
            let table_name: &String = &table_ident.to_string();
            let name: &String = &format!("{table_ident}Id");
            let has_id = quote!(
                impl ::rust_query::HasId for #table_ident {
                    const ID: &'static str = #name;
                    const NAME: &'static str = #table_name;
                }
            );
        
            mod_output.extend(quote! {
                pub struct #table_ident;
        
                pub struct #dummy_ident<#(#generics_defs),*> {
                    #(#defs,)*
                }
        
                impl ::rust_query::Table for #table_ident {
                    type Dummy<'t> = #dummy_ident<#(#typs),*>;
                    type Schema = #schema;
        
                    fn name(&self) -> String {
                        #table_name.to_owned()
                    }
        
                    fn build(f: ::rust_query::Builder<'_>) -> Self::Dummy<'_> {
                        #dummy_ident {
                            #(#inits,)*
                        }
                    }
                }
        
                impl<'t, #(#read_bounds),*> ::rust_query::insert::Writable<'t> for #dummy_ident<#(#generics),*> {
                    type T = #table_ident;
                    fn read(self, f: ::rust_query::insert::Reader<'t>) {
                        #(#reads;)*
                    }
                }
        
                // impl ::rust_query::ValidInSchema<#schema> for #table_ident {}
        
                const _: fn() = || {
                    #(#typ_asserts)*
                };
        
                #has_id
            });
    
        }

        let mod_ident = format_ident!("v{version}");
    
        output.extend(quote! {
            pub mod #mod_ident {
                pub struct #schema (());

                #mod_output
            }
        });
    
    }


    Ok(output)
    // }
}

// #[proc_macro_attribute]
// pub fn schema(
//     attr: proc_macro::TokenStream,
//     item: proc_macro::TokenStream,
// ) -> proc_macro::TokenStream {
//     assert!(attr.is_empty());
//     let item = syn::parse_macro_input!(item as Const);

//     generate(item).into()
// }
