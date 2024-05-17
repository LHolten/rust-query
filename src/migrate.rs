use std::{cell::Cell, marker::PhantomData};

use elsa::FrozenVec;
use rusqlite::Connection;
use sea_query::{
    Alias, InsertStatement, SchemaStatement, SelectStatement, SqliteQueryBuilder,
    TableCreateStatement, TableDropStatement, TableRenameStatement, TableStatement,
};

use crate::{
    ast::{Joins, MySelect, Source},
    client::QueryBuilder,
    insert::{Reader, Writable},
    mymap::MyMap,
    value::{Db, Field, FieldAlias, FkInfo, MyAlias, MyIdenT, Value},
    HasId, Row, Table,
};

// pub struct TableBuilder<'a>(PhantomData<dyn Fn(&'a ()) -> &'a ()>);

// impl<'a> TableBuilder<'a> {
//     pub fn new_column(&self, name: &'static str, v: impl Value<'a>) {
//         todo!()
//     }

//     pub fn drop_column(&self, name: &'static str) {
//         todo!()
//     }
// }

pub trait TableMigration<'a, A: HasId> {
    type T;

    fn into_new(self: Box<Self>, prev: Db<'a, A>) -> Box<dyn Writable<'a, T = Self::T>>;
}

pub struct SchemaBuilder<'x> {
    conn: &'x Connection,
    create: Vec<TableCreateStatement>,
    drop: Vec<TableDropStatement>,
    rename: Vec<TableRenameStatement>,
}

impl<'x> SchemaBuilder<'x> {
    pub fn migrate_table<M, A: HasId, B: HasId>(&mut self, name: &'static str, mut m: M)
    where
        M: for<'y, 'a> FnMut(Row<'y, 'a>, Db<'a, A>) -> Box<dyn TableMigration<'a, A, T = B>>,
    {
        let ast = MySelect {
            sources: FrozenVec::new(),
            filters: FrozenVec::new(),
            select: MyMap::default(),
            filter_on: FrozenVec::new(),
            group: Cell::new(false),
        };
        let limit = u32::MAX;

        let table = MyAlias::new();
        let Source::Table(_, joins) = ast.sources.push_get(Box::new(Source::Table(
            A::NAME.to_owned(),
            Joins {
                table,
                joined: FrozenVec::new(),
            },
        ))) else {
            panic!()
        };
        let db = FkInfo::<A>::joined(joins, Field::Str(A::ID));

        let mut select = ast.simple(0, limit);
        let sql = select.to_string(SqliteQueryBuilder);

        let conn = self.conn;
        let mut statement = conn.prepare(&sql).unwrap();
        let mut rows = statement.query([]).unwrap();

        // let mut out = vec![];
        let mut offset = 0;
        while let Some(row) = rows.next().unwrap() {
            let updated = Cell::new(false);

            {
                let row = Row {
                    offset,
                    limit,
                    inner: PhantomData,
                    row,
                    ast: &ast,
                    conn,
                    updated: &updated,
                };

                let res = m(row, db.clone());
                let res = res.into_new(db.clone());

                let columns = FrozenVec::new();
                let field = ast.add_select(db.id().build_expr());
                columns.push(Box::new((field, B::NAME)));
                res.read(Reader {
                    _phantom: PhantomData,
                    ast: &ast,
                    out: &columns,
                });

                let table = MyAlias::new();
                let new_select = ast.simple(offset, 1);

                let mut insert = InsertStatement::new();
                let names = columns.iter().map(|(_field, name)| Alias::new(*name));
                let vals = columns.iter().map(|(field, _name)| FieldAlias {
                    table,
                    col: **field,
                });
                insert.columns(names);
                insert
                    .select_from(
                        SelectStatement::new()
                            .from_subquery(new_select, table)
                            .columns(vals)
                            .take(),
                    )
                    .unwrap();

                let sql = insert.to_string(SqliteQueryBuilder);

                conn.execute(&sql, []).unwrap();
            }

            offset += 1;
            if updated.get() {
                select = ast.simple(offset, limit);
                let sql = select.to_string(SqliteQueryBuilder);

                drop(rows);
                statement = conn.prepare(&sql).unwrap();
                rows = statement.query([]).unwrap();
            }
        }
        // out
    }

    pub fn drop_table(&mut self, name: &'static str) {
        let name = sea_query::Alias::new(name);
        let step = sea_query::Table::drop().table(name).take();
        self.drop.push(step);
    }

    pub fn new_table(&mut self, name: &'static str) {
        // let name = sea_query::Alias::new(name);
        // sea_query::Table::create()
        //     .table(name)
        //     .col(column)
        //     .primary_key(sea_query::Index::create().col("id"));

        todo!()
    }
}

pub trait Migration<From> {
    type S;

    fn tables(b: SchemaBuilder);
}

pub struct Migrator<'x, S> {
    schema: Option<S>,
    conn: &'x Connection,
}

impl<'a, S> Migrator<'a, S> {
    pub fn migrate<M: Migration<S>>(self, _f: impl FnOnce(&Self) -> M) -> Migrator<'a, M::S> {
        todo!()
    }

    pub fn check(self) -> S {
        todo!()
    }
}
