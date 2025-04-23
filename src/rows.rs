use std::marker::PhantomData;

use sea_query::Iden;

use crate::{
    Expr, Table,
    alias::TmpTable,
    ast::MySelect,
    db::Join,
    value::{IntoExpr, Typed},
};

/// [Rows] keeps track of all rows in the current query.
///
/// This is the base type for other query types like [crate::args::Aggregate] and [crate::args::Query].
/// It contains most query functionality like joining tables and doing sub-queries.
///
/// [Rows] mutability is only about which rows are included.
/// Adding new columns does not require mutating [Rows].
pub struct Rows<'inner, S> {
    // we might store 'inner
    pub(crate) phantom: PhantomData<fn(&'inner ()) -> &'inner ()>,
    pub(crate) _p: PhantomData<S>,
    pub(crate) ast: MySelect,
}

impl<'inner, S> Rows<'inner, S> {
    /// Join a table, this is like a super simple [Iterator::flat_map] but for queries.
    ///
    /// After this operation [Rows] has rows for the combinations of each original row with each row of the table.
    /// (Also called the "Carthesian product")
    ///
    /// For convenience there is also [Table::join].
    pub fn join<T: Table<Schema = S>>(&mut self) -> Expr<'inner, S, T> {
        let alias = self.ast.builder.scope.new_alias();
        self.ast.tables.push((T::NAME.to_owned(), alias));
        Expr::new(Join::new(alias))
    }

    pub(crate) fn join_custom<T: Table<Schema = S>>(&mut self, t: T) -> Expr<'inner, S, T> {
        let alias = self.ast.builder.scope.new_alias();
        self.ast.tables.push((t.name(), alias));
        Expr::new(Join::new(alias))
    }

    pub(crate) fn join_tmp<T: Table<Schema = S>>(&mut self, tmp: TmpTable) -> Expr<'inner, S, T> {
        let mut tmp_string = String::new();
        tmp.unquoted(&mut tmp_string);
        let alias = self.ast.builder.scope.new_alias();
        self.ast.tables.push((tmp_string, alias));
        Expr::new(Join::new(alias))
    }

    // Join a vector of values.
    // pub fn vec<V: IntoExpr<'inner>>(&mut self, vec: Vec<V>) -> Join<'inner, V::Typ> {
    //     todo!()
    // }

    /// Filter rows based on a column.
    pub fn filter(&mut self, prop: impl IntoExpr<'inner, S, Typ = bool>) {
        let prop = prop.into_expr();
        self.ast.filters.push(prop.inner);
    }

    /// Filter out rows where this column is [None].
    ///
    /// Returns a new column with the unwrapped type.
    pub fn filter_some<Typ: 'static>(
        &mut self,
        val: impl IntoExpr<'inner, S, Typ = Option<Typ>>,
    ) -> Expr<'inner, S, Typ> {
        let val = val.into_expr();
        self.ast.filters.push(val.is_some().inner);

        Expr::adhoc(move |b| val.inner.build_expr(b))
    }
}
