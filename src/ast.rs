use std::{cell::OnceCell, fmt};

use elsa::FrozenVec;
use sea_query::{Alias, Condition, Expr, NullAlias, SelectStatement, SimpleExpr};

use crate::{
    mymap::MyMap,
    value::{Field, FieldAlias, MyAlias},
};

#[derive(Default)]
pub struct MySelect {
    // the sources to use
    pub(super) sources: FrozenVec<Box<Source>>,
    // all conditions to check
    pub(super) filters: FrozenVec<Box<SimpleExpr>>,
    // distinct on
    pub(super) group: OnceCell<(SimpleExpr, &'static str, &'static str, MyAlias)>,
    // calculating these agregates
    pub(super) aggr: MyMap<SimpleExpr, MyAlias>,
    // sort on value (and keep row with smallest value)
    pub(super) sort: MyMap<SimpleExpr, MyAlias>,
}

pub struct MyTable {
    // pub(super) name: (&'static str, MyAlias),
    pub(super) name: &'static str,
    pub(super) id: &'static str,
    // pub(super) joins: FrozenVec<Box<(&'static str, MyTable)>>,
    pub(super) joins: Joins,
}

pub(super) struct Joins {
    pub(super) alias: MyAlias,
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
    Table(&'static str, Joins),
}

impl Joins {
    pub fn wrap(
        &self,
        inner: &SelectStatement,
        offset: usize,
        last: &FrozenVec<Box<(MyAlias, SimpleExpr)>>,
    ) -> SelectStatement {
        let mut select = SelectStatement::new();
        select.from_subquery(inner.clone(), self.alias);

        for (col, table) in self.joined.iter() {
            let field = self.col_alias(*col);
            table.join(field, &mut select)
        }

        select.expr_as(Expr::val(1), NullAlias);
        for (alias, expr) in last.iter() {
            select.expr_as(expr.clone(), *alias);
        }

        // TODO: Figure out how to do this properly
        select.offset(offset as u64);
        select.limit(1000000000);

        select
    }
}

impl MySelect {
    pub fn build_select(&self) -> SelectStatement {
        let mut select = SelectStatement::new();
        select.from_values([1], NullAlias);
        for source in self.sources.iter() {
            match source {
                Source::Select(q, joins) => {
                    select.join_subquery(
                        sea_query::JoinType::InnerJoin,
                        q.build_select(),
                        joins.alias,
                        Condition::all(),
                    );

                    for (col, table) in joins.joined.iter() {
                        let field = joins.col_alias(*col);
                        table.join(field, &mut select)
                    }
                }
                Source::Table(table, joins) => {
                    select.join_as(
                        sea_query::JoinType::InnerJoin,
                        Alias::new(*table),
                        joins.alias,
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

        if let Some((group, table, id, alias)) = self.group.get() {
            let table_alias = MyAlias::new();

            let id_field = Expr::col((table_alias, Alias::new(*id)));
            let filter = id_field.clone().eq(group.clone());

            select.join_as(
                sea_query::JoinType::RightJoin,
                Alias::new(*table),
                table_alias,
                Condition::all().add(filter),
            );

            select.expr_as(id_field, *alias);
            select.group_by_col(*alias);
        }

        select.expr_as(Expr::val(1), NullAlias);
        for (aggr, alias) in self.aggr.iter() {
            select.expr_as(aggr.clone(), *alias);
        }

        for (sort, alias) in self.sort.iter() {
            select.expr_as(sort.clone(), *alias);
            select.order_by(*alias, sea_query::Order::Asc);
        }

        select
    }
}

impl Joins {
    pub fn col_alias(&self, col: Field) -> FieldAlias {
        FieldAlias {
            table: self.alias,
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
            self.joins.alias,
            Condition::all().add(filter),
        );

        for (col, table) in self.joins.joined.iter() {
            let field = self.joins.col_alias(*col);
            table.join(field, select)
        }
    }
}
