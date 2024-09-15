use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use ref_cast::RefCast;
use sea_query::{Expr, Func, SimpleExpr};

use crate::{
    alias::{Field, MyAlias},
    ast::MySelect,
    query::Rows,
    value::{
        operations::{Const, IsNotNull, UnwrapOr},
        EqTyp, MyTyp, NumTyp, Typed, Value,
    },
    Table,
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
    fn select<T>(&'inner self, expr: impl Into<SimpleExpr>) -> Aggr<'outer, S, Option<T>> {
        let alias = self
            .ast
            .select
            .get_or_init(expr.into(), || self.ast.scope.new_field());
        Aggr::db(self.table, *alias)
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
    ) -> Aggr<'outer, S, Option<f64>> {
        let expr = Func::avg(val.build_expr(self.ast.builder()));
        self.select(expr)
    }

    /// Return the maximum value in a column, this is [None] if there are zero rows.
    pub fn max<T>(&'inner self, val: impl Value<'inner, S, Typ = T>) -> Aggr<'outer, S, Option<T>>
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
    ) -> UnwrapOr<Aggr<'outer, S, Option<T>>, Const<T>>
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
    ) -> UnwrapOr<Aggr<'outer, S, Option<i64>>, Const<i64>>
    where
        T: EqTyp,
    {
        let expr = Func::count_distinct(val.build_expr(self.ast.builder()));
        UnwrapOr(self.select(expr), Const(0))
    }

    /// Return whether there are any rows.
    pub fn exists(&'inner self) -> IsNotNull<Aggr<'outer, S, Option<i64>>> {
        let expr = SimpleExpr::Constant(1.into_value());
        IsNotNull(self.select::<i64>(expr))
    }
}

pub struct Aggr<'t, S, T> {
    pub(crate) table: MyAlias,
    pub(crate) _p: PhantomData<fn(&'t S) -> &'t S>,
    pub(crate) _p2: PhantomData<T>,
    pub(crate) field: Field,
}

impl<S, T> Aggr<'_, S, T> {
    fn db(table: MyAlias, field: Field) -> Self {
        Aggr {
            _p: PhantomData,
            _p2: PhantomData,
            field,
            table,
        }
    }
}

impl<S, T> Copy for Aggr<'_, S, T> {}
impl<S, T> Clone for Aggr<'_, S, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'t, S, T: MyTyp> Typed for Aggr<'t, S, T> {
    type Typ = T;
}

impl<'t, S, T: MyTyp> Value<'t, S> for Aggr<'t, S, T> {
    fn build_expr(&self, _: crate::value::ValueBuilder) -> SimpleExpr {
        Expr::col((self.table, self.field)).into()
    }
}

impl<S, T: Table> Deref for Aggr<'_, S, T> {
    type Target = T::Ext<Self>;

    fn deref(&self) -> &Self::Target {
        RefCast::ref_cast(self)
    }
}
