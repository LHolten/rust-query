use std::{marker::PhantomData, path::Path, sync::atomic::AtomicBool};

use rusqlite::{config::DbConfig, Connection};
use sea_query::{
    Alias, ColumnDef, InsertStatement, IntoTableRef, SqliteQueryBuilder, TableDropStatement,
    TableRenameStatement,
};
use sea_query_rusqlite::RusqliteBinder;

use crate::{
    alias::{Scope, TmpTable},
    ast::MySelect,
    client::LocalClient,
    dummy_impl::{Cacher, DummyImpl, Prepared, Row},
    hash,
    schema_pragma::read_schema,
    transaction::Database,
    value::{self, DynTypedExpr, Private},
    writable::Reader,
    Column, IntoColumn, IntoDummy, Rows, Table,
};

pub type M<'a, From, To> = Box<
    dyn 'a
        + for<'t> FnOnce(
            ::rust_query::Column<'t, <From as Table>::Schema, From>,
        ) -> Alter<'t, 'a, From, To>,
>;

/// This is the type used to return table alterations in migrations.
///
/// Note that migrations allow you to use anything that implements [crate::IntoDummy] to specify the new values.
/// In particular this allows mapping values using native rust with [crate::IntoDummy::map_dummy].
///
/// Take a look at the documentation of [crate::migration::schema] for more general information.
///
/// The purpose of wrapping migration results in [Alter] (and [Create]) is to dyn box the type so that type inference works.
/// (Type inference is problematic with higher ranked generic returns from closures).
/// Futhermore [Alter] (and [Create]) also have an implied bound of `'a: 't` which makes it easier to implement migrations.
pub struct Alter<'t, 'a, From, To> {
    inner: Box<dyn 't + TableMigration<'t, 'a, From = From, To = To>>,
    _p: PhantomData<&'t &'a ()>,
}

impl<'t, 'a, From, To> Alter<'t, 'a, From, To> {
    pub fn new(val: impl 't + TableMigration<'t, 'a, From = From, To = To>) -> Self {
        Self {
            inner: Box::new(val),
            _p: PhantomData,
        }
    }
}

pub type C<'a, FromSchema, To> =
    Box<dyn 'a + for<'t> FnOnce(&mut Rows<'t, FromSchema>) -> Create<'t, 'a, FromSchema, To>>;

/// This is the type used to return table creations in migrations.
///
/// For more information take a look at [Alter].
pub struct Create<'t, 'a, FromSchema, To> {
    inner: Box<dyn 't + TableCreation<'t, 'a, FromSchema = FromSchema, To = To>>,
    _p: PhantomData<&'t &'a ()>,
}

impl<'t, 'a, FromSchema: 't, To: 't> Create<'t, 'a, FromSchema, To> {
    pub fn new(val: impl 't + TableCreation<'t, 'a, FromSchema = FromSchema, To = To>) -> Self {
        Self {
            inner: Box::new(val),
            _p: PhantomData,
        }
    }

    /// Use this if you want the new table to be empty.
    pub fn empty(rows: &mut Rows<'t, FromSchema>) -> Self {
        rows.filter(false);
        Create::new(NeverCreate(PhantomData, PhantomData))
    }
}

struct NeverCreate<FromSchema, To>(PhantomData<FromSchema>, PhantomData<To>);

impl<'t, 'a, FromSchema, To> TableCreation<'t, 'a> for NeverCreate<FromSchema, To> {
    type FromSchema = FromSchema;
    type To = To;

    fn prepare(self: Box<Self>, _: &mut CacheAndRead<'t, 'a, Self::FromSchema>) {}
}

pub struct TableTypBuilder<S> {
    pub(crate) ast: hash::Schema,
    _p: PhantomData<S>,
}

impl<S> Default for TableTypBuilder<S> {
    fn default() -> Self {
        Self {
            ast: Default::default(),
            _p: Default::default(),
        }
    }
}

impl<S> TableTypBuilder<S> {
    pub fn table<T: Table<Schema = S>>(&mut self) {
        let mut b = hash::TypBuilder::default();
        T::typs(&mut b);
        self.ast.tables.insert((T::NAME.to_owned(), b.ast));
    }
}

pub trait Schema: Sized + 'static {
    const VERSION: i64;
    fn typs(b: &mut TableTypBuilder<Self>);
}

pub struct CacheAndRead<'t, 'a, S> {
    cacher: Cacher,
    _p: PhantomData<fn(&'t ())>,
    _p3: PhantomData<S>,
    columns: Vec<(&'static str, DynPrepared<'a>)>,
}

struct DynPrepared<'a> {
    inner: Box<dyn 'a + FnMut(Row<'_>) -> DynTypedExpr>,
}

impl<'a> DynPrepared<'a> {
    pub fn new(val: impl 'a + FnMut(Row<'_>) -> DynTypedExpr) -> Self {
        Self {
            inner: Box::new(val),
        }
    }
}

impl<'t, 'a, S> CacheAndRead<'t, 'a, S> {
    pub fn col<O: IntoColumn<'a, S>, Impl>(
        &mut self,
        name: &'static str,
        val: impl IntoDummy<'t, 'a, S, Out = O, Impl = Impl>,
    ) where
        Impl: 'a + DummyImpl<'a, Prepared: Prepared<Out = O>>,
    {
        let mut p = val.into_dummy().inner.prepare(&mut self.cacher);
        let p = DynPrepared::new(move |row| p.call(row).into_column().inner.erase());
        self.columns.push((name, p));
    }
}

pub trait TableMigration<'t, 'a> {
    type From: Table;
    type To;

    fn prepare(
        self: Box<Self>,
        prev: Column<'t, <Self::From as Table>::Schema, Self::From>,
        cacher: &mut CacheAndRead<'t, 'a, <Self::From as Table>::Schema>,
    );
}

pub trait TableCreation<'t, 'a> {
    type FromSchema;
    type To;

    fn prepare(self: Box<Self>, cacher: &mut CacheAndRead<'t, 'a, Self::FromSchema>);
}

struct Wrapper<'t, 'a, From: Table, To> {
    inner: Box<dyn 't + TableMigration<'t, 'a, From = From, To = To>>,
    db_id: Column<'t, From::Schema, From>,
}

impl<'t, 'a, From: Table, To> TableCreation<'t, 'a> for Wrapper<'t, 'a, From, To> {
    type FromSchema = From::Schema;
    type To = To;

    fn prepare(self: Box<Self>, cacher: &mut CacheAndRead<'t, 'a, Self::FromSchema>) {
        // keep the ID the same
        cacher.col(From::ID, self.db_id.clone());
        Box::new(self.inner).prepare(self.db_id, cacher);
    }
}

pub struct SchemaBuilder<'x, 'a> {
    // this is used to create temporary table names
    scope: Scope,
    conn: &'x rusqlite::Transaction<'x>,
    drop: Vec<TableDropStatement>,
    rename: Vec<TableRenameStatement>,
    _p: PhantomData<fn(&'a ()) -> &'a ()>,
}

impl<'a> SchemaBuilder<'_, 'a> {
    pub fn migrate_table<From: Table, To: Table>(&mut self, m: M<'a, From, To>) {
        self.create_inner::<From::Schema, To>(|rows| {
            let db_id = From::join(rows);
            let migration = m(db_id.clone());
            Create::new(Wrapper {
                inner: migration.inner,
                db_id,
            })
        });

        self.drop.push(
            sea_query::Table::drop()
                .table(Alias::new(From::NAME))
                .take(),
        );
    }

    pub fn create_from<FromSchema, To: Table>(&mut self, f: C<'a, FromSchema, To>) {
        self.create_inner::<FromSchema, To>(f);
    }

    fn create_inner<FromSchema, To: Table>(
        &mut self,
        f: impl for<'t> FnOnce(&mut Rows<'t, FromSchema>) -> Create<'t, 'a, FromSchema, To>,
    ) {
        let new_table_name = self.scope.tmp_table();
        new_table::<To>(self.conn, new_table_name);

        self.rename.push(
            sea_query::Table::rename()
                .table(new_table_name, Alias::new(To::NAME))
                .take(),
        );

        let mut q = Rows::<FromSchema> {
            phantom: PhantomData,
            ast: MySelect::default(),
            _p: PhantomData,
        };
        let create = f(&mut q);
        let mut cache_and_read = CacheAndRead {
            columns: Vec::new(),
            cacher: Cacher::new(),
            _p: PhantomData,
            _p3: PhantomData,
        };
        create.inner.prepare(&mut cache_and_read);
        let cached = q.ast.cache(cache_and_read.cacher.columns);

        let select = q.ast.simple();
        let (sql, values) = select.build_rusqlite(SqliteQueryBuilder);

        // no caching here, migration is only executed once
        let mut statement = self.conn.prepare(&sql).unwrap();
        let mut rows = statement.query(&*values.as_params()).unwrap();

        while let Some(row) = rows.next().unwrap() {
            let row = Row {
                row,
                fields: &cached,
            };

            let new_ast = MySelect::default();
            let reader = Reader::<FromSchema> {
                ast: &new_ast,
                _p: PhantomData,
                _p2: PhantomData,
            };
            for (name, prepared) in &mut cache_and_read.columns {
                reader.col_erased(name, (prepared.inner)(row));
            }

            let mut insert = InsertStatement::new();
            let names = new_ast.select.iter().map(|(_field, name)| *name);
            insert.into_table(new_table_name);
            insert.columns(names);
            insert.select_from(new_ast.simple()).unwrap();

            let (sql, values) = insert.build_rusqlite(SqliteQueryBuilder);
            let mut statement = self.conn.prepare_cached(&sql).unwrap();
            statement.execute(&*values.as_params()).unwrap();
        }
    }

    pub fn drop_table<T: Table>(&mut self) {
        let name = Alias::new(T::NAME);
        let step = sea_query::Table::drop().table(name).take();
        self.drop.push(step);
    }
}

fn new_table<T: Table>(conn: &Connection, alias: TmpTable) {
    let mut f = crate::hash::TypBuilder::default();
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

pub trait Migration<'a> {
    type From: Schema;
    type To: Schema;

    fn tables(self, b: &mut SchemaBuilder<'_, 'a>);
}

/// [Config] is used to open a database from a file or in memory.
///
/// This is the first step in the [Config] -> [Migrator] -> [Database] chain to
/// get a [Database] instance.
pub struct Config {
    manager: r2d2_sqlite::SqliteConnectionManager,
    init: Box<dyn FnOnce(&rusqlite::Transaction)>,
}

static ALLOWED: AtomicBool = AtomicBool::new(true);

impl Config {
    /// Open a database that is stored in a file.
    /// Creates the database if it does not exist.
    ///
    /// Opening the same database multiple times at the same time is fine,
    /// as long as they migrate to or use the same schema.
    /// All locking is done by sqlite, so connections can even be made using different client implementations.
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
            inner.set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)?;
            Ok(())
        });

        Self {
            manager,
            init: Box::new(|_| {}),
        }
    }

    /// Execute a raw sql statement if the database was just created.
    ///
    /// The statement is executed after creating the empty database and executingall previous statements.
    pub fn init_stmt(mut self, sql: &'static str) -> Self {
        self.init = Box::new(move |txn| {
            (self.init)(txn);

            txn.execute_batch(sql)
                .expect("raw sql statement to populate db failed");
        });
        self
    }
}

impl LocalClient {
    /// Create a [Migrator] to migrate a database.
    ///
    /// Returns [None] if the database `user_version` on disk is older than `S`.
    ///
    /// This function will panic if the schema on disk does not match what is expected for its `user_version`.
    pub fn migrator<'t, S: Schema>(&'t mut self, config: Config) -> Option<Migrator<'t, S>> {
        use r2d2::ManageConnection;
        let conn = self.conn.insert(config.manager.connect().unwrap());
        conn.pragma_update(None, "foreign_keys", "OFF").unwrap();

        let conn = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Exclusive)
            .unwrap();

        // check if this database is newly created
        if schema_version(&conn) == 0 {
            let mut b = TableTypBuilder::default();
            S::typs(&mut b);

            for (table_name, table) in &*b.ast.tables {
                new_table_inner(&conn, table, Alias::new(table_name));
            }
            (config.init)(&conn);
            set_user_version(&conn, S::VERSION).unwrap();
        }

        let user_version = user_version(&conn).unwrap();
        // We can not migrate databases older than `S`
        if user_version < S::VERSION {
            return None;
        } else if user_version == S::VERSION {
            foreign_key_check::<S>(&conn);
        }

        Some(Migrator {
            manager: config.manager,
            transaction: conn,
            _p: PhantomData,
            _local: PhantomData,
        })
    }
}

/// [Migrator] is used to apply database migrations.
///
/// When all migrations are done, it can be turned into a [Database] instance with
/// [Migrator::finish].
pub struct Migrator<'t, S> {
    manager: r2d2_sqlite::SqliteConnectionManager,
    transaction: rusqlite::Transaction<'t>,
    _p: PhantomData<S>,
    // We want to make sure that Migrator is always used with the same LocalClient
    // so we make it local to the current thread.
    // This is mostly important because the LocalClient can have a reference to our transaction.
    _local: PhantomData<LocalClient>,
}

impl<'t, S: Schema> Migrator<'t, S> {
    /// Apply a database migration if the current schema is `S` and return a [Migrator] for the next schema `N`.
    ///
    /// This function will panic if the schema on disk does not match what is expected for its `user_version`.
    pub fn migrate<M, N: Schema>(self, m: M) -> Migrator<'t, N>
    where
        M: Migration<'t, From = S, To = N>,
    {
        let conn = &self.transaction;

        if user_version(conn).unwrap() == S::VERSION {
            let mut builder = SchemaBuilder {
                scope: Default::default(),
                conn,
                drop: vec![],
                rename: vec![],
                _p: PhantomData,
            };
            m.tables(&mut builder);
            for drop in builder.drop {
                let sql = drop.to_string(SqliteQueryBuilder);
                conn.execute(&sql, []).unwrap();
            }
            for rename in builder.rename {
                let sql = rename.to_string(SqliteQueryBuilder);
                conn.execute(&sql, []).unwrap();
            }
            foreign_key_check::<N>(conn);
            set_user_version(conn, N::VERSION).unwrap();
        }

        Migrator {
            manager: self.manager,
            transaction: self.transaction,
            _p: PhantomData,
            _local: PhantomData,
        }
    }

    /// Commit the migration transaction and return a [Database].
    ///
    /// Returns [None] if the database schema version is newer than `S`.
    pub fn finish(self) -> Option<Database<S>> {
        let conn = &self.transaction;
        if user_version(conn).unwrap() != S::VERSION {
            return None;
        }

        let schema_version = schema_version(conn);
        self.transaction.commit().unwrap();

        Some(Database {
            manager: self.manager,
            schema_version,
            schema: PhantomData,
        })
    }
}

pub fn schema_version(conn: &rusqlite::Transaction) -> i64 {
    conn.pragma_query_value(None, "schema_version", |r| r.get(0))
        .unwrap()
}

// Read user version field from the SQLite db
fn user_version(conn: &rusqlite::Transaction) -> Result<i64, rusqlite::Error> {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
}

// Set user version field from the SQLite db
fn set_user_version(conn: &rusqlite::Transaction, v: i64) -> Result<(), rusqlite::Error> {
    conn.pragma_update(None, "user_version", v)
}

fn foreign_key_check<S: Schema>(conn: &rusqlite::Transaction) {
    let errors = conn
        .prepare("PRAGMA foreign_key_check")
        .unwrap()
        .query_map([], |_| Ok(()))
        .unwrap()
        .count();
    if errors != 0 {
        panic!("migration violated foreign key constraint")
    }

    let mut b = TableTypBuilder::default();
    S::typs(&mut b);
    pretty_assertions::assert_eq!(
        b.ast,
        read_schema(crate::Transaction::ref_cast(conn)),
        "schema is different (expected left, but got right)",
    );
}

/// Special table name that is used as souce of newly created tables.
#[derive(Clone, Copy)]
pub struct NoTable(());

impl value::Typed for NoTable {
    type Typ = NoTable;
    fn build_expr(&self, _b: value::ValueBuilder) -> sea_query::SimpleExpr {
        unreachable!("NoTable can not be constructed")
    }
}
impl Private for NoTable {}
impl<'t, S> IntoColumn<'t, S> for NoTable {
    type Typ = NoTable;
    fn into_column(self) -> Column<'t, S, Self::Typ> {
        Column::new(self)
    }
}
