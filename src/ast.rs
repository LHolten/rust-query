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
    pub(super) group: OnceCell<(SimpleExpr, &'static str, &'static str, MyAlias, MyAlias)>,
    // calculating these agregates
    pub(super) select: MyMap<SimpleExpr, MyAlias>,
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
    Table(&'static str, Joins),
}

impl Joins {
    pub fn wrap(
        &self,
        inner: &MySelect,
        offset: usize,
        last: &FrozenVec<Box<(MyAlias, SimpleExpr)>>,
    ) -> SelectStatement {
        let mut select = SelectStatement::new();
        select.from_values([1], NullAlias);
        inner.join(self, &mut select);

        if last.is_empty() {
            select.expr_as(Expr::val(1), NullAlias);
        }
        for (alias, expr) in last.iter() {
            select.expr_as(expr.clone(), *alias);
            select.order_by(*alias, sea_query::Order::Asc);
        }

        // TODO: Figure out how to do this properly
        select.offset(offset as u64);
        select.limit(1000000000);

        select
    }
}

impl MySelect {
    pub fn join(&self, joins: &Joins, select: &mut SelectStatement) {
        if let Some((_group, table, id, table_alias, alias)) = self.group.get() {
            select.join_as(
                sea_query::JoinType::InnerJoin,
                Alias::new(*table),
                *table_alias,
                Condition::all(),
            );

            let id_field = Expr::col((*table_alias, Alias::new(*id)));
            let id_field2 = Expr::col((joins.table, *alias));
            let filter = id_field.eq(id_field2);
            select.join_subquery(
                sea_query::JoinType::LeftJoin,
                self.build_select(),
                joins.table,
                Condition::all().add(filter),
            );
        } else {
            select.join_subquery(
                sea_query::JoinType::InnerJoin,
                self.build_select(),
                joins.table,
                Condition::all(),
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
                        Alias::new(*table),
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

        if let Some((group, _table, _id, _table_alias, alias)) = self.group.get() {
            select.expr_as(group.clone(), *alias);
            select.group_by_col(*alias);
        }

        if self.select.is_empty() {
            select.expr_as(Expr::val(1), NullAlias);
        }
        for (aggr, alias) in self.select.iter() {
            select.expr_as(aggr.clone(), *alias);
        }

        select
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
