use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
};

use crate::schema::canonical::{Column, ColumnType};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Index {
    // column order matters for performance
    pub columns: Vec<Cow<'static, str>>,
    pub unique: bool,
}

#[derive(Debug)]
pub struct Table {
    pub primary_key: String,
    pub columns: BTreeMap<String, Column>,
    pub indices: BTreeSet<Index>,
}

#[derive(Debug, Default)]
pub struct Schema {
    pub tables: BTreeMap<String, Table>,
}

impl Column {
    pub fn render_rust(&self) -> String {
        let base = if let Some((table, col)) = &self.fk {
            if col == "id" {
                table.clone()
            } else {
                format!("{table}::{col}")
            }
        } else {
            match &self.typ {
                ColumnType::Integer => "i64".to_owned(),
                ColumnType::Text => "String".to_owned(),
                ColumnType::Real => "f64".to_owned(),
                ColumnType::Blob => "Vec<u8>".to_owned(),
                ColumnType::Unknown(unknown) => format!("{{{unknown}}}"),
            }
        };
        let wrapped = if self.nullable {
            format!("Option<{base}>")
        } else {
            base
        };
        if let Some(check) = &self.check {
            format!("{wrapped} CHECK ({check})")
        } else {
            wrapped
        }
    }
}
