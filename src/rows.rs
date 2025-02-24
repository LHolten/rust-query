use std::marker::PhantomData;

use sea_query::SimpleExpr;

use crate::{
    ast::MySelect,
    db::Join,
    value::{operations::Assume, IntoColumn, Typed},
    Expr, Table,
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
        let alias = self.ast.scope.new_alias();
        self.ast.tables.push((T::NAME.to_owned(), alias));
        Expr::new(Join::new(alias))
    }

    pub(crate) fn join_custom<T: Table>(&mut self, t: T) -> Expr<'inner, S, T> {
        let alias = self.ast.scope.new_alias();
        self.ast.tables.push((t.name(), alias));
        Expr::new(Join::new(alias))
    }

    // Join a vector of values.
    // pub fn vec<V: IntoColumn<'inner>>(&mut self, vec: Vec<V>) -> Join<'inner, V::Typ> {
    //     todo!()
    // }

    /// Filter rows based on a column.
    pub fn filter(&mut self, prop: impl IntoColumn<'inner, S, Typ = bool>) {
        let prop = prop.into_column().inner;
        self.filter_private(prop.build_expr(self.ast.builder()));
    }

    fn filter_private(&mut self, prop: SimpleExpr) {
        self.ast.filters.push(Box::new(prop));
    }

    /// Filter out rows where this column is [None].
    ///
    /// Returns a new column with the unwrapped type.
    pub fn filter_some<Typ: 'static>(
        &mut self,
        val: impl IntoColumn<'inner, S, Typ = Option<Typ>>,
    ) -> Expr<'inner, S, Typ> {
        let val = val.into_column().inner;
        self.filter_private(
            sea_query::Expr::expr(val.build_expr(self.ast.builder())).is_not_null(),
        );
        Expr::new(Assume(val))
    }
}
