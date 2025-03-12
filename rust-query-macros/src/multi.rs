use std::collections::BTreeMap;

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Token};

#[derive(Clone)]
pub(crate) struct Unique {
    pub name: Ident,
    pub columns: Vec<Ident>,
}

#[derive(Clone)]
pub(crate) enum ColumnTyp {
    Normal(TokenStream),
    Reference(Reference),
}

#[derive(Clone)]
pub(crate) struct Reference {
    pub inner: Ident,
    pub wrap: Option<(Ident, Token![<], Token![>])>,
}

impl Reference {
    fn wrap(&self, expr: TokenStream) -> TokenStream {
        if let Some((first, open, close)) = &self.wrap {
            quote! {
                #first #open #expr #close
            }
        } else {
            expr
        }
    }
}

pub(crate) struct VersionedSchema {
    pub versions: std::ops::Range<u32>,
    pub tables: Vec<VersionedTable>,
}

// This is a table fully parsed from the schema, it represents multiple versions
pub(crate) struct VersionedTable {
    pub name: Ident,
    pub versions: std::ops::Range<u32>,
    pub prev: Option<Ident>,
    pub uniques: Vec<Unique>,
    pub columns: Vec<VersionedColumn>,
    pub referenceable: bool,
}

pub(crate) struct VersionedColumn {
    pub versions: std::ops::Range<u32>,
    pub name: Ident,
    pub typ: ColumnTyp,
}

impl VersionedSchema {
    pub fn get(&self, version: u32) -> syn::Result<BTreeMap<usize, SingleVersionTable>> {
        assert!(self.versions.contains(&version));
        let mut tables = BTreeMap::new();
        for (i, t) in self.tables.iter().enumerate() {
            if t.versions.contains(&version) {
                tables.insert(i, self.get_table(t, version)?);
            }
        }
        Ok(tables)
    }

    fn get_table(&self, table: &VersionedTable, version: u32) -> syn::Result<SingleVersionTable> {
        assert!(table.versions.contains(&version));
        let mut columns = BTreeMap::new();
        for (i, c) in table.columns.iter().enumerate() {
            if c.versions.contains(&version) {
                let typ = match &c.typ {
                    ColumnTyp::Normal(typ) => typ.clone(),
                    ColumnTyp::Reference(reference) => {
                        let table = self.resolve(&reference.inner, version, c.versions.end - 1)?;
                        reference.wrap(quote! {#table})
                    }
                };
                columns.insert(
                    i,
                    SingleVersionColumn {
                        name: c.name.clone(),
                        typ,
                    },
                );
            }
        }
        let mut prev = Some(table.name.clone());
        if version == table.versions.start {
            prev = table.prev.clone();
        }
        // let is_referee = self.tables.iter().flat_map(|t| &t.columns).any(|c| {
        //     c.versions.contains(&version)
        //         && matches!(&c.typ, ColumnTyp::Reference(x) if x.inner == table.name)
        // });

        Ok(SingleVersionTable {
            prev,
            name: table.name.clone(),
            uniques: table.uniques.clone(),
            columns,
            referenceable: table.referenceable,
        })
    }

    fn resolve(&self, name: &Ident, version: u32, latest: u32) -> syn::Result<&Ident> {
        assert!(version <= latest);
        let Some(table) = self
            .tables
            .iter()
            .find(|x| &x.name == name && x.versions.contains(&latest))
        else {
            return Err(syn::Error::new_spanned(
                name,
                format!("{name} does not exist in version {latest}"),
            ));
        };

        if table.versions.contains(&version) {
            Ok(&table.name)
        } else if let Some(prev) = &table.prev {
            self.resolve(prev, version, table.versions.start - 1)
        } else {
            Err(syn::Error::new_spanned(
                name,
                format!(
                    "expected {name} to have a `from` attribute with a table that exists in version {version}"
                ),
            ))
        }
    }
}

pub(crate) struct SingleVersionTable {
    pub prev: Option<Ident>,
    pub name: Ident,
    pub uniques: Vec<Unique>,
    pub columns: BTreeMap<usize, SingleVersionColumn>,
    pub referenceable: bool,
}

pub(crate) struct SingleVersionColumn {
    pub name: Ident,
    pub typ: TokenStream,
}
