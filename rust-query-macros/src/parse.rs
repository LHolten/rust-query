use std::ops::{Not, Range};

use quote::ToTokens;
use syn::{parse2, punctuated::Punctuated, Attribute, Field, Ident, Item, Path, Token, Visibility};

use crate::multi::{
    ColumnTyp, Reference, Unique, VersionedColumn, VersionedSchema, VersionedTable,
};

impl VersionedColumn {
    pub fn parse(field: Field, limit: Range<u32>) -> syn::Result<Self> {
        let Some(name) = field.ident.clone() else {
            return Err(syn::Error::new_spanned(field, "field must be named"));
        };

        let Visibility::Public(_) = field.vis else {
            return Err(syn::Error::new_spanned(name, "field must be public"));
        };

        // not sure if case matters here
        if name.to_string().to_lowercase() == "id" {
            return Err(syn::Error::new_spanned(
                name,
                "The `id` column is reserved to be used by rust-query internally",
            ));
        }

        let mut other_field_attr = vec![];
        let mut follow_rename = false;
        for attr in field.attrs.clone() {
            if attr.path().is_ident("follow") {
                follow_rename = true;
            } else {
                other_field_attr.push(attr);
            }
        }
        let versions = parse_version(&other_field_attr)?
            .unwrap_or_default()
            .into_std(limit, true)?;

        Ok(VersionedColumn {
            versions,
            name,
            typ: if follow_rename {
                ColumnTyp::Reference(parse2(field.ty.clone().into_token_stream())?)
            } else {
                ColumnTyp::Normal(field.ty.into_token_stream())
            },
        })
    }
}

impl VersionedTable {
    pub fn parse(table: syn::ItemStruct, limit: Range<u32>) -> syn::Result<Self> {
        let Visibility::Public(_) = table.vis else {
            return Err(syn::Error::new_spanned(table.ident, "table must be public"));
        };

        let mut other_attrs = vec![];
        let mut uniques = vec![];
        let mut prev = None;
        let mut referenceable = true;

        for attr in &table.attrs {
            if let Some(unique) = is_unique(attr.path()) {
                let idents =
                    attr.parse_args_with(Punctuated::<Ident, Token![,]>::parse_separated_nonempty)?;
                uniques.push(Unique {
                    name: unique,
                    columns: idents.into_iter().collect(),
                })
            } else if attr.path().is_ident("no_reference") {
                referenceable = false;
            } else if attr.path().is_ident("from") {
                prev = Some(attr.parse_args()?)
            } else {
                other_attrs.push(attr.clone());
            }
        }

        if !referenceable && prev.is_some() {
            return Err(syn::Error::new_spanned(
                prev,
                "can not use `no_reference` and `from` together",
            ));
        }

        let versions = parse_version(&other_attrs)?
            .unwrap_or_default()
            .into_std(limit, true)?;

        let columns = table
            .fields
            .into_iter()
            .map(|x| VersionedColumn::parse(x, versions.clone()))
            .collect::<Result<_, _>>()?;

        Ok(VersionedTable {
            versions,
            prev,
            name: table.ident.clone(),
            columns,
            uniques,
            referenceable,
        })
    }
}

impl VersionedSchema {
    pub fn parse(item: syn::ItemMod) -> syn::Result<Self> {
        let versions = parse_version(&item.attrs)?
            .unwrap_or_default()
            .into_std(0..1, false)?;

        let Visibility::Public(_) = item.vis else {
            return Err(syn::Error::new_spanned(item.ident, "module must be public"));
        };

        if let Some(unsafety) = item.unsafety {
            return Err(syn::Error::new_spanned(
                unsafety,
                "module can not be unsafe",
            ));
        };

        let Some(content) = item.content else {
            return Err(syn::Error::new_spanned(item.ident, "module must be inline"));
        };

        let tables = content
            .1
            .into_iter()
            .map(|x| {
                let Item::Struct(x) = x else {
                    return Err(syn::Error::new_spanned(x, "only struct items are allowed"));
                };

                VersionedTable::parse(x, versions.clone())
            })
            .collect::<Result<_, _>>()?;

        Ok(VersionedSchema { versions, tables })
    }
}

impl syn::parse::Parse for Reference {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let first = input.parse()?;
        if input.is_empty() {
            return Ok(Reference {
                inner: first,
                wrap: None,
            });
        }
        if first != "Option" {
            return Err(syn::Error::new_spanned(first, "expected `Option`"));
        }
        let open = input.parse()?;
        let inner = input.parse()?;
        let close = input.parse()?;
        Ok(Reference {
            inner,
            wrap: Some((first, open, close)),
        })
    }
}

#[derive(Default)]
pub(crate) struct MyRange {
    pub start: Option<syn::LitInt>,
    pub _dotdot: Token![..],
    pub end: Option<RangeEnd>,
}

pub(crate) struct RangeEnd {
    pub equals: Option<Token![=]>,
    pub num: syn::LitInt,
}

impl syn::parse::Parse for MyRange {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(Self {
            start: input.parse()?,
            _dotdot: input.parse()?,
            end: input.is_empty().not().then(|| input.parse()).transpose()?,
        })
    }
}

impl syn::parse::Parse for RangeEnd {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(Self {
            equals: input.parse()?,
            num: input.parse()?,
        })
    }
}

impl MyRange {
    pub fn into_std(self, limit: Range<u32>, check: bool) -> syn::Result<Range<u32>> {
        let start = self
            .start
            .as_ref()
            .map(|x| x.base10_parse())
            .transpose()?
            .unwrap_or(limit.start);
        if check && start < limit.start {
            return Err(syn::Error::new_spanned(
                self.start,
                "start of range is before outer range start",
            ));
        }

        let end = self
            .end
            .as_ref()
            .map(|x| syn::Result::Ok(x.num.base10_parse::<u32>()? + (x.equals.is_some() as u32)))
            .transpose()?
            .unwrap_or(limit.end);
        if check && limit.end < end {
            return Err(syn::Error::new_spanned(
                self.end.unwrap().num,
                "end of range is after outer range end",
            ));
        }

        if end <= start {
            return Err(syn::Error::new_spanned(self._dotdot, "range is empty"));
        }

        Ok(start..end)
    }
}

fn parse_version(attrs: &[Attribute]) -> syn::Result<Option<MyRange>> {
    let mut version = None;
    for attr in attrs {
        if attr.path().is_ident("version") {
            if version.is_some() {
                return Err(syn::Error::new_spanned(
                    attr,
                    "There should be only one version attribute.",
                ));
            }
            version = Some(attr.parse_args::<MyRange>()?);
        } else {
            return Err(syn::Error::new_spanned(attr, "unexpected attribute"));
        }
    }
    Ok(version)
}

fn is_unique(path: &Path) -> Option<Ident> {
    path.get_ident().and_then(|ident| {
        ident
            .to_string()
            .starts_with("unique")
            .then(|| ident.clone())
    })
}
