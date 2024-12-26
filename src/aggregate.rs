use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
    rc::Rc,
};

use ref_cast::RefCast;
use sea_query::{Expr, Func, SelectStatement, SimpleExpr};

use crate::{
    alias::{Field, MyAlias},
    ast::MySelect,
    rows::Rows,
    value::{
        operations::{Const, IsNotNull, UnwrapOr},
        EqTyp, IntoColumn, MyTyp, NumTyp, Typed, ValueBuilder,
    },
    Column, Table,
};

/// This is the argument type used for aggregates.
///
/// While it is possible to join many tables in an aggregate, there can be only one result.
/// (The result can be a tuple or struct with multiple values though).
pub struct Aggregate<'outer, 'inner, S> {
    // pub(crate) outer_ast: &'inner MySelect,
    pub(crate) conds: Vec<(Field, Rc<dyn Fn(ValueBuilder) -> SimpleExpr>)>,
    pub(crate) query: Rows<'inner, S>,
    // pub(crate) table: MyAlias,
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

impl<'outer: 'inner, 'inner, S: 'static> Aggregate<'outer, 'inner, S> {
    fn select<T>(&'inner self, expr: impl Into<SimpleExpr>) -> Aggr<S, Option<T>> {
        let alias = self
            .ast
            .select
            .get_or_init(expr.into(), || self.ast.scope.new_field());
        Aggr {
            _p2: PhantomData,
            select: self.query.ast.build_select(true),
            field: *alias,
            conds: self.conds.clone(),
        }
    }

    /// Filter the rows of this sub-query based on a value from the outer query.
    pub fn filter_on<T>(
        &mut self,
        val: impl IntoColumn<'inner, S, Typ = T>,
        on: impl IntoColumn<'outer, S, Typ = T>,
    ) {
        let on = on.into_owned();
        let alias = self.ast.scope.new_alias();
        self.conds
            .push((Field::U64(alias), Rc::new(move |b| on.build_expr(b))));
        self.ast
            .filter_on
            .push(Box::new((val.build_expr(self.ast.builder()), alias)))
    }

    /// Return the average value in a column, this is [None] if there are zero rows.
    pub fn avg(
        &'inner self,
        val: impl IntoColumn<'inner, S, Typ = f64>,
    ) -> Column<'outer, S, Option<f64>> {
        let expr = Func::avg(val.build_expr(self.ast.builder()));
        self.select(expr).into_column()
    }

    /// Return the maximum value in a column, this is [None] if there are zero rows.
    pub fn max<T>(
        &'inner self,
        val: impl IntoColumn<'inner, S, Typ = T>,
    ) -> Column<'outer, S, Option<T>>
    where
        T: NumTyp,
    {
        let expr = Func::max(val.build_expr(self.ast.builder()));
        self.select(expr).into_column()
    }

    /// Return the sum of a column.
    pub fn sum<T>(&'inner self, val: impl IntoColumn<'inner, S, Typ = T>) -> Column<'outer, S, T>
    where
        T: NumTyp,
    {
        let expr = Func::sum(val.build_expr(self.ast.builder()));
        UnwrapOr(self.select::<T>(expr), Const(T::ZERO)).into_column()
    }

    /// Return the number of distinct values in a column.
    pub fn count_distinct<T>(
        &'inner self,
        val: impl IntoColumn<'inner, S, Typ = T>,
    ) -> Column<'outer, S, i64>
    where
        T: EqTyp,
    {
        let expr = Func::count_distinct(val.build_expr(self.ast.builder()));
        UnwrapOr(self.select::<i64>(expr), Const(0)).into_column()
    }

    /// Return whether there are any rows.
    pub fn exists(&'inner self) -> Column<'outer, S, bool> {
        let expr = SimpleExpr::Constant(1.into_sea_value());
        IsNotNull(self.select::<i64>(expr)).into_column()
    }
}

pub struct Aggr<S, T> {
    pub(crate) _p2: PhantomData<(S, T)>,
    pub(crate) select: SelectStatement,
    pub(crate) conds: Vec<(Field, Rc<dyn Fn(ValueBuilder) -> SimpleExpr>)>,
    pub(crate) field: Field,
}

impl<S, T> Clone for Aggr<S, T> {
    fn clone(&self) -> Self {
        Self {
            _p2: PhantomData,
            select: self.select.clone(),
            conds: self.conds.clone(),
            field: self.field,
        }
    }
}

impl<S, T: MyTyp> Typed for Aggr<S, T> {
    type Typ = T;
    fn build_expr(&self, b: crate::value::ValueBuilder) -> SimpleExpr {
        Expr::col((self.build_table(b), self.field)).into()
    }
}

impl<S, T> Aggr<S, T> {
    fn build_table(&self, b: crate::value::ValueBuilder) -> MyAlias {
        let conds = self.conds.iter().map(|(field, expr)| (*field, expr(b)));
        b.get_aggr(self.select.clone(), conds.collect())
    }
}

impl<'t, S: 'static, T: MyTyp> IntoColumn<'t, S> for Aggr<S, T> {
    type Owned = Self;

    fn into_owned(self) -> Self::Owned {
        self
    }
}

impl<S, T: Table> Deref for Aggr<S, T> {
    type Target = T::Ext<Self>;

    fn deref(&self) -> &Self::Target {
        RefCast::ref_cast(self)
    }
}

/// Perform an aggregate that returns a single result for each of the current rows.
///
/// You can filter the rows in the aggregate based on values from the outer query.
/// That is the only way to get a different aggregate for each outer row.
pub fn aggregate<'outer, S, F, R>(f: F) -> R
where
    F: for<'a> FnOnce(&'a mut Aggregate<'outer, 'a, S>) -> R,
{
    let ast = MySelect::default();
    let inner = Rows {
        phantom: PhantomData,
        ast,
    };
    let mut group = Aggregate {
        conds: Vec::new(),
        query: inner,
        phantom2: PhantomData,
    };
    f(&mut group)
}
