use std::collections::BTreeMap;

#[derive(Debug)]
pub struct Column {
    pub typ: String,
    pub nullable: bool,
    pub fk: Option<(String, String)>,
}

#[derive(Debug)]
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

// Temporary impls until diff.rs is done
mod to_macro {
    use super::*;
    use crate::schema::from_macro;

    impl Column {
        fn to_macro(self) -> from_macro::Column {
            let typ = match self.typ.as_str() {
                "INTEGER" => from_macro::ColumnType::Integer,
                "TEXT" => from_macro::ColumnType::String,
                "REAL" => from_macro::ColumnType::Float,
                t => panic!("unknown type {t}"),
            };
            from_macro::Column {
                typ,
                nullable: self.nullable,
                fk: self.fk,
                span: (0, 0),
            }
        }
    }

    impl Index {
        fn to_macro(self) -> from_macro::Index {
            from_macro::Index {
                columns: self.columns,
                unique: self.unique,
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
            }
        }
    }
}
