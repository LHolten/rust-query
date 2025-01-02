use elsa::FrozenVec;
use sea_query::{Alias, Asterisk, Condition, Expr, NullAlias, SelectStatement, SimpleExpr};

use crate::{
    alias::{Field, MyAlias, RawAlias, Scope},
    mymap::MyMap,
    value::{DynTypedExpr, ValueBuilder},
};

#[derive(Default)]
pub struct MySelect {
    pub(super) scope: Scope,
    // tables to join, adding more requires mutating
    pub(super) tables: Vec<(String, MyAlias)>,
    // implicit joins
    pub(super) extra: MyMap<Source, MyAlias>,
    // all conditions to check
    pub(super) filters: FrozenVec<Box<SimpleExpr>>,
    // calculating these results
    pub(super) select: MyMap<SimpleExpr, Field>,
    // values that must be returned/ filtered on
    pub(super) filter_on: FrozenVec<Box<(SimpleExpr, MyAlias)>>,
}

#[derive(PartialEq)]
pub(super) struct Source {
    pub(super) conds: Vec<(Field, SimpleExpr)>,
    pub(super) kind: SourceKind,
}

pub(super) enum SourceKind {
    Aggregate(SelectStatement),
    // table and pk
    Implicit(String),
}

impl PartialEq for SourceKind {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Implicit(l0), Self::Implicit(r0)) => l0 == r0,
            (Self::Aggregate(l0), Self::Aggregate(l1)) => l0 == l1,
            _ => false,
        }
    }
}

impl MySelect {
    pub fn builder(&self) -> ValueBuilder<'_> {
        ValueBuilder { inner: self }
    }

    pub fn cache(&self, exprs: impl IntoIterator<Item = DynTypedExpr>) -> Vec<Field> {
        exprs
            .into_iter()
            .map(|val| {
                let expr = (val.0)(self.builder());
                let new_field = || self.scope.new_field();
                *self.select.get_or_init(expr, new_field)
            })
            .collect()
    }

    pub fn simple(&self) -> SelectStatement {
        let mut select = self.build_select(false);
        for (aggr, _alias) in self.select.iter() {
            select.order_by_expr(aggr.clone(), sea_query::Order::Asc);
        }
        select
    }

    pub fn build_select(&self, is_group: bool) -> SelectStatement {
        let mut select = SelectStatement::new();

        let mut any_from = false;
        for (table, alias) in &self.tables {
            select.from_as(RawAlias(table.clone()), *alias);
            any_from = true
        }

        if !any_from {
            select.from_values([1], NullAlias);
        }

        for (source, table_alias) in self.extra.iter() {
            let mut cond = Condition::all();
            for (field, outer_value) in &source.conds {
                let id_field = Expr::expr(outer_value.clone());
                let id_field2 = Expr::col((*table_alias, *field));
                let filter = id_field.eq(id_field2);
                cond = cond.add(filter);
            }

            match &source.kind {
                SourceKind::Aggregate(ast) => {
                    let join_type = sea_query::JoinType::LeftJoin;
                    select.join_subquery(join_type, ast.clone(), *table_alias, cond);
                }
                SourceKind::Implicit(table) => {
                    let join_type = sea_query::JoinType::LeftJoin;
                    select.join_as(join_type, Alias::new(table), *table_alias, cond);
                }
            }
        }

        for filter in &self.filters {
            select.and_where(filter.clone());
        }

        let mut any_expr = false;
        let mut any_group = false;
        for (group, alias) in self.filter_on.iter() {
            any_expr = true;

            select.expr_as(group.clone(), *alias);
            if is_group {
                any_group = true;
                select.add_group_by([group.clone()]);
            }
        }

        for (aggr, alias) in self.select.iter() {
            any_expr = true;
            select.expr_as(aggr.clone(), *alias);
        }

        if !any_expr {
            select.expr_as(Expr::val(1), NullAlias);
        }

        if !any_group && is_group {
            select.expr_as(Expr::count(Expr::col(Asterisk)), NullAlias);
        }

        select
    }
}
