use std::rc::Rc;

use sea_query::{Alias, Asterisk, Condition, Expr, NullAlias, SelectStatement, SimpleExpr};

use crate::{
    alias::{Field, MyAlias, RawAlias},
    value::{DynTypedExpr, ValueBuilder},
};

#[derive(Default)]
pub struct MySelect {
    // contains implicit joins etc necessary for the filters below
    pub(crate) builder: ValueBuilder,
    // tables to join, adding more requires mutating
    pub(super) tables: Vec<(String, MyAlias)>,
    // all conditions to check
    pub(super) filters: Vec<SimpleExpr>,
    // values that must be returned/ filtered on
    pub(super) filter_on: Vec<(SimpleExpr, MyAlias)>,
}

#[derive(PartialEq, Clone)]
pub(super) struct Source {
    pub(super) conds: Vec<(Field, SimpleExpr)>,
    pub(super) kind: SourceKind,
}

#[derive(Clone)]
pub(super) enum SourceKind {
    Aggregate(Rc<SelectStatement>),
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
    pub fn simple(&self, select: Vec<DynTypedExpr>) -> (SelectStatement, Vec<Field>) {
        self.build_select(false, select)
    }

    pub fn build_select(
        &self,
        must_group: bool,
        select_out: Vec<DynTypedExpr>,
    ) -> (SelectStatement, Vec<Field>) {
        let mut select = SelectStatement::new();

        let mut builder = ValueBuilder {
            scope: self.builder.scope.tmp_copy(),
            extra: self.builder.extra.clone(),
            select: self.builder.select.clone(),
        };
        let out_fields = builder.cache(select_out);

        let mut any_from = false;
        for (table, alias) in &self.tables {
            let tbl_ref = (Alias::new("main"), RawAlias(table.clone()));
            select.from_as(tbl_ref, *alias);
            any_from = true
        }

        if !any_from {
            select.from_values([1], NullAlias);
        }

        for (source, table_alias) in builder.extra.iter() {
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
                    select.join_subquery(join_type, ast.as_ref().clone(), *table_alias, cond);
                }
                SourceKind::Implicit(table) => {
                    let join_type = sea_query::JoinType::LeftJoin;
                    let tbl_ref = (Alias::new("main"), Alias::new(table));
                    select.join_as(join_type, tbl_ref, *table_alias, cond);
                }
            }
        }

        for filter in &self.filters {
            select.and_where(filter.clone());
        }

        let mut any_expr = false;
        let mut any_group = false;
        for (group, alias) in &self.filter_on {
            select.expr_as(group.clone(), *alias);
            any_expr = true;

            select.add_group_by([group.clone()]);
            any_group = true;
        }

        for (aggr, alias) in builder.select.iter() {
            select.expr_as(aggr.clone(), *alias);
            any_expr = true;
        }

        if !any_expr {
            select.expr_as(Expr::val(1), NullAlias);
            any_expr = true
        }
        assert!(any_expr);

        if !any_group && must_group {
            select.expr_as(Expr::count(Expr::col(Asterisk)), NullAlias);
            any_group = true;
        }
        assert_eq!(any_group, must_group);

        (select, out_fields)
    }
}
