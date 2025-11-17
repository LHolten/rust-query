use std::collections::BTreeMap;

use crate::schema::canonical::ColumnType;

#[derive(Debug)]
pub struct Column {
    pub typ: String,
    pub nullable: bool,
    pub fk: Option<(String, String)>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Index {
    // column order matters for performance
    pub columns: Vec<String>,
    pub unique: bool,
}

#[derive(Debug, Default)]
pub struct Table {
    pub columns: BTreeMap<String, Column>,
    pub indices: BTreeMap<String, Index>,
}

#[derive(Debug, Default)]
pub struct Schema {
    pub tables: BTreeMap<String, Table>,
}

impl Column {
    pub fn parse_typ(&self) -> Option<ColumnType> {
        // These are all the possible types in a STRICT table.
        Some(match self.typ.as_str() {
            "INTEGER" | "INT" => ColumnType::Integer,
            "TEXT" => ColumnType::Text,
            "REAL" => ColumnType::Real,
            "BLOB" => ColumnType::Blob,
            "ANY" => ColumnType::Any,
            _ => return None,
        })
    }

    pub fn render_rust(&self) -> String {
        let base = if let Some((table, col)) = &self.fk {
            if col == "id" {
                table.clone()
            } else {
                format!("{table}::{col}")
            }
        } else {
            match self.parse_typ() {
                Some(ColumnType::Integer) => "i64".to_owned(),
                Some(ColumnType::Text) => "String".to_owned(),
                Some(ColumnType::Real) => "f64".to_owned(),
                Some(ColumnType::Blob) => "Vec<u8>".to_owned(),
                _ => format!("{{{}}}", self.typ),
            }
        };
        if self.nullable {
            format!("Option<{base}>")
        } else {
            base
        }
    }
}
