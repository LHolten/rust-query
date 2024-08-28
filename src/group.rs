use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use sea_query::{Alias, Func, SimpleExpr};

use crate::{
    alias::{Field, MyAlias},
    ast::MySelect,
    db::{Col, TableRef},
    query::Query,
    value::{
        operations::{Const, NotNull, UnwrapOr},
        EqTyp, NumTyp, Value,
    },
};

/// This is the query type used in sub-queries.
/// It can only produce one result (for each outer result).
/// This type dereferences to [Query].
pub struct Aggregate<'outer, 'inner, S> {
    pub(crate) outer_ast: &'inner MySelect,
    pub(crate) conds: &'inner mut Vec<(Field, SimpleExpr)>,
    pub(crate) query: Query<'inner, S>,
    pub(crate) table: MyAlias,
    pub(crate) phantom2: PhantomData<fn(&'outer ()) -> &'outer ()>,
}

impl<'outer, 'inner, S> Deref for Aggregate<'outer, 'inner, S> {
    type Target = Query<'inner, S>;

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
    pub fn avg<V: Value<'inner, S, Typ = f64>>(
        &'inner self,
        val: V,
    ) -> AggrCol<'outer, S, Option<f64>> {
        let expr = Func::cast_as(
            Func::avg(val.build_expr(self.ast.builder())),
            Alias::new("real"),
        );
        self.select(expr)
    }

    /// Return the maximum value in a column, this is [None] if there are zero rows.
    pub fn max<V: Value<'inner, S>>(&'inner self, val: V) -> AggrCol<'outer, S, Option<V::Typ>>
    where
        V::Typ: NumTyp,
    {
        let expr = Func::max(val.build_expr(self.ast.builder()));
        self.select(expr)
    }

    /// Return the sum of a column.
    pub fn sum<V: Value<'inner, S>>(
        &'inner self,
        val: V,
    ) -> UnwrapOr<AggrCol<'outer, S, Option<V::Typ>>, Const<V::Typ>>
    where
        V::Typ: NumTyp,
    {
        let expr = Func::cast_as(
            Func::sum(val.build_expr(self.ast.builder())),
            Alias::new("integer"),
        );
        UnwrapOr(self.select(expr), Const(V::Typ::ZERO))
    }

    /// Return the number of distinct values in a column.
    pub fn count_distinct<V: Value<'inner, S>>(
        &'inner self,
        val: V,
    ) -> UnwrapOr<AggrCol<'outer, S, Option<i64>>, Const<i64>>
    where
        V::Typ: EqTyp,
    {
        let expr = Func::count_distinct(val.build_expr(self.ast.builder()));
        UnwrapOr(self.select(expr), Const(0))
    }

    /// Return whether there are any rows.
    pub fn exists(&'inner self) -> NotNull<AggrCol<'outer, S, Option<i64>>> {
        let expr = SimpleExpr::Constant(1.into_value());
        NotNull(self.select::<i64>(expr))
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
