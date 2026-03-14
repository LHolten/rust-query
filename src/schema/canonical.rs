use std::{borrow::Cow, collections::BTreeSet};

use crate::schema::check_constraint::Parsed;

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum ColumnType {
    Integer = 0,
    Real = 1,
    Text = 2,
    Blob = 3,
    Any = 4,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Column {
    pub typ: ColumnType,
    pub nullable: bool,
    pub fk: Option<(String, String)>,
    pub check: Option<Parsed>,
}

impl std::hash::Hash for Column {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.typ.hash(state);
        self.nullable.hash(state);
        self.fk.hash(state);
        // for backwards compatibility
        if let Some(check) = &self.check {
            Some(check.to_string()).hash(state);
        }
    }
}

// TODO: remove redundant unique constraints
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Unique {
    pub columns: BTreeSet<Cow<'static, str>>,
}

impl std::hash::Hash for Unique {
    fn hash<H: std::hash::Hasher>(&self, hasher: &mut H) {
        self.columns.hash(hasher);
        true.hash(hasher); // for backwards compatibility
    }
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Default)]
#[cfg(feature = "dev")]
pub struct Table {
    pub columns: std::collections::BTreeMap<String, Column>,
    pub indices: BTreeSet<Unique>,
}

#[derive(Debug, Hash, Default, PartialEq, Eq)]
#[cfg(feature = "dev")]
pub struct Schema {
    pub tables: std::collections::BTreeMap<String, Table>,
}
