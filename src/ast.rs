use elsa::FrozenVec;
use sea_query::{Alias, Condition, Expr, SelectStatement, SimpleExpr};

use crate::value::{AnyAlias, MyAlias, MyTableAlias};

#[derive(Default)]
pub struct MySelect {
    // the sources to use
    pub(super) sources: FrozenVec<Box<Source>>,
    // all conditions to check
    pub(super) filters: FrozenVec<Box<SimpleExpr>>,
    // distinct on
    pub(super) group: FrozenVec<Box<(AnyAlias, SimpleExpr)>>,
    // calculating these agregates
    pub(super) aggr: FrozenVec<Box<(MyAlias, SimpleExpr)>>,
    // sort on value (and keep row with smallest value)
    pub(super) sort: FrozenVec<Box<(AnyAlias, SimpleExpr)>>,
}

pub struct MyTable {
    pub(super) name: &'static str,
    pub(super) id: &'static str,
    pub(super) columns: FrozenVec<Box<(&'static str, AnyAlias)>>,
}

pub(super) enum Source {
    Select(MySelect),
    // table and pk
    Table(MyTableAlias),
}

impl MySelect {
    pub fn build_select(&self) -> SelectStatement {
        let mut select = SelectStatement::new();
        select.from_values([1], Alias::new("_"));
        for source in self.sources.iter() {
            match source {
                Source::Select(join) => {
                    select.join_subquery(
                        sea_query::JoinType::InnerJoin,
                        join.build_select(),
                        MyAlias::new().into_alias(),
                        Condition::all(),
                    );
                }
                Source::Table(alias) => {
                    let item = (alias.table.id, AnyAlias::Value(alias.val));
                    alias.table.columns.push(Box::new(item));

                    alias.table.join(None, &mut select)
                }
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
    pub fn join(&self, filter: Option<MyAlias>, select: &mut SelectStatement) {
        if self.columns.is_empty() {
            return;
        }

        let tbl_alias = MyAlias::new();
        let filter = filter.map(|pk| {
            Expr::col((tbl_alias.into_alias(), Alias::new(self.id))).equals(pk.into_alias())
        });

        select.join_as(
            sea_query::JoinType::InnerJoin,
            Alias::new(self.name),
            tbl_alias.into_alias(),
            Condition::all().add_option(filter),
        );

        for (col, alias) in self.columns.iter() {
            let col_alias = Alias::new(*col);
            select.expr_as(
                Expr::col((tbl_alias.into_alias(), col_alias)),
                alias.into_alias(),
            );

            if let AnyAlias::Table(alias) = alias {
                alias.table.join(Some(alias.val), select)
            }
        }
    }
}
