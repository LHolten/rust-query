use std::{cell::Cell, marker::PhantomData, mem::take};

use elsa::FrozenVec;
use rusqlite::Connection;
use sea_query::{
    Alias, Expr, InsertStatement, SqliteQueryBuilder, TableDropStatement, TableRenameStatement,
};

use crate::{
    ast::{add_table, MySelect},
    insert::{Reader, Writable},
    mymap::MyMap,
    value::{Db, Field, FkInfo, MyAlias, Value},
    HasId, Row,
};

pub trait TableMigration<'a, A: HasId> {
    type T;

    fn into_new(self: Box<Self>, prev: Db<'a, A>) -> Box<dyn Writable<'a, T = Self::T>>;
}

pub struct SchemaBuilder<'x> {
    conn: &'x Connection,
    drop: Vec<TableDropStatement>,
    rename: Vec<TableRenameStatement>,
}

impl<'x> SchemaBuilder<'x> {
    pub fn migrate_table<M, A: HasId, B: HasId>(&mut self, mut m: M)
    where
        M: for<'y, 'a> FnMut(Row<'y, 'a>, Db<'a, A>) -> Box<dyn TableMigration<'a, A, T = B>>,
    {
        let mut ast = MySelect {
            sources: FrozenVec::new(),
            filters: FrozenVec::new(),
            select: MyMap::default(),
            filter_on: FrozenVec::new(),
            group: Cell::new(false),
        };

        let joins = add_table(&ast.sources, A::NAME.to_owned());
        let db = FkInfo::<A>::joined(joins, Field::Str(A::ID));

        let mut select = ast.simple(0, u32::MAX);
        let sql = select.to_string(SqliteQueryBuilder);

        let mut statement = self.conn.prepare(&sql).unwrap();
        let mut rows = statement.query([]).unwrap();

        let mut offset = 0;
        while let Some(row) = rows.next().unwrap() {
            let updated = Cell::new(false);

            let row = Row {
                offset,
                limit: u32::MAX,
                inner: PhantomData,
                row,
                ast: &ast,
                conn: self.conn,
                updated: &updated,
            };

            let res = m(row, db.clone());
            let id = row.get(db.id());
            let res = res.into_new(db.clone());

            let old_select = take(&mut ast.select);
            {
                ast.select
                    .get_or_init(Expr::val(id).into(), || Field::Str(B::ID));
                res.read(Reader {
                    _phantom: PhantomData,
                    ast: &ast,
                });

                let mut new_select = ast.simple(0, u32::MAX);
                new_select.and_where(db.id().build_expr().eq(id));

                let new_table = MyAlias::new();
                let mut insert = InsertStatement::new();
                let names = ast.select.iter().map(|(_field, name)| *name);
                insert.into_table(new_table);
                insert.columns(names);
                insert.select_from(new_select).unwrap();

                let sql = insert.to_string(SqliteQueryBuilder);
                self.conn.execute(&sql, []).unwrap();

                self.drop
                    .push(sea_query::Table::drop().table(Alias::new(A::NAME)).take());
                self.rename.push(
                    sea_query::Table::rename()
                        .table(new_table, Alias::new(B::NAME))
                        .take(),
                );
            }
            ast.select = old_select;

            offset += 1;
            if updated.get() {
                select = ast.simple(offset, u32::MAX);
                let sql = select.to_string(SqliteQueryBuilder);

                drop(rows);
                statement = self.conn.prepare(&sql).unwrap();
                rows = statement.query([]).unwrap();
            }
        }
    }

    pub fn drop_table(&mut self, name: &'static str) {
        let name = Alias::new(name);
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

    fn tables(self, b: &mut SchemaBuilder) -> Self::S;
}

pub struct Migrator<'x, S> {
    schema: Option<S>,
    conn: &'x Connection,
}

impl<'a, S> Migrator<'a, S> {
    pub fn migrate<M: Migration<S>>(self, f: impl FnOnce(&S) -> M) -> Migrator<'a, M::S> {
        if let Some(s) = self.schema {
            let res = f(&s);
            let mut builder = SchemaBuilder {
                conn: self.conn,
                drop: vec![],
                rename: vec![],
            };
            let res = res.tables(&mut builder);
            for drop in builder.drop {
                let sql = drop.to_string(SqliteQueryBuilder);
                self.conn.execute(&sql, []).unwrap();
            }
            for rename in builder.rename {
                let sql = rename.to_string(SqliteQueryBuilder);
                self.conn.execute(&sql, []).unwrap();
            }

            return Migrator {
                schema: Some(res),
                conn: self.conn,
            };
        }
        todo!()
    }

    pub fn check(self) -> S {
        self.schema.unwrap()
    }
}