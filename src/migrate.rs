use std::{cell::Cell, marker::PhantomData, mem::take, path::Path, sync::atomic::AtomicBool};

use elsa::FrozenVec;

use ouroboros::self_referencing;
use rusqlite::{config::DbConfig, Connection, Transaction};
use sea_query::{
    Alias, ColumnDef, Expr, InsertStatement, SqliteQueryBuilder, TableDropStatement,
    TableRenameStatement,
};

use crate::{
    ast::{add_table, MySelect},
    client::Client,
    exec::Row,
    hash::{self, hash_schema},
    insert::Reader,
    mymap::MyMap,
    pragma::read_schema,
    value::{Db, Field, FkInfo, MyAlias, Value},
    HasId, Table,
};

#[derive(Default)]
pub struct TableTypBuilder {
    pub(crate) ast: hash::Schema,
}

impl TableTypBuilder {
    pub fn table<T: HasId>(&mut self) {
        let mut b = crate::TypBuilder::default();
        T::typs(&mut b);
        self.ast.tables.insert((T::NAME.to_owned(), b.ast));
    }
}

pub trait Schema: Sized {
    const VERSION: i64;
    fn new() -> Self;
    fn typs(b: &mut TableTypBuilder);
}

pub trait TableMigration<'a, A: HasId> {
    type T;

    fn into_new(self: Box<Self>, prev: Db<'a, A>, reader: Reader<'_, 'a>);
}

pub struct SchemaBuilder<'x> {
    conn: &'x Connection,
    drop: Vec<TableDropStatement>,
    rename: Vec<TableRenameStatement>,
}

impl<'x> SchemaBuilder<'x> {
    pub fn migrate_table<M, A: HasId, B: HasId>(&mut self, mut m: M)
    where
        M: for<'y, 'a> FnMut(Row<'y, 'a>, Db<'a, A>) -> Box<dyn TableMigration<'a, A, T = B> + 'a>,
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

        let new_table_name = MyAlias::new();
        new_table::<B>(self.conn, new_table_name);

        self.drop
            .push(sea_query::Table::drop().table(Alias::new(A::NAME)).take());
        self.rename.push(
            sea_query::Table::rename()
                .table(new_table_name, Alias::new(B::NAME))
                .take(),
        );

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

            let old_select = take(&mut ast.select);
            {
                ast.select
                    .get_or_init(Expr::val(id).into(), || Field::Str(B::ID));
                let reader = Reader {
                    _phantom: PhantomData,
                    ast: &ast,
                };
                res.into_new(db.clone(), reader);

                let mut new_select = ast.simple(0, u32::MAX);
                new_select.and_where(db.id().build_expr().eq(id));

                let mut insert = InsertStatement::new();
                let names = ast.select.iter().map(|(_field, name)| *name);
                insert.into_table(new_table_name);
                insert.columns(names);
                insert.select_from(new_select).unwrap();

                let sql = insert.to_string(SqliteQueryBuilder);
                self.conn.execute(&sql, []).unwrap();
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

    pub fn drop_table<T: HasId>(&mut self) {
        let name = Alias::new(T::NAME);
        let step = sea_query::Table::drop().table(name).take();
        self.drop.push(step);
    }

    pub fn new_table<T: HasId>(&mut self) {
        let new_table_name = MyAlias::new();
        new_table::<T>(self.conn, new_table_name);

        self.rename.push(
            sea_query::Table::rename()
                .table(new_table_name, Alias::new(T::NAME))
                .take(),
        );
    }
}

fn new_table<T: Table>(conn: &Connection, alias: MyAlias) {
    let mut f = crate::TypBuilder::default();
    T::typs(&mut f);
    let mut ast = f.ast.create();
    ast.table(alias)
        .col(ColumnDef::new(Alias::new("id")).integer().primary_key());
    let mut sql = ast.to_string(SqliteQueryBuilder);
    sql.push_str(" STRICT");
    conn.execute(&sql, []).unwrap();
}

pub trait Migration<From> {
    type S: Schema;

    fn tables(self, b: &mut SchemaBuilder);
}

pub struct Prepare {
    pub(crate) conn: Connection,
}

static ALLOWED: AtomicBool = AtomicBool::new(true);

#[self_referencing]
pub(crate) struct OwnedTransaction {
    pub(crate) conn: Connection,
    #[borrows(mut conn)]
    #[covariant]
    pub(crate) transaction: Transaction<'this>,
}

impl Prepare {
    pub fn open(p: impl AsRef<Path>) -> Self {
        let inner = rusqlite::Connection::open(p).unwrap();
        Self::open_internal(inner)
    }

    pub fn open_in_memory() -> Self {
        let inner = rusqlite::Connection::open_in_memory().unwrap();
        Self::open_internal(inner)
    }

    fn open_internal(inner: rusqlite::Connection) -> Self {
        assert!(ALLOWED.swap(false, std::sync::atomic::Ordering::Relaxed));
        inner.pragma_update(None, "journal_mode", "WAL").unwrap();
        inner.pragma_update(None, "synchronous", "NORMAL").unwrap();
        inner.pragma_update(None, "foreign_keys", "ON").unwrap();
        inner
            .set_db_config(DbConfig::SQLITE_DBCONFIG_DQS_DDL, false)
            .unwrap();
        inner
            .set_db_config(DbConfig::SQLITE_DBCONFIG_DQS_DML, false)
            .unwrap();

        Self { conn: inner }
    }

    /// Execute a raw sql statement.
    pub fn execute_batch(&self, sql: &str) {
        self.conn.execute_batch(sql).unwrap();
    }

    pub fn migrator<S: Schema>(self) -> Migrator<S> {
        let schema = new_checked::<S>(&self.conn);

        self.conn
            .pragma_update(None, "foreign_keys", "OFF")
            .unwrap();

        let owned = OwnedTransaction::new(self.conn, |x| {
            x.transaction_with_behavior(rusqlite::TransactionBehavior::Exclusive)
                .unwrap()
        });

        Migrator {
            schema,
            transaction: owned,
        }
    }
}

pub struct Migrator<S> {
    pub(crate) schema: Option<S>,
    pub(crate) transaction: OwnedTransaction,
}

impl<S: Schema> Migrator<S> {
    /// Execute a new query.
    // pub fn new_query<'s, F, R>(&'s self, f: F) -> Option<R>
    // where
    //     F: for<'a> FnOnce(&'s S, &'a mut Exec<'s, 'a>) -> R,
    // {
    //     let schema = self.schema.as_ref()?;
    //     Some(
    //         self.transaction
    //             .borrow_transaction()
    //             .new_query(|q| f(schema, q)),
    //     )
    // }

    pub fn migrate<M: Migration<S>>(self, f: impl FnOnce(&S) -> M) -> Migrator<M::S> {
        let conn = self.transaction.borrow_transaction();
        if let Some(s) = self.schema {
            let res = f(&s);
            let mut builder = SchemaBuilder {
                conn,
                drop: vec![],
                rename: vec![],
            };
            res.tables(&mut builder);
            for drop in builder.drop {
                let sql = drop.to_string(SqliteQueryBuilder);
                conn.execute(&sql, []).unwrap();
            }
            for rename in builder.rename {
                let sql = rename.to_string(SqliteQueryBuilder);
                conn.execute(&sql, []).unwrap();
            }
            conn.execute_batch("PRAGMA foreign_key_check").unwrap();
            set_user_version(conn, M::S::VERSION).unwrap();

            Migrator {
                schema: Some(<M as Migration<S>>::S::new()),
                transaction: self.transaction,
            }
        } else {
            Migrator {
                schema: new_checked::<<M as Migration<S>>::S>(conn),
                transaction: self.transaction,
            }
        }
    }

    pub fn finish(mut self) -> (Client, S) {
        self.transaction
            .with_transaction_mut(|x| x.set_drop_behavior(rusqlite::DropBehavior::Commit));

        let heads = self.transaction.into_heads();
        heads
            .conn
            .pragma_update(None, "foreign_keys", "ON")
            .unwrap();

        (Client { inner: heads.conn }, self.schema.unwrap())
    }
}

// Read user version field from the SQLite db
fn user_version(conn: &Connection) -> Result<i64, rusqlite::Error> {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
}

// Set user version field from the SQLite db
fn set_user_version(conn: &Connection, v: i64) -> Result<(), rusqlite::Error> {
    conn.pragma_update(None, "user_version", v)
}

fn new_checked<T: Schema>(conn: &Connection) -> Option<T> {
    if user_version(conn).unwrap() != T::VERSION {
        return None;
    }

    let mut b = TableTypBuilder::default();
    T::typs(&mut b);
    assert_eq!(
        b.ast,
        read_schema(conn),
        "user version is equal, but schema is different"
    );

    Some(T::new())
}
