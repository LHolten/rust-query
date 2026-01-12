use std::sync::atomic::{AtomicU64, Ordering};

use sea_query::{DynIden, FunctionCall};

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

impl From<Field> for sea_query::DynIden {
    fn from(value: Field) -> Self {
        match value {
            Field::U64(alias) => alias.into(),
            Field::Str(name) => name.into(),
        }
    }
}

impl From<MyAlias> for sea_query::DynIden {
    fn from(value: MyAlias) -> Self {
        format!("_{}", value.name).into()
    }
}

impl From<TmpTable> for sea_query::DynIden {
    fn from(value: TmpTable) -> Self {
        format!("_tmp{}", value.name).into()
    }
}

#[derive(Clone)]
pub(crate) enum JoinableTable {
    Normal(DynIden),
    Pragma(FunctionCall),
}

impl JoinableTable {
    pub fn main_column(&self) -> &'static str {
        match self {
            JoinableTable::Normal(_) => "id",
            JoinableTable::Pragma(_) => panic!("main_column should not be used on pragma"),
        }
    }
}
