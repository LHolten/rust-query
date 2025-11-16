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
        Ok(match self.typ.as_str() {
            "INTEGER" => ColumnType::Integer,
            "TEXT" => ColumnType::String,
            "REAL" => ColumnType::Float,
            t => return Err(format!("unknown type {t}")),
        })
    }

    pub fn render_rust(&self) -> String {
        let inner = match (&self.fk, self.parse_typ()) {
            (Some((table, col)), Ok(ColumnType::Integer)) if col == "id" => table.clone(),
            (None, Ok(ColumnType::Integer)) => "i64".to_owned(),
            (None, Ok(ColumnType::String)) => "String".to_owned(),
            (None, Ok(ColumnType::Float)) => "f64".to_owned(),
            (None, Ok(ColumnType::Blob)) => "Vec<u8>".to_owned(),
            _ => "{unknown}".to_owned(),
        };
        if self.nullable {
            format!("Option<{inner}>")
        } else {
            inner
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
