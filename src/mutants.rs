use sea_query::IntoIden;

use crate::{
    Expr, IntoExpr, Table, TableRow,
    alias::{JoinableTable, MyAlias, Scope, TmpTable},
    db::TableRowInner,
    value::MyTyp,
};

impl<'column, S, T: MyTyp + Default + IntoExpr<'column, S, Typ = T>> Default
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
        JoinableTable::Normal("foo".into_iden())
    }
}

impl Default for TmpTable {
    #[mutants::skip]
    fn default() -> Self {
        Scope::default().tmp_table()
    }
}

impl Default for MyAlias {
    #[mutants::skip]
    fn default() -> Self {
        Scope::default().new_alias()
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
