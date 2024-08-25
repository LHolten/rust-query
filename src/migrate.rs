use std::{
    marker::PhantomData,
    ops::Deref,
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};

use ouroboros::self_referencing;
use rusqlite::{config::DbConfig, Connection, Transaction};
use sea_query::{
    Alias, ColumnDef, InsertStatement, IntoTableRef, SqliteQueryBuilder, TableDropStatement,
    TableRenameStatement,
};
use sea_query_rusqlite::RusqliteBinder;

use crate::{
    alias::{Scope, TmpTable},
    ast::MySelect,
    client::{private_exec, Client, QueryBuilder},
    exec::Execute,
    from_row::AdHoc,
    hash,
    insert::Reader,
    pragma::read_schema,
    private::FromRow,
    transaction::{DbClient, LatestToken, SnapshotToken, ThreadToken},
    Db, Free, HasId, Table,
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

pub trait TableMigration<A: HasId> {
    type T;

    // there is no reason to specify the lifetime of prev
    // because it is only used for reader, which doesn't care.
    fn into_new(self, prev: Free<'_, A>, reader: Reader<'_, A::Schema>);
}

pub struct SchemaBuilder<'x> {
    // this is used to create temporary table names
    scope: Scope,
    conn: &'x rusqlite::Transaction<'x>,
    drop: Vec<TableDropStatement>,
    rename: Vec<TableRenameStatement>,
}

impl<'x> SchemaBuilder<'x> {
    pub fn migrate_table<M, O, A: HasId, B: HasId>(&mut self, mut m: M)
    where
        M: FnMut(Free<'x, A>) -> O,
        O: TableMigration<A, T = B>,
    {
        self.create_from(move |db: Free<A>| Some(m(db)));

        self.drop
            .push(sea_query::Table::drop().table(Alias::new(A::NAME)).take());
    }

    pub fn create_from<F, O, A: HasId, B: HasId>(&mut self, mut f: F)
    where
        F: FnMut(Free<'x, A>) -> Option<O>,
        O: TableMigration<A, T = B>,
    {
        let new_table_name = self.scope.tmp_table();
        new_table::<B>(self.conn, new_table_name);

        self.rename.push(
            sea_query::Table::rename()
                .table(new_table_name, Alias::new(B::NAME))
                .take(),
        );

        self.conn.new_query(|e| {
            // TODO: potentially replace this with Execute::join
            let table = e.ast.scope.new_alias();
            e.ast.tables.push((A::NAME.to_owned(), table));
            let db_id = Db::<A>::new(table);

            e.into_vec(AdHoc::new(|mut row| {
                let just_db_cache = row.cache(db_id);
                move |row| {
                    let just_db = row.get(just_db_cache);
                    if let Some(res) = f(just_db) {
                        let ast = MySelect::default();

                        let reader = Reader {
                            ast: &ast,
                            _p: PhantomData,
                        };
                        res.into_new(just_db, reader);

                        let new_select = ast.simple();

                        let mut insert = InsertStatement::new();
                        let names = ast.select.iter().map(|(_field, name)| *name);
                        insert.into_table(new_table_name);
                        insert.columns(names);
                        insert.select_from(new_select).unwrap();

                        let (sql, values) = insert.build_rusqlite(SqliteQueryBuilder);

                        self.conn.execute(&sql, &*values.as_params()).unwrap();
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
        let new_table_name = self.scope.tmp_table();
        new_table::<T>(self.conn, new_table_name);

        self.rename.push(
            sea_query::Table::rename()
                .table(new_table_name, Alias::new(T::NAME))
                .take(),
        );
    }
}

fn new_table<T: Table>(conn: &Connection, alias: TmpTable) {
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
    /// TODO: return an error if the file is already opened
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
    pub fn create_db_sql<S: Schema>(self, sql: &[&str]) -> Migrator<S> {
        self.migrator::<S>(|conn| {
            for sql in sql {
                conn.execute_batch(sql).unwrap();
            }
        })
    }

    /// Create empty tables based on the schema if the database was just created.
    pub fn create_db_empty<S: Schema>(self) -> Migrator<S> {
        self.migrator::<S>(|conn| {
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
            set_user_version(conn, S::VERSION).unwrap();
        }

        Migrator {
            client: Client::new(self.manager),
            transaction: owned,
            _p: PhantomData,
        }
    }
}

/// This type is used to apply database migrations.
pub struct Migrator<S> {
    client: Client,
    transaction: OwnedTransaction,
    _p: PhantomData<S>,
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

    /// Apply a database migration if `s` is [Some] (because that means the migration can be applied).
    /// If the migration was applied or if the database already had the new schema it is returned.
    pub fn migrate<'a, F, M, N: Schema>(self, t: &'a mut ThreadToken, f: F) -> Migrator<N>
    where
        F: FnOnce(ReadClient<'_, 'a, S>) -> M,
        M: Migration<'a, S, S = N>,
    {
        let conn = self.transaction.borrow_transaction();
        if let Some(s) = new_checked::<S>(conn) {
            let client = ReadClient(conn, s, t);
            let res = f(client);
            let mut builder = SchemaBuilder {
                scope: Default::default(),
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
            client: self.client,
            transaction: self.transaction,
            _p: PhantomData,
        }
    }

    /// Commit the migration transaction and return a [Client].
    pub fn finish(mut self) -> Option<DbClient<S>> {
        self.transaction
            .with_transaction_mut(|x| x.set_drop_behavior(rusqlite::DropBehavior::Commit));

        // TODO: clean this up
        let Some(schema) = new_checked(self.transaction.borrow_transaction()) else {
            return None;
        };
        let Some(schema2) = new_checked(self.transaction.borrow_transaction()) else {
            return None;
        };

        let heads = self.transaction.into_heads();
        heads
            .conn
            .pragma_update(None, "foreign_keys", "ON")
            .unwrap();

        let client = Arc::new(self.client);
        Some(DbClient {
            latest: LatestToken(SnapshotToken {
                client: client.clone(),
                schema,
            }),
            snapshot: SnapshotToken {
                client,
                schema: schema2,
            },
        })
    }
}

pub struct ReadClient<'x, 'a, S>(&'x rusqlite::Transaction<'x>, S, &'a ThreadToken);

impl<S> Deref for ReadClient<'_, '_, S> {
    type Target = S;

    fn deref(&self) -> &Self::Target {
        &self.1
    }
}

impl<S> ReadClient<'_, '_, S> {
    /// Same as [Client::exec].
    pub fn exec<'s, F, R>(&'s self, f: F) -> R
    where
        F: for<'a> FnOnce(&'a mut Execute<'s, 'a, S>) -> R,
    {
        private_exec(&self.0, f)
    }

    /// Same as [Client::get].
    pub fn get<'s, T>(&'s self, val: impl for<'a> FromRow<'a, 's, S, Out = T>) -> T {
        self.exec(|e| e.into_vec(val)).pop().unwrap()
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
