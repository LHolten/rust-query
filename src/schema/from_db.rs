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
    pub fn parse_typ(&self) -> Result<ColumnType, String> {
        // These are all the possible types in a STRICT table.
        Ok(match self.typ.as_str() {
            "INTEGER" | "INT" => ColumnType::Integer,
            "TEXT" => ColumnType::Text,
            "REAL" => ColumnType::Real,
            "BLOB" => ColumnType::Blob,
            "ANY" => ColumnType::Any,
            t => return Err(format!("unknown type {t}")),
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
                Ok(ColumnType::Integer) => "i64".to_owned(),
                Ok(ColumnType::Text) => "String".to_owned(),
                Ok(ColumnType::Real) => "f64".to_owned(),
                Ok(ColumnType::Blob) => "Vec<u8>".to_owned(),
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

// Temporary impls until diff.rs is done
mod to_macro {
    use super::*;
    use crate::schema::{canonical, from_macro};

    impl Column {
        fn to_macro(self) -> from_macro::Column {
            from_macro::Column {
                def: canonical::Column {
                    typ: self.parse_typ().unwrap(),
                    nullable: self.nullable,
                    fk: self.fk,
                },
                span: (0, 0),
            }
        }
    }

    impl Index {
        fn to_macro(self) -> from_macro::Index {
            from_macro::Index {
                def: self,
                span: (0, 0),
            }
        }
    }

    impl Table {
        fn to_macro(self) -> from_macro::Table {
            from_macro::Table {
                columns: self
                    .columns
                    .into_iter()
                    .map(|(k, v)| (k, v.to_macro()))
                    .collect(),
                indices: self.indices.into_values().map(Index::to_macro).collect(),
                span: (0, 0),
            }
        }
    }

    impl Schema {
        pub fn to_macro(self) -> from_macro::Schema {
            from_macro::Schema {
                tables: self
                    .tables
                    .into_iter()
                    .map(|(k, v)| (k, v.to_macro()))
                    .collect(),
                span: (0, 0),
            }
        }
    }
}
