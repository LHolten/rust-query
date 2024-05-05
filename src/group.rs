use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use sea_query::{Alias, Expr, Func};

use crate::{
    ast::Joins,
    value::{Db, Field, IsNotNull, MyAlias, MyIdenT, UnwrapOr, Value},
    Query,
};

pub struct GroupQuery<'outer, 'inner> {
    pub(crate) query: Query<'inner>,
    pub(crate) joins: &'outer Joins,
    pub(crate) phantom2: PhantomData<dyn Fn(&'outer ()) -> &'outer ()>,
}

impl<'outer, 'inner> Deref for GroupQuery<'outer, 'inner> {
    type Target = Query<'inner>;

    fn deref(&self) -> &Self::Target {
        &self.query
    }
}

impl<'outer, 'inner> DerefMut for GroupQuery<'outer, 'inner> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.query
    }
}

impl<'outer, 'inner> GroupQuery<'outer, 'inner> {
    pub fn filter_on<T: MyIdenT>(
        &mut self,
        val: impl Value<'inner, Typ = T>,
        on: impl Value<'outer, Typ = T>,
    ) {
        let alias = MyAlias::new();
        self.ast
            .filter_on
            .push(Box::new((val.build_expr(), alias, on.build_expr())))
    }

    pub fn avg<V: Value<'inner, Typ = i64>>(&'inner self, val: V) -> Db<'outer, Option<i64>> {
        let expr = Func::cast_as(Func::avg(val.build_expr()), Alias::new("integer"));
        let alias = self.ast.select.get_or_init(expr.into(), Field::new);
        Option::iden_any(self.joins, *alias)
    }

    pub fn max<V: Value<'inner, Typ = i64>>(&'inner self, val: V) -> Db<'outer, Option<i64>> {
        let expr = Func::max(val.build_expr());
        let alias = self.ast.select.get_or_init(expr.into(), Field::new);
        Option::iden_any(self.joins, *alias)
    }

    pub fn sum_float<V: Value<'inner, Typ = f64>>(
        &'inner self,
        val: V,
    ) -> UnwrapOr<Db<'outer, Option<f64>>, f64> {
        let expr = Func::cast_as(Func::sum(val.build_expr()), Alias::new("integer"));
        let alias = self.ast.select.get_or_init(expr.into(), Field::new);
        UnwrapOr(Option::iden_any(self.joins, *alias), 0.)
    }

    pub fn count_distinct<V: Value<'inner>>(
        &'inner self,
        val: V,
    ) -> UnwrapOr<Db<'outer, Option<i64>>, i64> {
        let expr = Func::count_distinct(val.build_expr());
        let alias = self.ast.select.get_or_init(expr.into(), Field::new);
        UnwrapOr(Option::iden_any(self.joins, *alias), 0)
    }

    pub fn exists(&'inner self) -> IsNotNull<Db<'outer, i64>> {
        let expr = Expr::val(1);
        let alias = self.ast.select.get_or_init(expr.into(), Field::new);
        IsNotNull(i64::iden_any(self.joins, *alias))
    }

    // // only one Group can exist at a time
    // pub fn group(&'inner mut self) -> Group<'outer, 'inner> {
    //     self.ast.group.set(true);
    //     Group { inner: self }
    // }
}
