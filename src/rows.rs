use std::marker::PhantomData;

use sea_query::{Expr, SimpleExpr};

use crate::{
    ast::MySelect,
    db::Join,
    value::{operations::Assume, IntoColumn},
    Column, Table,
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
    pub(crate) phantom: PhantomData<fn(&'inner S) -> &'inner S>,
    pub(crate) ast: MySelect,
}

impl<'inner, S> Rows<'inner, S> {
    /// Join a table, this is like a super simple [Iterator::flat_map] but for queries.
    ///
    /// After this operation [Rows] has rows for the combinations of each original row with each row of the table.
    /// (Also called the "Carthesian product")
    ///
    /// For convenience there is also [Table::join].
    pub fn join<T: Table<Schema = S>>(&mut self) -> Column<'inner, S, T> {
        let alias = self.ast.scope.new_alias();
        self.ast.tables.push((T::NAME.to_owned(), alias));
        Column::new(Join::new(alias))
    }

    pub(crate) fn join_custom<T: Table>(&mut self, t: T) -> Column<'inner, S, T> {
        let alias = self.ast.scope.new_alias();
        self.ast.tables.push((t.name(), alias));
        Column::new(Join::new(alias))
    }

    // Join a vector of values.
    // pub fn vec<V: Column<'inner>>(&mut self, vec: Vec<V>) -> Join<'inner, V::Typ> {
    //     todo!()
    // }

    /// Filter rows based on a column.
    pub fn filter(&mut self, prop: impl IntoColumn<'inner, S, Typ = bool>) {
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
    ) -> Column<'inner, S, Typ> {
        self.filter_private(Expr::expr(val.build_expr(self.ast.builder())).is_not_null());
        Column::new(Assume(val.into_column().0))
    }
}
