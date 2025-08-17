use std::rc::Rc;

use sea_query::{Alias, Asterisk, Condition, Expr, ExprTrait, NullAlias, SelectStatement};

use crate::{
    alias::{Field, JoinableTable, MyAlias, Scope},
    value::{DynTyped, DynTypedExpr, Typed, ValueBuilder},
};

#[derive(Default, Clone)]
pub struct MySelect {
    // this is used to check which `MySelect` a table is from
    pub(crate) scope_rc: Rc<()>,
    // tables to join, adding more requires mutating
    pub(super) tables: Vec<JoinableTable>,
    // all conditions to check
    pub(super) filters: Vec<DynTyped<bool>>,
}

#[derive(PartialEq, Clone)]
pub(super) struct Source {
    pub(super) conds: Vec<(Field, sea_query::Expr)>,
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
            scope: Scope::create(self.tables.len(), 0),
            extra: Default::default(),
            from: self,
            forwarded: Default::default(),
        }
    }
}

impl ValueBuilder {
    pub fn simple_one(&mut self, val: DynTypedExpr) -> (SelectStatement, MyAlias) {
        let (a, mut b) = self.simple(vec![val]);
        assert!(b.len() == 1);
        (a, b.swap_remove(0))
    }

    pub fn simple(&mut self, select: Vec<DynTypedExpr>) -> (SelectStatement, Vec<MyAlias>) {
        let res = self.build_select(false, select);
        assert!(self.forwarded.is_empty());
        res
    }

    pub fn build_select(
        &mut self,
        must_group: bool,
        select_out: Vec<DynTypedExpr>,
    ) -> (SelectStatement, Vec<MyAlias>) {
        let mut select = SelectStatement::new();
        let from = self.from.clone();

        // this stuff adds more to the self.extra list and self.forwarded list
        let select_out: Vec<_> = select_out.into_iter().map(|val| (val.0)(self)).collect();
        let filters: Vec<_> = from.filters.iter().map(|x| x.build_expr(self)).collect();

        let mut any_from = false;
        for (idx, table) in from.tables.iter().enumerate() {
            match table {
                JoinableTable::Normal(table_name) => {
                    let tbl_ref = ("main", table_name.clone());
                    select.from_as(tbl_ref, MyAlias::new(idx));
                }
                JoinableTable::Pragma(func) => {
                    select.from_function(func.clone(), MyAlias::new(idx));
                }
            }
            any_from = true;
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

        for (idx, group) in self.forwarded.iter().enumerate() {
            select.from_as((Alias::new("main"), Alias::new(group.1.0)), group.1.2);
            any_from = true;

            select.expr_as(
                Expr::column((group.1.2, Alias::new("id"))),
                MyAlias::new(idx),
            );
            any_expr = true;

            // this constant refers to the 1 indexed output column.
            // should work on postgresql and sqlite.
            let constant =
                sea_query::Expr::Constant(sea_query::Value::BigInt(Some((idx + 1) as i64)));
            select.add_group_by([constant]);
            any_group = true;
        }

        let forwarded_len = self.forwarded.len();

        let mut out_fields = vec![];
        for (idx, aggr) in select_out.into_iter().enumerate() {
            let alias = MyAlias::new(forwarded_len + idx);
            out_fields.push(alias);
            select.expr_as(aggr, alias);
            any_expr = true;
        }

        if !any_from {
            select.from_values([1], NullAlias);
            any_from = true;
        }
        assert!(any_from);

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
