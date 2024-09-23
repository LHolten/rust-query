use std::marker::PhantomData;

use sea_query::{Expr, SimpleExpr};

use crate::{
    ast::MySelect,
    db::Join,
    value::{operations::Assume, Typed, Value},
    DynValue, Table,
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
    pub(crate) ast: &'inner mut MySelect,
}

impl<'inner, S> Rows<'inner, S> {
    /// Join a table, this is like a super simple [Iterator::flat_map] but for queries.
    ///
    /// The resulting [Rows] has rows for the combinations of each original row with each row of the table.
    /// (Also called the "Carthesian product")
    ///
    /// For convenience there is also [Table::join].
    pub fn join<T: Table<Schema = S>>(&mut self) -> DynValue<'inner, S, T> {
        let alias = self.ast.scope.new_alias();
        self.ast.tables.push((T::NAME.to_owned(), alias));
        Value::into_dyn(Join::new(alias))
    }

    pub(crate) fn join_custom<T: Table>(&mut self, t: T) -> Join<'inner, T> {
        let alias = self.ast.scope.new_alias();
        self.ast.tables.push((t.name(), alias));
        Join::new(alias)
    }

    // Join a vector of values.
    // pub fn vec<V: Value<'inner>>(&mut self, vec: Vec<V>) -> Join<'inner, V::Typ> {
    //     todo!()
    // }

    /// Perform an aggregate that returns a single result for each of the current rows.
    ///
    /// You can filter the rows in the aggregate based on values from the outer query.
    /// That is the only way to get a different aggregate for each outer row.

    /// Filter rows based on a column.
    pub fn filter(&mut self, prop: impl Value<'inner, S, Typ = bool>) {
        self.filter_private(prop.build_expr(self.ast.builder()));
    }

    fn filter_private(&mut self, prop: SimpleExpr) {
        self.ast.filters.push(Box::new(prop));
    }

    /// Filter out rows where this column is [None].
    ///
    /// Returns a new column reference with the unwrapped type.
    pub fn filter_some<T, V>(&mut self, val: V) -> Assume<V>
    where
        V: Value<'inner, S, Typ = Option<T>>,
    {
        self.filter_private(Expr::expr(val.build_expr(self.ast.builder())).is_not_null());
        Assume(val)
    }

    pub fn empty<T: 'inner>(&mut self) -> DynValue<'inner, S, T> {
        self.filter(false);
        Never(PhantomData).into_dyn()
    }
}

struct Never<'t, T>(PhantomData<fn(&'t T) -> &'t T>);
impl<T> Typed for Never<'_, T> {
    type Typ = T;
}
impl<'t, S, T> Value<'t, S> for Never<'t, T> {
    fn build_expr(&self, _: crate::value::ValueBuilder) -> SimpleExpr {
        SimpleExpr::Keyword(sea_query::Keyword::Null)
    }
}
