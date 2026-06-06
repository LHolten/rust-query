use std::marker::PhantomData;

use crate::{
    Expr, Table, TableRow,
    db::TableRowInner,
    lower::{self, JoinableTable, Scope, TmpTable},
    private::Joinable,
    select,
    value::DbTyp,
};

impl<'column, S, T: DbTyp> Default for Expr<'column, S, T> {
    #[cfg_attr(false, mutants::skip)]
    fn default() -> Self {
        Expr::adhoc(lower::Expr::default())
    }
}

impl Default for JoinableTable {
    #[cfg_attr(false, mutants::skip)]
    fn default() -> Self {
        JoinableTable::Table("foo")
    }
}

impl<S, T: DbTyp> Default for Joinable<'_, S, T> {
    #[cfg_attr(false, mutants::skip)]
    fn default() -> Self {
        Self::new(JoinableTable::default(), "id")
    }
}

impl Default for TmpTable {
    #[cfg_attr(false, mutants::skip)]
    fn default() -> Self {
        Scope::default().tmp_table()
    }
}

impl<T: Table> Default for TableRow<T> {
    #[cfg_attr(false, mutants::skip)]
    fn default() -> Self {
        Self {
            _local: Default::default(),
            inner: TableRowInner {
                _p: std::marker::PhantomData,
                idx: 0,
            },
        }
    }
}

impl Default for lower::Expr {
    #[cfg_attr(false, mutants::skip)]
    fn default() -> Self {
        lower::Expr::Constant("null")
    }
}

impl<T> Default for select::Cached<T> {
    #[cfg_attr(false, mutants::skip)]
    fn default() -> Self {
        Self {
            idx: 0,
            _p: PhantomData,
        }
    }
}
