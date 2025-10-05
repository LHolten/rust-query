use std::collections::BTreeMap;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::Ident;

use crate::SingleVersionTable;

impl SingleVersionTable {
    pub fn make_unique_tree(&self) -> UniqueTree {
        let mut res = UniqueTree::default();
        for unique in &self.uniques {
            res.add_unique(&unique.columns);
        }
        res
    }
}

// All the possible orderings of columns that give a unique row
#[derive(Default)]
pub struct UniqueTree {
    pub is_unique: bool,
    pub choice: BTreeMap<Ident, UniqueTree>,
}

impl UniqueTree {
    pub fn add_unique(&mut self, new: &[Ident]) {
        match new {
            [] => self.is_unique = true,
            [x, xs @ ..] => {
                self.choice.entry(x.clone()).or_default().add_unique(xs);
            }
        }
    }
}

pub struct Info {
    table: Ident,
    schema: Ident,
    // maps column name to type
    typs: BTreeMap<Ident, Ident>,
}

impl SingleVersionTable {
    pub fn make_info(&self, schema: Ident) -> Info {
        let mut typs = BTreeMap::new();
        let table = &self.name;
        for (i, x) in &self.columns {
            let tmp = format_ident!("_{table}{i}");
            typs.insert(x.name.clone(), tmp);
        }
        Info {
            table: self.name.clone(),
            schema,
            typs,
        }
    }
}

pub fn unique_tree(
    prefix: &Ident,
    prefix_lt: bool,
    tree: &UniqueTree,
    info: &Info,
) -> syn::Result<TokenStream> {
    let schema = &info.schema;
    let table = &info.table;

    let mut out = TokenStream::new();
    for (col, next) in &tree.choice {
        let col_typ = info.typs.get(col).ok_or(syn::Error::new_spanned(
            col,
            "Expected a column to exists for every name in the unique constraint.",
        ))?;
        let helper_name = format_ident!("{prefix}_{col}");
        let col_str = col.to_string();

        let anti_lt = (!prefix_lt).then_some(quote! {'inner}).unwrap_or_default();
        let prefix_lt = prefix_lt.then_some(quote! {'inner}).unwrap_or_default();

        out.extend(quote! {
            pub struct #helper_name<'inner>(#prefix<#prefix_lt>, ::rust_query::Expr<'inner, #schema, #col_typ>);

            impl<'inner> ::rust_query::private::Joinable<'inner> for #helper_name<'inner> {
                type Typ = #table;
                fn conds(self) -> ::std::vec::Vec<(&'static str, ::rust_query::private::DynTypedExpr)> {
                    let mut list = self.0.conds();
                    list.push((#col_str, ::rust_query::private::DynTypedExpr::erase(self.1)));
                    list
                }
            }
        });

        if next.is_unique {
            out.extend(quote! {
                impl<#prefix_lt> #prefix<#prefix_lt> {
                    pub fn #col<#anti_lt>(self, val: impl ::rust_query::IntoExpr<'inner, #schema, Typ = #col_typ>)
                        -> ::rust_query::Expr<'inner, #schema, Option<#table>>
                    {
                        ::rust_query::private::unique_from_joinable(#helper_name(self, val.into_expr()))
                    }
                }
            });
            continue;
        } else {
            out.extend(unique_tree(&helper_name, true, next, info));

            out.extend(quote! {
                impl<#prefix_lt> #prefix<#prefix_lt> {
                    pub fn #col<#anti_lt>(self, val: impl ::rust_query::IntoExpr<'inner, #schema, Typ = #col_typ>) -> #helper_name<'inner> {
                        #helper_name(self, val.into_expr())
                    }
                }
            });
        }
    }
    Ok(out)
}
