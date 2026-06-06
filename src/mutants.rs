use std::{cell::OnceCell, marker::PhantomData};

use crate::{
    Expr, Lazy, Mutable, Select, Table, TableRow, Transaction,
    db::TableRowInner,
    lower::{self, JoinableTable, Scope, TmpTable},
    private::Joinable,
    query::Iter,
    select::{self, Cached, DynPrepared, DynSelectImpl},
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

impl<T: Table> Default for Mutable<'_, T> {
    #[cfg_attr(false, mutants::skip)]
    fn default() -> Self {
        Self {
            cell: OnceCell::new(),
            row_id: TableRow::default(),
            _txn: Default::default(),
        }
    }
}

impl<T: Table> Default for Lazy<'_, T> {
    #[cfg_attr(false, mutants::skip)]
    fn default() -> Self {
        Self {
            id: TableRow::default(),
            lazy: OnceCell::new(),
            txn: Transaction::new_ref(),
        }
    }
}

impl<S, Out: DbTyp> Default for Select<'_, S, Out> {
    #[cfg_attr(false, mutants::skip)]
    fn default() -> Self {
        Self {
            inner: DynSelectImpl::default(),
            _p: PhantomData,
            _p2: PhantomData,
        }
    }
}

impl<Out: DbTyp> Default for DynSelectImpl<Out> {
    #[cfg_attr(false, mutants::skip)]
    fn default() -> Self {
        Self {
            inner: Box::new(|_cacher| DynPrepared::default()),
        }
    }
}

impl<Out: DbTyp> Default for DynPrepared<Out> {
    #[cfg_attr(false, mutants::skip)]
    fn default() -> Self {
        Self {
            inner: Box::new(Cached::default()),
        }
    }
}

impl<O: DbTyp> Default for Iter<'_, O> {
    #[cfg_attr(false, mutants::skip)]
    fn default() -> Self {
        Self {
            inner_phantom: PhantomData,
            inner: 0,
            prepared: DynPrepared::default(),
            cached: Vec::new(),
        }
    }
}
