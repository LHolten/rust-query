use std::{cell::Cell, marker::PhantomData, mem::take, path::Path, sync::atomic::AtomicBool};

use elsa::FrozenVec;

use ouroboros::self_referencing;
use rusqlite::{config::DbConfig, Connection, Transaction};
use sea_query::{
    Alias, ColumnDef, Expr, InsertStatement, IntoTableRef, SqliteQueryBuilder, TableDropStatement,
    TableRenameStatement,
};

use crate::{
    alias::{Field, MyAlias},
    ast::{add_table, MySelect},
    client::Client,
    db::DbCol,
    exec::Row,
    hash,
    insert::Reader,
    mymap::MyMap,
    pragma::read_schema,
    value::Value,
    HasId, Just, Table,
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

pub trait TableMigration<A: HasId> {
    type T;

    fn into_new<'a>(self: Box<Self>, prev: DbCol<'a, A>, reader: Reader<'_, 'a>);
}

pub struct SchemaBuilder<'x> {
    conn: &'x Connection,
    drop: Vec<TableDropStatement>,
    rename: Vec<TableRenameStatement>,
}

impl<'x> SchemaBuilder<'x> {
    pub fn migrate_table<M, A: HasId, B: HasId>(&mut self, mut m: M)
    where
        M: FnMut(Just<A>) -> Box<dyn TableMigration<A, T = B>>,
    {
        self.create_from(move |db: Just<A>| Some(m(db)));

        self.drop
            .push(sea_query::Table::drop().table(Alias::new(A::NAME)).take());
    }

    pub fn create_from<F, A: HasId, B: HasId>(&mut self, mut f: F)
    where
        F: FnMut(Just<A>) -> Option<Box<dyn TableMigration<A, T = B>>>,
    {
        let mut ast = MySelect {
            tables: Vec::new(),
            extra: MyMap::default(),
            filters: FrozenVec::new(),
            select: MyMap::default(),
            filter_on: FrozenVec::new(),
        };

        let table = add_table(&mut ast.tables, A::NAME.to_owned());
        let db = DbCol::<A>::db(table, Field::Str(A::ID));

        let mut select = ast.simple(0, u32::MAX);
        let sql = select.to_string(SqliteQueryBuilder);

        let new_table_name = MyAlias::new();
        new_table::<B>(self.conn, new_table_name);

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
            let just_db = row.get(db);

            if let Some(res) = f(just_db) {
                let old_select = take(&mut ast.select);
                {
                    ast.select
                        .get_or_init(Expr::val(just_db).into(), || Field::Str(B::ID));
                    let reader = Reader {
                        _phantom: PhantomData,
                        ast: &ast,
                    };
                    res.into_new(db, reader);

                    let db_id = db.build_expr(ast.builder());
                    let mut new_select = ast.simple(0, u32::MAX);
                    new_select.and_where(db_id.eq(just_db));

                    let mut insert = InsertStatement::new();
                    let names = ast.select.iter().map(|(_field, name)| *name);
                    insert.into_table(new_table_name);
                    insert.columns(names);
                    insert.select_from(new_select).unwrap();

                    let sql = insert.to_string(SqliteQueryBuilder);
                    self.conn.execute(&sql, []).unwrap();
                }
                ast.select = old_select;
            }

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
    new_table_inner(conn, &f.ast, alias);
}

fn new_table_inner(conn: &Connection, table: &crate::hash::Table, alias: impl IntoTableRef) {
    let mut create = table.create();
    create
        .table(alias)
        .col(ColumnDef::new(Alias::new("id")).integer().primary_key());
    let mut sql = create.to_string(SqliteQueryBuilder);
    sql.push_str(" STRICT");
    conn.execute(&sql, []).unwrap();
}

pub trait Migration<From> {
    type S: Schema;

    fn tables(self: Box<Self>, b: &mut SchemaBuilder);
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
    pub fn create_db_sql<S: Schema>(self, sql: &[&str]) -> Migrator<S> {
        self.migrator(|conn| {
            for sql in sql {
                conn.execute_batch(sql).unwrap();
            }
        })
    }

    pub fn create_db_empty<S: Schema>(self) -> Migrator<S> {
        self.migrator(|conn| {
            let mut b = TableTypBuilder::default();
            S::typs(&mut b);

            for (table_name, table) in &*b.ast.tables {
                new_table_inner(conn, table, Alias::new(table_name));
            }
        })
    }

    fn migrator<S: Schema>(self, f: impl FnOnce(&Transaction)) -> Migrator<S> {
        self.conn
            .pragma_update(None, "foreign_keys", "OFF")
            .unwrap();

        let owned = OwnedTransaction::new(self.conn, |x| {
            x.transaction_with_behavior(rusqlite::TransactionBehavior::Exclusive)
                .unwrap()
        });

        let conn = owned.borrow_transaction();
        let schema_version: i64 = conn
            .pragma_query_value(None, "schema_version", |r| r.get(0))
            .unwrap();
        // check if this database is newly created
        if schema_version == 0 {
            f(conn);
            foreign_key_check(conn);
        }

        Migrator {
            schema: new_checked::<S>(conn),
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

    pub fn migrate<'a, F, N: Schema>(self, f: F) -> Migrator<N>
    where
        F: for<'x> FnOnce(&'x S, &'x [&'a ()], &'x Client) -> Box<dyn Migration<S, S = N> + 'x>,
    {
        let conn = self.transaction.borrow_transaction();
        if let Some(s) = self.schema {
            let res = f(&s, &[], todo!());
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
            foreign_key_check(conn);
            set_user_version(conn, N::VERSION).unwrap();
        }
        Migrator {
            schema: new_checked::<N>(conn),
            transaction: self.transaction,
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
    pretty_assertions::assert_eq!(
        b.ast,
        read_schema(conn),
        "user version is equal ({}), but schema is different (expected left, but got right)",
        T::VERSION
    );

    Some(T::new())
}

fn foreign_key_check(conn: &Connection) {
    let errors = conn
        .prepare("PRAGMA foreign_key_check")
        .unwrap()
        .query_map([], |_| Ok(()))
        .unwrap()
        .count();
    if errors != 0 {
        panic!("migration violated foreign key constraint")
    }
}
