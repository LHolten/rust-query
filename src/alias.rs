use std::sync::atomic::{AtomicUsize, Ordering};

use sea_query::{DynIden, FunctionCall};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Field {
    U64(MyAlias),
    Str(&'static str),
}

#[derive(Default)]
pub struct Scope {
    iden_num: AtomicUsize,
}

impl Scope {
    pub fn tmp_table(&self) -> TmpTable {
        let next = self.iden_num.fetch_add(1, Ordering::Relaxed);
        TmpTable { name: next }
    }

    pub fn new_alias(&self) -> MyAlias {
        let idx = self.iden_num.fetch_add(1, Ordering::Relaxed);
        MyAlias { idx }
    }

    pub fn create(num_tables: usize, num_filter_on: usize) -> Self {
        Self {
            iden_num: AtomicUsize::new(num_tables.max(num_filter_on)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MyAlias {
    pub(crate) idx: usize,
}

impl MyAlias {
    pub fn new(idx: usize) -> Self {
        Self { idx }
    }
    pub fn try_from(iden: &sea_query::DynIden) -> Option<Self> {
        Some(Self {
            idx: iden.inner().strip_prefix("_")?.parse().ok()?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct TmpTable {
    name: usize,
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
        format!("_{}", value.idx).into()
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
    Vec(Vec<sea_query::Value>),
}

impl JoinableTable {
    pub fn main_column(&self) -> &'static str {
        match self {
            JoinableTable::Normal(_) => "id",
            JoinableTable::Pragma(_) => "pragma_id", // should always be replaced
            JoinableTable::Vec(_) => "value",
        }
    }
}
