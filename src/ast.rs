use std::{cell::Cell, fmt};

use elsa::FrozenVec;
use sea_query::{Alias, Asterisk, Condition, Expr, NullAlias, SelectStatement, SimpleExpr};

use crate::{
    mymap::MyMap,
    value::{Field, FieldAlias, MyAlias, RawAlias},
};

#[derive(Default)]
pub struct MySelect {
    // the sources to use
    pub(super) sources: FrozenVec<Box<Source>>,
    // all conditions to check
    pub(super) filters: FrozenVec<Box<SimpleExpr>>,
    // calculating these agregates
    pub(super) select: MyMap<SimpleExpr, Field>,
    // values that must be returned/ filtered on
    pub(super) filter_on: FrozenVec<Box<(SimpleExpr, MyAlias, SimpleExpr)>>,
    // is this a grouping select
    pub(super) group: Cell<bool>,
}

pub struct MyTable {
    // pub(super) name: (&'static str, MyAlias),
    pub(super) name: &'static str,
    pub(super) id: &'static str,
    // pub(super) joins: FrozenVec<Box<(&'static str, MyTable)>>,
    pub(super) joins: Joins,
}

pub(super) struct Joins {
    pub(super) table: MyAlias,
    pub(super) joined: FrozenVec<Box<(Field, MyTable)>>,
}

impl fmt::Debug for MyTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MyTable")
            .field("name", &self.name)
            .field("id", &self.id)
            // .field("columns", &self.joins.iter().collect::<Vec<_>>())
            .finish()
    }
}

pub(super) enum Source {
    Select(MySelect, Joins),
    // table and pk
    Table(String, Joins),
}

impl MySelect {
    pub fn simple(&self, offset: usize, limit: u32) -> SelectStatement {
        let mut select = self.build_select();

        let mut cond = Condition::all();
        for (inner_value, _alias, outer_value) in self.filter_on.iter() {
            let id_field = Expr::expr(outer_value.clone());
            let id_field2 = Expr::expr(inner_value.clone());
            let filter = id_field.eq(id_field2);
            cond = cond.add(filter);
        }
        select.cond_where(cond);

        // TODO: Figure out how to do this properly
        select.offset(offset as u64);
        select.limit((limit as u64).min(18446744073709551610));

        select
    }

    pub fn join(&self, joins: &Joins, select: &mut SelectStatement) {
        let mut cond = Condition::all();
        for (_, alias, outer_value) in self.filter_on.iter() {
            let id_field = Expr::expr(outer_value.clone());
            let id_field2 = Expr::col((joins.table, *alias));
            let filter = id_field.eq(id_field2);
            cond = cond.add(filter);
        }

        if self.group.get() {
            select.join_subquery(
                sea_query::JoinType::LeftJoin,
                self.build_select(),
                joins.table,
                cond,
            );
        } else {
            select.join_subquery(
                sea_query::JoinType::InnerJoin,
                self.build_select(),
                joins.table,
                cond,
            );
        }

        for (col, table) in joins.joined.iter() {
            let field = joins.col_alias(*col);
            table.join(field, select)
        }
    }

    pub fn build_select(&self) -> SelectStatement {
        let mut select = SelectStatement::new();
        select.from_values([1], NullAlias);
        for source in self.sources.iter() {
            match source {
                Source::Select(q, joins) => q.join(joins, &mut select),
                Source::Table(table, joins) => {
                    select.join_as(
                        sea_query::JoinType::InnerJoin,
                        RawAlias(table.clone()),
                        joins.table,
                        Condition::all(),
                    );

                    for (col, table) in joins.joined.iter() {
                        let field = joins.col_alias(*col);
                        table.join(field, &mut select)
                    }
                }
            }
        }

        for filter in &self.filters {
            select.and_where(filter.clone());
        }

        let mut any_expr = false;
        let mut any_group = false;
        for (group, alias, _) in self.filter_on.iter() {
            any_expr = true;

            select.expr_as(group.clone(), *alias);
            if self.group.get() {
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

        if !any_group && self.group.get() {
            select.expr_as(Expr::count(Expr::col(Asterisk)), NullAlias);
        }

        select
    }

    pub fn add_select(&self, expr: impl Into<SimpleExpr>) -> &Field {
        self.select.get_or_init(expr.into(), Field::new)
    }
}

impl Joins {
    pub fn col_alias(&self, col: Field) -> FieldAlias {
        FieldAlias {
            table: self.table,
            col,
        }
    }
}

impl MyTable {
    pub fn id_alias(&self) -> FieldAlias {
        self.joins.col_alias(Field::Str(self.id))
    }

    pub fn join(&self, filter: FieldAlias, select: &mut SelectStatement) {
        let id_field = self.id_alias();
        let filter = Expr::col(id_field).equals(filter);

        select.join_as(
            sea_query::JoinType::LeftJoin,
            Alias::new(self.name),
            self.joins.table,
            Condition::all().add(filter),
        );

        for (col, table) in self.joins.joined.iter() {
            let field = self.joins.col_alias(*col);
            table.join(field, select)
        }
    }
}
