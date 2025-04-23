use std::sync::atomic::{AtomicU64, Ordering};

use sea_query::Iden;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Field {
    U64(MyAlias),
    Str(&'static str),
}

#[derive(Default)]
pub struct Scope {
    iden_num: AtomicU64,
}

impl Scope {
    pub fn tmp_table(&self) -> TmpTable {
        let next = self.iden_num.fetch_add(1, Ordering::Relaxed);
        TmpTable { name: next }
    }

    pub fn new_alias(&self) -> MyAlias {
        let next = self.iden_num.fetch_add(1, Ordering::Relaxed);
        MyAlias { name: next }
    }

    pub fn new_field(&self) -> Field {
        Field::U64(self.new_alias())
    }

    pub fn create(num_tables: usize, num_filter_on: usize) -> Self {
        Self {
            iden_num: AtomicU64::new(num_tables.max(num_filter_on) as u64),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MyAlias {
    name: u64,
}

impl MyAlias {
    pub fn new(idx: usize) -> Self {
        Self { name: idx as u64 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct TmpTable {
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

impl sea_query::Iden for MyAlias {
    fn unquoted(&self, s: &mut dyn std::fmt::Write) {
        write!(s, "_{}", self.name).unwrap()
    }
}

impl sea_query::Iden for TmpTable {
    fn unquoted(&self, s: &mut dyn std::fmt::Write) {
        write!(s, "_tmp{}", self.name).unwrap()
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
