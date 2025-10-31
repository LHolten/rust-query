use std::{
    collections::{HashMap, HashSet},
    convert::Infallible,
    marker::PhantomData,
    ops::{Deref, Not},
    path::Path,
    sync::atomic::AtomicI64,
};

use rusqlite::{Connection, config::DbConfig};
use sea_query::{Alias, ColumnDef, IntoTableRef, SqliteQueryBuilder, TableDropStatement};
use self_cell::MutBorrow;

use crate::{
    FromExpr, IntoExpr, Table, TableRow, Transaction,
    alias::{Scope, TmpTable},
    hash,
    schema_pragma::read_schema,
    transaction::{Database, OwnedTransaction, TXN, try_insert_private},
};

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
        let table = hash::Table::new::<T>();
        let old = self.ast.tables.insert(T::NAME.to_owned(), table);
        debug_assert!(old.is_none());
    }
}

pub trait Schema: Sized + 'static {
    const VERSION: i64;
    fn typs(b: &mut TableTypBuilder<Self>);
}

pub trait Migration {
    type FromSchema: 'static;
    type From: Table<Schema = Self::FromSchema>;
    type To: Table<MigrateFrom = Self::From>;
    type Conflict;

    #[doc(hidden)]
    fn prepare(
        val: Self,
        prev: crate::Expr<'static, Self::FromSchema, Self::From>,
    ) -> <Self::To as Table>::Insert;
    #[doc(hidden)]
    fn map_conflict(val: TableRow<Self::From>) -> Self::Conflict;
}

/// Transaction type for use in migrations.
pub struct TransactionMigrate<FromSchema> {
    inner: Transaction<FromSchema>,
    scope: Scope,
    rename_map: HashMap<&'static str, TmpTable>,
    // creating indices is delayed so that they don't need to be renamed
    extra_index: Vec<String>,
}

impl<FromSchema> Deref for TransactionMigrate<FromSchema> {
    type Target = Transaction<FromSchema>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<FromSchema> TransactionMigrate<FromSchema> {
    fn new_table_name<T: Table>(&mut self) -> TmpTable {
        *self.rename_map.entry(T::NAME).or_insert_with(|| {
            let new_table_name = self.scope.tmp_table();
            TXN.with_borrow(|txn| {
                let conn = txn.as_ref().unwrap().get();
                let table = crate::hash::Table::new::<T>();
                let extra_indices = new_table_inner(conn, &table, new_table_name, T::NAME);
                self.extra_index.extend(extra_indices);
            });
            new_table_name
        })
    }

    fn unmigrated<M: Migration<FromSchema = FromSchema>, Out>(
        &self,
        new_name: TmpTable,
    ) -> impl Iterator<Item = (i64, Out)>
    where
        Out: FromExpr<FromSchema, M::From>,
    {
        let data = self.inner.query(|rows| {
            let old = rows.join_private::<M::From>();
            rows.into_vec((&old, Out::from_expr(&old)))
        });

        let migrated = Transaction::new().query(|rows| {
            let new = rows.join_tmp::<M::From>(new_name);
            rows.into_vec(new)
        });
        let migrated: HashSet<_> = migrated.into_iter().map(|x| x.inner.idx).collect();

        data.into_iter().filter_map(move |(row, data)| {
            migrated
                .contains(&row.inner.idx)
                .not()
                .then_some((row.inner.idx, data))
        })
    }

    /// Migrate some rows to the new schema.
    ///
    /// This will return an error when there is a conflict.
    /// The error type depends on the number of unique constraints that the
    /// migration can violate:
    /// - 0 => [Infallible]
    /// - 1.. => `TableRow<T::From>` (row in the old table that could not be migrated)
    pub fn migrate_optional<
        M: Migration<FromSchema = FromSchema>,
        X: FromExpr<FromSchema, M::From>,
    >(
        &mut self,
        mut f: impl FnMut(X) -> Option<M>,
    ) -> Result<(), M::Conflict> {
        let new_name = self.new_table_name::<M::To>();

        for (idx, x) in self.unmigrated::<M, X>(new_name) {
            if let Some(new) = f(x) {
                try_insert_private::<M::To>(
                    new_name.into_table_ref(),
                    Some(idx),
                    M::prepare(new, TableRow::new(idx).into_expr()),
                )
                .map_err(|_| M::map_conflict(TableRow::new(idx)))?;
            };
        }
        Ok(())
    }

    /// Migrate all rows to the new schema.
    ///
    /// Conflict errors work the same as in [Self::migrate_optional].
    ///
    /// However, this method will return [Migrated] when all rows are migrated.
    /// This can then be used as proof that there will be no foreign key violations.
    pub fn migrate<M: Migration<FromSchema = FromSchema>, X: FromExpr<FromSchema, M::From>>(
        &mut self,
        mut f: impl FnMut(X) -> M,
    ) -> Result<Migrated<'static, FromSchema, M::To>, M::Conflict> {
        self.migrate_optional::<M, X>(|x| Some(f(x)))?;

        Ok(Migrated {
            _p: PhantomData,
            f: Box::new(|_| {}),
            _local: PhantomData,
        })
    }

    /// Helper method for [Self::migrate].
    ///
    /// It can only be used when the migration is known to never cause unique constraint conflicts.
    pub fn migrate_ok<
        M: Migration<FromSchema = FromSchema, Conflict = Infallible>,
        X: FromExpr<FromSchema, M::From>,
    >(
        &mut self,
        f: impl FnMut(X) -> M,
    ) -> Migrated<'static, FromSchema, M::To> {
        let Ok(res) = self.migrate(f);
        res
    }
}

pub struct SchemaBuilder<'t, FromSchema> {
    inner: TransactionMigrate<FromSchema>,
    drop: Vec<TableDropStatement>,
    foreign_key: HashMap<&'static str, Box<dyn 't + FnOnce() -> Infallible>>,
}

impl<'t, FromSchema: 'static> SchemaBuilder<'t, FromSchema> {
    pub fn foreign_key<To: Table>(&mut self, err: impl 't + FnOnce() -> Infallible) {
        self.inner.new_table_name::<To>();

        self.foreign_key.insert(To::NAME, Box::new(err));
    }

    pub fn create_empty<To: Table>(&mut self) {
        self.inner.new_table_name::<To>();
    }

    pub fn drop_table<T: Table>(&mut self) {
        let name = Alias::new(T::NAME);
        let step = sea_query::Table::drop().table(name).take();
        self.drop.push(step);
    }
}

fn new_table_inner(
    conn: &Connection,
    table: &crate::hash::Table,
    alias: impl IntoTableRef,
    index_table: &str,
) -> Vec<String> {
    let mut extra_indices = Vec::new();
    let mut create = table.create(&mut extra_indices);
    create
        .table(alias)
        .col(ColumnDef::new(Alias::new("id")).integer().primary_key());
    let mut sql = create.to_string(SqliteQueryBuilder);
    sql.push_str(" STRICT");
    conn.execute(&sql, []).unwrap();

    let index_table_ref = Alias::new(index_table);
    extra_indices
        .into_iter()
        .enumerate()
        .map(|(index_num, mut index)| {
            index
                .table(index_table_ref.clone())
                .name(format!("{index_table}_index_{index_num}"))
                .to_string(SqliteQueryBuilder)
        })
        .collect()
}

pub trait SchemaMigration<'a> {
    type From: Schema;
    type To: Schema;

    fn tables(self, b: &mut SchemaBuilder<'a, Self::From>);
}

/// [Config] is used to open a database from a file or in memory.
///
/// This is the first step in the [Config] -> [Migrator] -> [Database] chain to
/// get a [Database] instance.
///
/// # Sqlite config
///
/// Sqlite is configured to be in [WAL mode](https://www.sqlite.org/wal.html).
/// The effect of this mode is that there can be any number of readers with one concurrent writer.
/// What is nice about this is that a `&`[crate::Transaction] can always be made immediately.
/// Making a `&mut`[crate::Transaction] has to wait until all other `&mut`[crate::Transaction]s are finished.
pub struct Config {
    manager: r2d2_sqlite::SqliteConnectionManager,
    init: Box<dyn FnOnce(&rusqlite::Transaction)>,
    /// Configure how often SQLite will synchronize the database to disk.
    ///
    /// The default is [Synchronous::Full].
    pub synchronous: Synchronous,
    /// Configure how foreign keys should be checked.
    pub foreign_keys: ForeignKeys,
}

/// <https://www.sqlite.org/pragma.html#pragma_synchronous>
///
/// Note that the database uses WAL mode, so make sure to read the WAL specific section.
#[non_exhaustive]
pub enum Synchronous {
    /// SQLite will fsync after every transaction.
    ///
    /// Transactions are durable, even following a power failure or hard reboot.
    Full,

    /// SQLite will only do essential fsync to prevent corruption.
    ///
    /// The database will not rollback transactions due to application crashes, but it might rollback due to a hardware reset or power loss.
    /// Use this when performance is more important than durability.
    Normal,
}

impl Synchronous {
    fn as_str(self) -> &'static str {
        match self {
            Synchronous::Full => "FULL",
            Synchronous::Normal => "NORMAL",
        }
    }
}

/// Which method should be used to check foreign-key constraints.
///
/// The default is [ForeignKeys::SQLite], but this is likely to change to [ForeignKeys::Rust].
#[non_exhaustive]
pub enum ForeignKeys {
    /// Foreign-key constraints are checked by rust-query only.
    ///
    /// Most foreign-key checks are done at compile time and are thus completely free.
    /// However, some runtime checks are required for deletes.
    Rust,

    /// Foreign-key constraints are checked by SQLite in addition to the checks done by rust-query.
    ///
    /// This is useful when using rust-query with [crate::TransactionWeak::rusqlite_transaction]
    /// or when other software can write to the database.
    /// Both can result in "dangling" foreign keys (which point at a non-existent row) if written incorrectly.
    /// Dangling foreign keys can result in wrong results, but these dangling foreign keys can also turn
    /// into "false" foreign keys if a new record is inserted that makes the foreign key valid.
    /// This is a lot worse than a dangling foreign key, because it is generally not possible to detect.
    ///
    /// With the [ForeignKeys::SQLite] option, rust-query will prevent creating such false foreign keys
    /// and panic instead.
    /// The downside is that indexes are required on all foreign keys to make the checks efficient.
    SQLite,
}

impl ForeignKeys {
    fn as_str(self) -> &'static str {
        match self {
            ForeignKeys::Rust => "OFF",
            ForeignKeys::SQLite => "ON",
        }
    }
}

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
        Self {
            manager,
            init: Box::new(|_| {}),
            synchronous: Synchronous::Full,
            foreign_keys: ForeignKeys::SQLite,
        }
    }

    /// Append a raw sql statement to be executed if the database was just created.
    ///
    /// The statement is executed after creating the empty database and executing all previous statements.
    pub fn init_stmt(mut self, sql: &'static str) -> Self {
        self.init = Box::new(move |txn| {
            (self.init)(txn);

            txn.execute_batch(sql)
                .expect("raw sql statement to populate db failed");
        });
        self
    }
}

impl<S: Schema> Database<S> {
    /// Create a [Migrator] to migrate a database.
    ///
    /// Returns [None] if the database `user_version` on disk is older than `S`.
    pub fn migrator(config: Config) -> Option<Migrator<S>> {
        let synchronous = config.synchronous.as_str();
        let foreign_keys = config.foreign_keys.as_str();
        let manager = config.manager.with_init(move |inner| {
            inner.pragma_update(None, "journal_mode", "WAL")?;
            inner.pragma_update(None, "synchronous", synchronous)?;
            inner.pragma_update(None, "foreign_keys", foreign_keys)?;
            inner.set_db_config(DbConfig::SQLITE_DBCONFIG_DQS_DDL, false)?;
            inner.set_db_config(DbConfig::SQLITE_DBCONFIG_DQS_DML, false)?;
            inner.set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)?;
            Ok(())
        });

        use r2d2::ManageConnection;
        let conn = manager.connect().unwrap();
        conn.pragma_update(None, "foreign_keys", "OFF").unwrap();
        let txn = OwnedTransaction::new(MutBorrow::new(conn), |conn| {
            Some(
                conn.borrow_mut()
                    .transaction_with_behavior(rusqlite::TransactionBehavior::Exclusive)
                    .unwrap(),
            )
        });

        // check if this database is newly created
        if schema_version(txn.get()) == 0 {
            let schema = crate::hash::Schema::new::<S>();

            for (table_name, table) in &schema.tables {
                let table_name_ref = Alias::new(table_name);
                let extra_indices = new_table_inner(txn.get(), table, table_name_ref, table_name);
                for stmt in extra_indices {
                    txn.get().execute(&stmt, []).unwrap();
                }
            }
            (config.init)(txn.get());
            set_user_version(txn.get(), S::VERSION).unwrap();
        }

        let user_version = user_version(txn.get()).unwrap();
        // We can not migrate databases older than `S`
        if user_version < S::VERSION {
            return None;
        }
        debug_assert_eq!(
            foreign_key_check(txn.get()),
            None,
            "foreign key constraint violated"
        );

        Some(Migrator {
            manager,
            transaction: txn,
            _p: PhantomData,
        })
    }
}

/// [Migrator] is used to apply database migrations.
///
/// When all migrations are done, it can be turned into a [Database] instance with
/// [Migrator::finish].
pub struct Migrator<S> {
    manager: r2d2_sqlite::SqliteConnectionManager,
    transaction: OwnedTransaction,
    _p: PhantomData<S>,
}

/// [Migrated] provides a proof of migration.
///
/// This only needs to be provided for tables that are migrated from a previous table.
pub struct Migrated<'t, FromSchema, T> {
    _p: PhantomData<T>,
    f: Box<dyn 't + FnOnce(&mut SchemaBuilder<'t, FromSchema>)>,
    _local: PhantomData<*const ()>,
}

impl<'t, FromSchema: 'static, T: Table> Migrated<'t, FromSchema, T> {
    /// Don't migrate the remaining rows.
    ///
    /// This can cause foreign key constraint violations, which is why an error callback needs to be provided.
    pub fn map_fk_err(err: impl 't + FnOnce() -> Infallible) -> Self {
        Self {
            _p: PhantomData,
            f: Box::new(|x| x.foreign_key::<T>(err)),
            _local: PhantomData,
        }
    }

    #[doc(hidden)]
    pub fn apply(self, b: &mut SchemaBuilder<'t, FromSchema>) {
        (self.f)(b)
    }
}

impl<S: Schema> Migrator<S> {
    /// Apply a database migration if the current schema is `S` and return a [Migrator] for the next schema `N`.
    ///
    /// This function will panic if the schema on disk does not match what is expected for its `user_version`.
    pub fn migrate<'x, M>(
        mut self,
        m: impl Send + FnOnce(&mut TransactionMigrate<S>) -> M,
    ) -> Migrator<M::To>
    where
        M: SchemaMigration<'x, From = S>,
    {
        if user_version(self.transaction.get()).unwrap() == S::VERSION {
            let res = std::thread::scope(|s| {
                s.spawn(|| {
                    TXN.set(Some(self.transaction));

                    check_schema::<S>();

                    let mut txn = TransactionMigrate {
                        inner: Transaction::new(),
                        scope: Default::default(),
                        rename_map: HashMap::new(),
                        extra_index: Vec::new(),
                    };
                    let m = m(&mut txn);

                    let mut builder = SchemaBuilder {
                        drop: vec![],
                        foreign_key: HashMap::new(),
                        inner: txn,
                    };
                    m.tables(&mut builder);

                    let transaction = TXN.take().unwrap();

                    for drop in builder.drop {
                        let sql = drop.to_string(SqliteQueryBuilder);
                        transaction.get().execute(&sql, []).unwrap();
                    }
                    for (to, tmp) in builder.inner.rename_map {
                        let rename = sea_query::Table::rename().table(tmp, Alias::new(to)).take();
                        let sql = rename.to_string(SqliteQueryBuilder);
                        transaction.get().execute(&sql, []).unwrap();
                    }
                    if let Some(fk) = foreign_key_check(transaction.get()) {
                        (builder.foreign_key.remove(&*fk).unwrap())();
                    }
                    #[allow(
                        unreachable_code,
                        reason = "rustc is stupid and thinks this is unreachable"
                    )]
                    // adding indexes is fine to do after checking foreign keys
                    for stmt in builder.inner.extra_index {
                        transaction.get().execute(&stmt, []).unwrap();
                    }
                    set_user_version(transaction.get(), M::To::VERSION).unwrap();

                    transaction
                })
                .join()
            });
            match res {
                Ok(val) => self.transaction = val,
                Err(payload) => std::panic::resume_unwind(payload),
            }
        }

        Migrator {
            manager: self.manager,
            transaction: self.transaction,
            _p: PhantomData,
        }
    }

    /// Commit the migration transaction and return a [Database].
    ///
    /// Returns [None] if the database schema version is newer than `S`.
    ///
    /// This function will panic if the schema on disk does not match what is expected for its `user_version`.
    pub fn finish(mut self) -> Option<Database<S>> {
        let conn = &self.transaction;
        if user_version(conn.get()).unwrap() != S::VERSION {
            return None;
        }

        let res = std::thread::scope(|s| {
            s.spawn(|| {
                TXN.set(Some(self.transaction));
                check_schema::<S>();
                TXN.take().unwrap()
            })
            .join()
        });
        match res {
            Ok(val) => self.transaction = val,
            Err(payload) => std::panic::resume_unwind(payload),
        }

        // adds an sqlite_stat1 table
        self.transaction
            .get()
            .execute_batch("PRAGMA optimize;")
            .unwrap();

        let schema_version = schema_version(self.transaction.get());
        self.transaction.with(|x| x.commit().unwrap());

        Some(Database {
            manager: self.manager,
            schema_version: AtomicI64::new(schema_version),
            schema: PhantomData,
            mut_lock: parking_lot::FairMutex::new(()),
        })
    }
}

pub fn schema_version(conn: &rusqlite::Transaction) -> i64 {
    conn.pragma_query_value(None, "schema_version", |r| r.get(0))
        .unwrap()
}

// Read user version field from the SQLite db
pub fn user_version(conn: &rusqlite::Transaction) -> Result<i64, rusqlite::Error> {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
}

// Set user version field from the SQLite db
fn set_user_version(conn: &rusqlite::Transaction, v: i64) -> Result<(), rusqlite::Error> {
    conn.pragma_update(None, "user_version", v)
}

pub(crate) fn check_schema<S: Schema>() {
    // normalize both sides, because we only care about compatibility
    pretty_assertions::assert_eq!(
        crate::hash::Schema::new::<S>().normalize(),
        read_schema(&crate::Transaction::new()).normalize(),
        "schema is different (expected left, but got right)",
    );
}

fn foreign_key_check(conn: &rusqlite::Transaction) -> Option<String> {
    let error = conn
        .prepare("PRAGMA foreign_key_check")
        .unwrap()
        .query_map([], |row| row.get(2))
        .unwrap()
        .next();
    error.transpose().unwrap()
}

#[test]
fn open_multiple() {
    #[crate::migration::schema(Empty)]
    pub mod vN {}

    let _a = Database::<v0::Empty>::migrator(Config::open_in_memory());
    let _b = Database::<v0::Empty>::migrator(Config::open_in_memory());
}
