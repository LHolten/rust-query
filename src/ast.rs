use std::rc::Rc;

use sea_query::{Alias, Asterisk, Condition, Expr, NullAlias, SelectStatement, SimpleExpr};

use crate::{
    alias::{Field, MyAlias, RawAlias, Scope},
    value::{DynTyped, DynTypedExpr, Typed, ValueBuilder},
};

#[derive(Default, Clone)]
pub struct MySelect {
    // this is used to check which `MySelect` a table is from
    pub(crate) scope_rc: Rc<()>,
    // tables to join, adding more requires mutating
    pub(super) tables: Vec<String>,
    // all conditions to check
    pub(super) filters: Vec<DynTyped<bool>>,
    // values that must be returned/ filtered on
    pub(super) filter_on: Vec<DynTypedExpr>,
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
    pub fn full(self: Rc<Self>) -> ValueBuilder {
        ValueBuilder {
            scope: Scope::create(self.tables.len(), self.filter_on.len()),
            extra: Default::default(),
            select: Default::default(),
            from: self,
            forwarded: Default::default(),
        }
    }
}

impl ValueBuilder {
    pub fn simple(&mut self, select: Vec<DynTypedExpr>) -> (SelectStatement, Vec<Field>) {
        self.build_select(false, select)
    }

    pub fn build_select(
        &mut self,
        must_group: bool,
        select_out: Vec<DynTypedExpr>,
    ) -> (SelectStatement, Vec<Field>) {
        let mut select = SelectStatement::new();
        let from = self.from.clone();

        // this stuff adds more to the self.extra list
        self.select = select_out
            .into_iter()
            .map(|val| ((val.0)(self), self.scope.new_field()))
            .collect();
        let filters: Vec<_> = from.filters.iter().map(|x| x.build_expr(self)).collect();
        let filter_on: Vec<_> = from.filter_on.iter().map(|val| (val.0)(self)).collect();

        let mut any_from = false;
        for (idx, table) in from.tables.iter().enumerate() {
            let tbl_ref = (Alias::new("main"), RawAlias(table.clone()));
            select.from_as(tbl_ref, MyAlias::new(idx));
            any_from = true;
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
                    select.join_subquery(join_type, ast.as_ref().clone(), *table_alias, cond);
                }
                SourceKind::Implicit(table) => {
                    let join_type = sea_query::JoinType::LeftJoin;
                    let tbl_ref = (Alias::new("main"), Alias::new(table));
                    select.join_as(join_type, tbl_ref, *table_alias, cond);
                }
            }
        }

        for filter in filters {
            select.and_where(filter);
        }

        let mut any_expr = false;
        let mut any_group = false;
        for (idx, group) in filter_on.into_iter().enumerate() {
            select.expr_as(group.clone(), MyAlias::new(idx));
            any_expr = true;

            // this constant refers to the 1 indexed output column.
            // should work on postgresql and sqlite.
            let constant = SimpleExpr::Constant(sea_query::Value::BigInt(Some((idx + 1) as i64)));
            select.add_group_by([constant]);
            any_group = true;
        }

        for (aggr, alias) in &*self.select {
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

        let out_fields = self.select.iter().map(|(k, v)| v.clone()).collect();
        (select, out_fields)
    }
}
