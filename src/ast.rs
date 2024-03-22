use elsa::FrozenVec;
use sea_query::{Alias, Condition, Expr, SelectStatement, SimpleExpr};

use crate::value::{AnyAlias, MyAlias, MyTableAlias};

#[derive(Default)]
pub struct MySelect {
    // the sources to use
    pub(super) sources: FrozenVec<Box<Source>>,
    // all conditions to check
    pub(super) filters: Vec<SimpleExpr>,
    // distinct on
    pub(super) group: FrozenVec<Box<(AnyAlias, SimpleExpr)>>,
    // calculating these agregates
    pub(super) aggr: Vec<(MyAlias, SimpleExpr)>,
    // sort on value (and keep row with smallest value)
    pub(super) sort: Vec<(MyAlias, SimpleExpr)>,
}

pub struct MyTable {
    pub(super) name: &'static str,
    pub(super) columns: FrozenVec<Box<(&'static str, AnyAlias)>>,
}

pub(super) enum Source {
    Select(MySelect),
    // table and pk
    Table(MyTable),
}

impl MySelect {
    pub fn into_select(&self) -> SelectStatement {
        let mut select = SelectStatement::new();
        for source in self.sources.iter() {
            match source {
                Source::Select(join) => {
                    select.join_lateral(
                        sea_query::JoinType::InnerJoin,
                        join.into_select(),
                        MyAlias::new().into_alias(),
                        Condition::any(),
                    );
                }
                Source::Table(table) => table.join(&mut select),
            }
        }

        for filter in &self.filters {
            select.and_where(filter.clone());
        }

        for (alias, group) in self.group.iter() {
            select.expr_as(group.clone(), alias.into_alias());
            select.group_by_col(alias.into_alias());
            select.order_by(alias.into_alias(), sea_query::Order::Asc);
        }

        for (alias, aggr) in &self.aggr {
            select.expr_as(aggr.clone(), alias.into_alias());
        }

        for (alias, sort) in &self.sort {
            select.expr_as(sort.clone(), alias.into_alias());
            select.order_by(alias.into_alias(), sea_query::Order::Asc);
        }

        select
    }
}

impl MyTable {
    pub fn join(&self, select: &mut SelectStatement) {
        if self.columns.is_empty() {
            return;
        }

        let tbl_alias = MyAlias::new();
        select.join_as(
            sea_query::JoinType::InnerJoin,
            Alias::new(self.name),
            tbl_alias.into_alias(),
            Condition::any(),
        );

        for (col, alias) in self.columns.iter() {
            let col_alias = Alias::new(*col);
            select.expr_as(
                Expr::col((tbl_alias.into_alias(), col_alias)),
                alias.into_alias(),
            );

            if let AnyAlias::Table(alias) = alias {
                alias.table.join(select)
            }
        }
    }
}
