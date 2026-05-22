use crate::{
    Expr, IntoExpr, Table, TableRow,
    db::TableRowInner,
    lower::{JoinableTable, Scope, TmpTable},
    value::DbTyp,
};

impl<'column, S, T: DbTyp + Default + IntoExpr<'column, S, Typ = T>> Default
    for Expr<'column, S, T>
{
    #[mutants::skip]
    fn default() -> Self {
        T::default().into_expr()
    }
}

impl Default for JoinableTable {
    #[mutants::skip]
    fn default() -> Self {
        JoinableTable::Table("foo", "id")
    }
}

impl Default for TmpTable {
    #[mutants::skip]
    fn default() -> Self {
        Scope::default().tmp_table()
    }
}

impl<T: Table> Default for TableRow<T> {
    #[mutants::skip]
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
