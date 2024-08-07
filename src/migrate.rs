use std::{any::Any, path::Path, sync::atomic::AtomicBool};

use ouroboros::self_referencing;
use ref_cast::RefCast;
use rusqlite::{config::DbConfig, Connection, Transaction};
use sea_query::{
    Alias, ColumnDef, InsertStatement, IntoTableRef, SqliteQueryBuilder, TableDropStatement,
    TableRenameStatement,
};

use crate::{
    alias::{Field, MyAlias},
    ast::{add_table, MySelect},
    client::{private_exec, Client, QueryBuilder},
    db::DbCol,
    exec::Execute,
    from_row::AdHoc,
    hash,
    insert::Reader,
    pragma::read_schema,
    value::MyTyp,
    HasId, Just, Table, Value,
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

pub trait Schema: Sized + 'static {
    const VERSION: i64;
    fn new() -> Self;
    fn typs(b: &mut TableTypBuilder);
}

pub trait TableMigration<'a, A: HasId> {
    type T;

    fn into_new(self, prev: Just<'a, A>, reader: Reader<'_>);
}

pub struct SchemaBuilder<'x> {
    conn: &'x rusqlite::Transaction<'x>,
    drop: Vec<TableDropStatement>,
    rename: Vec<TableRenameStatement>,
}

impl<'x> SchemaBuilder<'x> {
    pub fn migrate_table<M, O, A: HasId, B: HasId>(&mut self, mut m: M)
    where
        M: FnMut(Just<'x, A>) -> O,
        O: TableMigration<'x, A, T = B>,
    {
        self.create_from(move |db: Just<A>| Some(m(db)));

        self.drop
            .push(sea_query::Table::drop().table(Alias::new(A::NAME)).take());
    }

    pub fn create_from<F, O, A: HasId, B: HasId>(&mut self, mut f: F)
    where
        F: FnMut(Just<'x, A>) -> Option<O>,
        O: TableMigration<'x, A, T = B>,
    {
        let new_table_name = MyAlias::new();
        new_table::<B>(self.conn, new_table_name);

        self.rename.push(
            sea_query::Table::rename()
                .table(new_table_name, Alias::new(B::NAME))
                .take(),
        );

        self.conn.new_query(|e| {
            let table = add_table(&mut e.ast.tables, A::NAME.to_owned());
            let db_id = DbCol::<A>::db(table, Field::Str(A::ID));

            e.into_vec(AdHoc::new(|mut row| {
                let just_db_cache = row.cache(db_id);
                move |row| {
                    let just_db = row.get(just_db_cache);
                    if let Some(res) = f(just_db) {
                        let ast = MySelect::default();

                        let reader = Reader { ast: &ast };
                        res.into_new(just_db, reader);

                        let new_select = ast.simple();

                        let mut insert = InsertStatement::new();
                        let names = ast.select.iter().map(|(_field, name)| *name);
                        insert.into_table(new_table_name);
                        insert.columns(names);
                        insert.select_from(new_select).unwrap();

                        let sql = insert.to_string(SqliteQueryBuilder);
                        self.conn.execute(&sql, []).unwrap();
                    }
                }
            }))
        });
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

pub trait Migration<'t, From> {
    type S: Schema;

    fn tables(self, b: &mut SchemaBuilder<'t>);
}

/// [Prepare] can be used to open a database in a file or in memory.
pub struct Prepare {
    manager: r2d2_sqlite::SqliteConnectionManager,
    conn: Connection,
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
    /// Open a database that is stored in a file.
    /// Creates the database if it does not exist.
    pub fn open(p: impl AsRef<Path>) -> Self {
        let manager = r2d2_sqlite::SqliteConnectionManager::file(p);
        Self::open_internal(manager)
    }

    /// Creates a new empty database in memory.
    pub fn open_in_memory() -> Self {
        let manager = r2d2_sqlite::SqliteConnectionManager::memory();
        Self::open_internal(manager)
    }

    fn open_internal(manager: r2d2_sqlite::SqliteConnectionManager) -> Self {
        assert!(ALLOWED.swap(false, std::sync::atomic::Ordering::Relaxed));
        let manager = manager.with_init(|inner| {
            inner.pragma_update(None, "journal_mode", "WAL")?;
            inner.pragma_update(None, "synchronous", "NORMAL")?;
            inner.pragma_update(None, "foreign_keys", "ON")?;
            inner.set_db_config(DbConfig::SQLITE_DBCONFIG_DQS_DDL, false)?;
            inner.set_db_config(DbConfig::SQLITE_DBCONFIG_DQS_DML, false)?;
            Ok(())
        });
        use r2d2::ManageConnection;
        let conn = manager.connect().unwrap();

        Self { conn, manager }
    }

    /// Execute a raw sql statement if the database was just created.
    pub fn create_db_sql<S: Schema>(self, sql: &[&str]) -> (Migrator, Option<S>) {
        self.migrator(|conn| {
            for sql in sql {
                conn.execute_batch(sql).unwrap();
            }
        })
    }

    /// Create empty tables based on the schema if the database was just created.
    pub fn create_db_empty<S: Schema>(self) -> (Migrator, Option<S>) {
        self.migrator(|conn| {
            let mut b = TableTypBuilder::default();
            S::typs(&mut b);

            for (table_name, table) in &*b.ast.tables {
                new_table_inner(conn, table, Alias::new(table_name));
            }
        })
    }

    fn migrator<S: Schema>(self, f: impl FnOnce(&Transaction)) -> (Migrator, Option<S>) {
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
            set_user_version(conn, S::VERSION).unwrap();
        }

        let schema = new_checked::<S>(conn);
        let migrator = Migrator {
            client: Client::new(self.manager),
            transaction: owned,
            schema: Box::new(()),
        };
        (migrator, schema)
    }
}

/// This type is used to apply database migrations.
pub struct Migrator {
    client: Client,
    transaction: OwnedTransaction,
    schema: Box<dyn Any>,
}

impl Migrator {
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

    /// Apply a database migration if `s` is [Some] (because that means the migration can be applied).
    /// If the migration was applied or if the database already had the new schema it is returned.
    pub fn migrate<'a, S: Schema, F, M, N: Schema>(&'a mut self, s: Option<S>, f: F) -> Option<N>
    where
        F: FnOnce(&'a S, &'a ReadClient<'a>) -> M,
        M: Migration<'a, S, S = N>,
    {
        let conn = self.transaction.borrow_transaction();
        if let Some(s) = s {
            self.schema = Box::new(s);
            let s = self.schema.as_ref().downcast_ref().unwrap();
            let res = f(s, ReadClient::ref_cast(conn));
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
        new_checked::<N>(conn)
    }

    /// Commit the migration transaction and return a [Client].
    pub fn finish(mut self) -> Client {
        self.transaction
            .with_transaction_mut(|x| x.set_drop_behavior(rusqlite::DropBehavior::Commit));

        let heads = self.transaction.into_heads();
        heads
            .conn
            .pragma_update(None, "foreign_keys", "ON")
            .unwrap();

        self.client
    }
}

#[derive(RefCast)]
#[repr(transparent)]
pub struct ReadClient<'a>(rusqlite::Transaction<'a>);

impl ReadClient<'_> {
    /// Same as [Client::exec].
    pub fn exec<'s, F, R>(&'s self, f: F) -> R
    where
        F: for<'a> FnOnce(&'a mut Execute<'s, 'a>) -> R,
    {
        private_exec(&self.0, f)
    }

    /// Same as [Client::get].
    pub fn get<'s, T: MyTyp>(&'s self, val: impl for<'a> Value<'a, Typ = T>) -> T::Out<'s> {
        self.exec(|e| e.into_vec(val.clone())).pop().unwrap()
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

fn new_checked<T: Schema>(conn: &rusqlite::Transaction) -> Option<T> {
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
