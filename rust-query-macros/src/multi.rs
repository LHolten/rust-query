use std::collections::BTreeMap;

use proc_macro2::{Span, TokenStream};
use quote::format_ident;
use syn::{Attribute, Ident};

#[derive(Clone)]
pub(crate) struct Unique {
    pub columns: Vec<Ident>,
}

pub(crate) struct VersionedSchema {
    pub versions: std::ops::Range<u32>,
    pub tables: Vec<VersionedTable>,
}

// This is a table fully parsed from the schema, it represents multiple versions
pub(crate) struct VersionedTable {
    pub name: Ident,
    pub versions: std::ops::Range<u32>,
    // `prev` always has a distinct span from `name`
    pub prev: Option<Ident>,
    pub uniques: Vec<Unique>,
    pub doc_comments: Vec<Attribute>,
    pub columns: Vec<VersionedColumn>,
    pub referenceable: bool,
}

pub(crate) struct VersionedColumn {
    pub versions: std::ops::Range<u32>,
    pub name: Ident,
    pub typ: TokenStream,
    pub doc_comments: Vec<Attribute>,
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
                columns.insert(
                    i,
                    SingleVersionColumn {
                        name: c.name.clone(),
                        typ: c.typ.clone(),
                        is_def: version == c.versions.end - 1,
                        doc_comments: c.doc_comments.clone(),
                    },
                );
            }
        }
        // we don't want to leak the span from table.name into `prev`
        let mut prev = Some(format_ident!("{}", table.name, span = Span::call_site()));
        if version == table.versions.start {
            prev = table.prev.clone();
        }
        if prev.is_some() && version == self.versions.start {
            return Err(syn::Error::new_spanned(
                prev,
                "the previous schema does not exists",
            ));
        }

        Ok(SingleVersionTable {
            prev,
            name: table.name.clone(),
            uniques: table.uniques.clone(),
            doc_comments: table.doc_comments.clone(),
            columns,
            referenceable: table.referenceable,
        })
    }
}

pub(crate) struct SingleVersionTable {
    pub prev: Option<Ident>,
    pub name: Ident,
    pub uniques: Vec<Unique>,
    pub doc_comments: Vec<Attribute>,
    pub columns: BTreeMap<usize, SingleVersionColumn>,
    pub referenceable: bool,
}

pub(crate) struct SingleVersionColumn {
    pub name: Ident,
    pub typ: TokenStream,
    // is this the latest version where the column exists?
    pub is_def: bool,
    pub doc_comments: Vec<Attribute>,
}
