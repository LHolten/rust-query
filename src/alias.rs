use std::sync::atomic::{AtomicU64, Ordering};

use sea_query::{Iden, IntoColumnRef};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct FieldAlias {
    pub table: MyAlias,
    pub col: Field,
}

impl IntoColumnRef for FieldAlias {
    fn into_column_ref(self) -> sea_query::ColumnRef {
        (self.table, self.col).into_column_ref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Field {
    U64(MyAlias),
    Str(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct MyAlias {
    name: u64,
}

impl sea_query::Iden for Field {
    fn unquoted(&self, s: &mut dyn std::fmt::Write) {
        match self {
            Field::U64(alias) => alias.unquoted(s),
            Field::Str(name) => write!(s, "{}", name).unwrap(),
        }
    }
}

impl MyAlias {
    pub fn new() -> Self {
        static IDEN_NUM: AtomicU64 = AtomicU64::new(0);
        let next = IDEN_NUM.fetch_add(1, Ordering::Relaxed);
        Self { name: next }
    }
}

impl Field {
    pub fn new() -> Self {
        Field::U64(MyAlias::new())
    }
}

impl sea_query::Iden for MyAlias {
    fn unquoted(&self, s: &mut dyn std::fmt::Write) {
        write!(s, "_{}", self.name).unwrap()
    }
}

pub(crate) struct RawAlias(pub(crate) String);

impl Iden for RawAlias {
    fn unquoted(&self, s: &mut dyn std::fmt::Write) {
        write!(s, "{}", self.0).unwrap()
    }
    fn prepare(&self, s: &mut dyn std::fmt::Write, _q: sea_query::Quote) {
        self.unquoted(s)
    }
}
