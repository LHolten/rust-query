use elsa::FrozenVec;
use sea_query::{Alias, Asterisk, Condition, Expr, NullAlias, SelectStatement, SimpleExpr};

use crate::{
    alias::{Field, MyAlias, RawAlias},
    mymap::MyMap,
    value::ValueBuilder,
};

#[derive(Default)]
pub struct MySelect {
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
    Aggregate(MySelect),
    // table and pk
    Implicit(String),
}

impl PartialEq for SourceKind {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Implicit(l0), Self::Implicit(r0)) => l0 == r0,
            _ => false,
        }
    }
}

impl MySelect {
    pub fn builder(&self) -> ValueBuilder<'_> {
        ValueBuilder { inner: self }
    }

    pub fn simple(&self) -> SelectStatement {
        self.build_select(false)
    }

    pub fn build_select(&self, is_group: bool) -> SelectStatement {
        let mut select = SelectStatement::new();
        select.from_values([1], NullAlias);

        for (table, alias) in &self.tables {
            select.join_as(
                sea_query::JoinType::InnerJoin,
                RawAlias(table.clone()),
                *alias,
                Condition::all(),
            );
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
                    select.join_subquery(join_type, ast.build_select(true), *table_alias, cond);
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
                select.group_by_col(*alias);
            }
        }

        for (aggr, alias) in self.select.iter() {
            any_expr = true;
            select.expr_as(aggr.clone(), *alias);
            select.order_by_expr(Expr::col(*alias).into(), sea_query::Order::Asc);
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

pub fn add_table(sources: &mut Vec<(String, MyAlias)>, name: String) -> MyAlias {
    let alias = MyAlias::new();
    sources.push((name, alias));
    alias
}
