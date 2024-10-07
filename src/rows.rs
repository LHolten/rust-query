use std::marker::PhantomData;

use sea_query::{Expr, SimpleExpr};

use crate::{
    ast::MySelect,
    db::Join,
    value::{operations::Assume, IntoColumn, Typed},
    Column, Table,
};

/// [Rows] keeps track of rows from which tables are in use.
///
/// Adding rows is done using the `::join()` method that exists on each table type.
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
    /// The resulting [Rows] has rows for the combinations of each original row with each row of the table.
    /// (Also called the "Carthesian product")
    ///
    /// For convenience there is also [Table::join].
    pub fn join<T: Table<Schema = S>>(&mut self) -> Column<'inner, S, T> {
        let alias = self.ast.scope.new_alias();
        self.ast.tables.push((T::NAME.to_owned(), alias));
        IntoColumn::into_value(Join::new(alias))
    }

    pub(crate) fn join_custom<T: Table>(&mut self, t: T) -> Join<'inner, T> {
        let alias = self.ast.scope.new_alias();
        self.ast.tables.push((t.name(), alias));
        Join::new(alias)
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
    /// Returns a new column reference with the unwrapped type.
    pub fn filter_some<Typ>(
        &mut self,
        val: impl IntoColumn<'inner, S, Typ = Option<Typ>>,
    ) -> Column<'inner, S, Typ> {
        self.filter_private(Expr::expr(val.build_expr(self.ast.builder())).is_not_null());
        Assume(val).into_value()
    }

    pub fn empty<T: 'inner>(&mut self) -> Column<'inner, S, T> {
        self.filter(false);
        Never(PhantomData).into_value()
    }
}

struct Never<'t, T>(PhantomData<fn(&'t T) -> &'t T>);

impl<'t, T> Copy for Never<'t, T> {}
impl<'t, T> Clone for Never<'t, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Typed for Never<'_, T> {
    type Typ = T;
    fn build_expr(&self, _: crate::value::ValueBuilder) -> SimpleExpr {
        SimpleExpr::Keyword(sea_query::Keyword::Null)
    }
}
impl<'t, S, T> IntoColumn<'t, S> for Never<'t, T> {
    type Owned = Self;

    fn into_owned(self) -> Self::Owned {
        self
    }
}
