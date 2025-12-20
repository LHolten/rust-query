use std::rc::Rc;

use sea_query::{Alias, Condition, Expr, ExprTrait, JoinType, NullAlias, SelectStatement};

use crate::{
    alias::{Field, JoinableTable, MyAlias, Scope},
    value::{DynTypedExpr, ValueBuilder},
};

#[derive(Default, Clone)]
pub struct MySelect {
    // this is used to check which `MySelect` a table is from
    pub(crate) scope_rc: Rc<()>,
    // tables to join, adding more requires mutating
    pub(super) tables: Vec<JoinableTable>,
    // all conditions to check
    pub(super) filters: Vec<DynTypedExpr>,
}

#[derive(PartialEq, Clone)]
pub(super) struct Source {
    pub(super) conds: Vec<(Field, sea_query::Expr)>,
    pub(super) kind: SourceKind,
}

#[derive(Clone)]
pub(super) enum SourceKind {
    Aggregate(Rc<SelectStatement>),
    Implicit(String, JoinType),
}

impl PartialEq for SourceKind {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            // TODO: possible optimization to unify the join_type?
            (Self::Implicit(l0, l1), Self::Implicit(r0, r1)) => l0 == r0 && l1 == r1,
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
        let (a, b) = self.simple(vec![val]);
        let [b] = b.try_into().unwrap();
        (a, b)
    }

    pub fn simple(&mut self, select: Vec<DynTypedExpr>) -> (SelectStatement, Vec<MyAlias>) {
        self.simple_ordered(select, Vec::new())
    }

    pub fn simple_ordered(
        &mut self,
        select: Vec<DynTypedExpr>,
        order_by: Vec<(DynTypedExpr, sea_query::Order)>,
    ) -> (SelectStatement, Vec<MyAlias>) {
        let res = self.build_select(select, order_by);
        assert!(self.forwarded.is_empty());
        res
    }

    pub fn build_select(
        &mut self,
        select_out: Vec<DynTypedExpr>,
        order_by: Vec<(DynTypedExpr, sea_query::Order)>,
    ) -> (SelectStatement, Vec<MyAlias>) {
        let mut select = SelectStatement::new();
        let from = self.from.clone();

        // this stuff adds more to the self.extra list and self.forwarded list
        let select_out: Vec<_> = select_out.into_iter().map(|val| (val.func)(self)).collect();
        let filters: Vec<_> = from.filters.iter().map(|x| (x.func)(self)).collect();
        let order_by: Vec<_> = order_by
            .into_iter()
            .map(|(x, o)| ((x.func)(self), o))
            .collect();

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

        let mut need_from = false;
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
                SourceKind::Implicit(table, join_type) => {
                    let tbl_ref = (Alias::new("main"), Alias::new(table));
                    select.join_as(*join_type, tbl_ref, *table_alias, cond);
                }
            }
            need_from = true;
        }

        for filter in filters {
            select.and_where(filter);
        }

        let mut any_expr = false;

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
        }

        let forwarded_len = self.forwarded.len();

        let mut out_fields = vec![];
        for (idx, aggr) in select_out.into_iter().enumerate() {
            let alias = MyAlias::new(forwarded_len + idx);
            out_fields.push(alias);
            select.expr_as(aggr, alias);
            any_expr = true;
        }

        for (key, order) in order_by {
            select.order_by_expr(key, order);
        }

        if need_from && !any_from {
            select.from_subquery(SelectStatement::new().expr(CONST_1).take(), NullAlias);
            any_from = true;
        }
        assert!(any_from || !need_from);

        if !any_expr {
            select.expr_as(CONST_1, NullAlias);
            any_expr = true
        }
        assert!(any_expr);

        (select, out_fields)
    }
}

pub const CONST_1: Expr = Expr::Constant(sea_query::Value::BigInt(Some(1)));
pub const CONST_0: Expr = Expr::Constant(sea_query::Value::BigInt(Some(0)));
