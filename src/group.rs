use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use sea_query::{Func, SimpleExpr};

use crate::{
    alias::{Field, MyAlias},
    ast::MySelect,
    db::{Col, TableRef},
    query::Rows,
    value::{
        operations::{Const, IsNotNull, UnwrapOr},
        EqTyp, NumTyp, Value,
    },
};

/// This is the argument type used for aggregates.
///
/// While it is possible to join many tables in an aggregate, there can be only one result.
/// (The result can be a tuple or struct with multiple values though).
pub struct Aggregate<'outer, 'inner, S> {
    pub(crate) outer_ast: &'inner MySelect,
    pub(crate) conds: &'inner mut Vec<(Field, SimpleExpr)>,
    pub(crate) query: Rows<'inner, S>,
    pub(crate) table: MyAlias,
    pub(crate) phantom2: PhantomData<fn(&'outer ()) -> &'outer ()>,
}

impl<'outer, 'inner, S> Deref for Aggregate<'outer, 'inner, S> {
    type Target = Rows<'inner, S>;

    fn deref(&self) -> &Self::Target {
        &self.query
    }
}

impl<'outer, 'inner, S> DerefMut for Aggregate<'outer, 'inner, S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.query
    }
}

impl<'outer: 'inner, 'inner, S> Aggregate<'outer, 'inner, S> {
    fn select<T>(&'inner self, expr: impl Into<SimpleExpr>) -> AggrCol<'outer, S, Option<T>> {
        let alias = self
            .ast
            .select
            .get_or_init(expr.into(), || self.ast.scope.new_field());
        AggrCol::db(self.table, *alias)
    }

    /// Filter the rows of this sub-query based on a value from the outer query.
    pub fn filter_on<T>(
        &mut self,
        val: impl Value<'inner, S, Typ = T>,
        on: impl Value<'outer, S, Typ = T>,
    ) {
        let alias = self.ast.scope.new_alias();
        self.conds
            .push((Field::U64(alias), on.build_expr(self.outer_ast.builder())));
        self.ast
            .filter_on
            .push(Box::new((val.build_expr(self.ast.builder()), alias)))
    }

    /// Return the average value in a column, this is [None] if there are zero rows.
    pub fn avg(
        &'inner self,
        val: impl Value<'inner, S, Typ = f64>,
    ) -> AggrCol<'outer, S, Option<f64>> {
        let expr = Func::avg(val.build_expr(self.ast.builder()));
        self.select(expr)
    }

    /// Return the maximum value in a column, this is [None] if there are zero rows.
    pub fn max<T>(
        &'inner self,
        val: impl Value<'inner, S, Typ = T>,
    ) -> AggrCol<'outer, S, Option<T>>
    where
        T: NumTyp,
    {
        let expr = Func::max(val.build_expr(self.ast.builder()));
        self.select(expr)
    }

    /// Return the sum of a column.
    pub fn sum<T>(
        &'inner self,
        val: impl Value<'inner, S, Typ = T>,
    ) -> UnwrapOr<AggrCol<'outer, S, Option<T>>, Const<T>>
    where
        T: NumTyp,
    {
        let expr = Func::sum(val.build_expr(self.ast.builder()));
        UnwrapOr(self.select(expr), Const(T::ZERO))
    }

    /// Return the number of distinct values in a column.
    pub fn count_distinct<T>(
        &'inner self,
        val: impl Value<'inner, S, Typ = T>,
    ) -> UnwrapOr<AggrCol<'outer, S, Option<i64>>, Const<i64>>
    where
        T: EqTyp,
    {
        let expr = Func::count_distinct(val.build_expr(self.ast.builder()));
        UnwrapOr(self.select(expr), Const(0))
    }

    /// Return whether there are any rows.
    pub fn exists(&'inner self) -> IsNotNull<AggrCol<'outer, S, Option<i64>>> {
        let expr = SimpleExpr::Constant(1.into_value());
        IsNotNull(self.select::<i64>(expr))
    }
}

pub struct Aggr<'t, S> {
    pub(crate) table: MyAlias,
    pub(crate) _p: PhantomData<fn(&'t S) -> &'t S>,
}

impl<S> Copy for Aggr<'_, S> {}
impl<S> Clone for Aggr<'_, S> {
    fn clone(&self) -> Self {
        *self
    }
}

type AggrCol<'t, S, T> = Col<T, Aggr<'t, S>>;

impl<'t, S, T> AggrCol<'t, S, T> {
    fn db(table: MyAlias, field: Field) -> Self {
        Col {
            _p: PhantomData,
            field,
            inner: Aggr {
                table,
                _p: PhantomData,
            },
        }
    }
}

impl<'t, S> TableRef<'t> for Aggr<'t, S> {
    type Schema = S;
    fn build_table(&self, _: crate::value::ValueBuilder) -> MyAlias {
        self.table
    }
}
