use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use sea_query::{Alias, Expr, Func, SimpleExpr};

use crate::{
    alias::{Field, MyAlias},
    ast::MySelect,
    db::DbCol,
    query::Query,
    value::{IsNotNull, UnwrapOr, Value},
};

/// This is the query type used in sub-queries.
/// It can only produce one result (for each outer result).
/// This type dereferences to [Query].
pub struct Aggregate<'outer, 'inner> {
    pub(crate) outer_ast: &'inner MySelect,
    pub(crate) conds: &'inner mut Vec<(MyAlias, SimpleExpr)>,
    pub(crate) query: Query<'inner>,
    pub(crate) table: MyAlias,
    pub(crate) phantom2: PhantomData<fn(&'outer ()) -> &'outer ()>,
}

impl<'outer, 'inner> Deref for Aggregate<'outer, 'inner> {
    type Target = Query<'inner>;

    fn deref(&self) -> &Self::Target {
        &self.query
    }
}

impl<'outer, 'inner> DerefMut for Aggregate<'outer, 'inner> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.query
    }
}

impl<'outer, 'inner> Aggregate<'outer, 'inner> {
    /// Filter the rows of this sub-query based on a value from the outer query.
    pub fn filter_on<T>(
        &mut self,
        val: impl Value<'inner, Typ = T>,
        on: impl Value<'outer, Typ = T>,
    ) {
        let alias = MyAlias::new();
        self.conds
            .push((alias, on.build_expr(self.outer_ast.builder())));
        self.ast
            .filter_on
            .push(Box::new((val.build_expr(self.ast.builder()), alias)))
    }

    /// Return the average value in a column, this is [None] if there are zero rows.
    pub fn avg<V: Value<'inner, Typ = i64>>(&'inner self, val: V) -> DbCol<'outer, Option<i64>> {
        let expr = Func::cast_as(
            Func::avg(val.build_expr(self.ast.builder())),
            Alias::new("integer"),
        );
        let alias = self.ast.select.get_or_init(expr.into(), Field::new);
        DbCol::db(self.table, *alias)
    }

    /// Return the maximum value in a column, this is [None] if there are zero rows.
    pub fn max<V: Value<'inner, Typ = i64>>(&'inner self, val: V) -> DbCol<'outer, Option<i64>> {
        let expr = Func::max(val.build_expr(self.ast.builder()));
        let alias = self.ast.select.get_or_init(expr.into(), Field::new);
        DbCol::db(self.table, *alias)
    }

    /// Return the sum of a column.
    pub fn sum_float<V: Value<'inner, Typ = f64>>(
        &'inner self,
        val: V,
    ) -> UnwrapOr<DbCol<'outer, Option<f64>>, f64> {
        let expr = Func::cast_as(
            Func::sum(val.build_expr(self.ast.builder())),
            Alias::new("integer"),
        );
        let alias = self.ast.select.get_or_init(expr.into(), Field::new);
        UnwrapOr(DbCol::db(self.table, *alias), 0.)
    }

    /// Return the number of distinct values in a column.
    pub fn count_distinct<V: Value<'inner>>(
        &'inner self,
        val: V,
    ) -> UnwrapOr<DbCol<'outer, Option<i64>>, i64> {
        let expr = Func::count_distinct(val.build_expr(self.ast.builder()));
        let alias = self.ast.select.get_or_init(expr.into(), Field::new);
        UnwrapOr(DbCol::db(self.table, *alias), 0)
    }

    /// Return whether there are any rows.
    pub fn exists(&'inner self) -> IsNotNull<DbCol<'outer, i64>> {
        let expr = Expr::val(1);
        let alias = self.ast.select.get_or_init(expr.into(), Field::new);
        IsNotNull(DbCol::db(self.table, *alias))
    }
}
