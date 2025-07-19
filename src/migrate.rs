use std::{
    collections::{HashMap, HashSet},
    convert::Infallible,
    marker::PhantomData,
    ops::{Deref, Not},
    path::Path,
    rc::Rc,
    sync::atomic::AtomicBool,
};

use rusqlite::{Connection, config::DbConfig};
use sea_query::{Alias, ColumnDef, IntoTableRef, SqliteQueryBuilder, TableDropStatement};
use self_cell::MutBorrow;

use crate::{
    FromExpr, IntoExpr, Table, TableRow, Transaction,
    alias::{Scope, TmpTable},
    hash,
    schema_pragma::read_schema,
    transaction::{Database, OwnedTransaction, try_insert_private},
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
        let mut b = hash::TypBuilder::default();
        T::typs(&mut b);
        self.ast.tables.insert((T::NAME.to_owned(), b.ast));
    }
}

pub trait Schema: Sized + 'static {
    const VERSION: i64;
    fn typs(b: &mut TableTypBuilder<Self>);
}

pub trait Migration<'t> {
    type FromSchema: 'static;
    type From: Table<Schema = Self::FromSchema>;
    type To: Table<MigrateFrom = Self::From>;
    type Conflict;

    #[doc(hidden)]
    fn prepare(
        val: Self,
        prev: crate::Expr<'t, Self::FromSchema, Self::From>,
    ) -> <Self::To as Table>::Insert<'t>;
    #[doc(hidden)]
    fn map_conflict(val: TableRow<'t, Self::From>) -> Self::Conflict;
}

/// Transaction type for use in migrations.
pub struct TransactionMigrate<FromSchema> {
    inner: Transaction<FromSchema>,
    scope: Scope,
    rename_map: HashMap<&'static str, TmpTable>,
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
            new_table::<T>(self.inner.transaction.get(), new_table_name);
            new_table_name
        })
    }

    fn unmigrated<M: Migration<'static, FromSchema = FromSchema>, Out>(
        &self,
        new_name: TmpTable,
    ) -> impl Iterator<Item = (i64, Out)>
    where
        Out: FromExpr<'static, FromSchema, M::From>,
    {
        let data = self.inner.query(|rows| {
            let old = rows.join(<M::From as Table>::TOKEN);
            rows.into_vec((&old, Out::from_expr(&old)))
        });

        let migrated = Transaction::new(self.inner.transaction.clone()).query(|rows| {
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
        M: Migration<'static, FromSchema = FromSchema>,
        X: FromExpr<'static, FromSchema, M::From>,
    >(
        &mut self,
        mut f: impl FnMut(X) -> Option<M>,
    ) -> Result<(), M::Conflict> {
        let new_name = self.new_table_name::<M::To>();

        for (idx, x) in self.unmigrated::<M, X>(new_name) {
            if let Some(new) = f(x) {
                try_insert_private::<M::To>(
                    &self.transaction,
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
    pub fn migrate<
        M: Migration<'static, FromSchema = FromSchema>,
        X: FromExpr<'static, FromSchema, M::From>,
    >(
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
        M: Migration<'static, FromSchema = FromSchema, Conflict = Infallible>,
        X: FromExpr<'static, FromSchema, M::From>,
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
/// What is nice about this is that a [Transaction] can always be made immediately.
/// Making a [crate::TransactionMut] has to wait until all other [crate::TransactionMut]s are finished.
///
/// Sqlite is also configured with [`synchronous=NORMAL`](https://www.sqlite.org/pragma.html#pragma_synchronous). This gives better performance by fsyncing less.
/// The database will not lose transactions due to application crashes, but it might due to system crashes or power loss.
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

impl<S: Schema> Database<S> {
    /// Create a [Migrator] to migrate a database.
    ///
    /// Returns [None] if the database `user_version` on disk is older than `S`.
    pub fn migrator(config: Config) -> Option<Migrator<S>> {
        use r2d2::ManageConnection;
        let conn = config.manager.connect().unwrap();
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
            let mut b = TableTypBuilder::default();
            S::typs(&mut b);

            for (table_name, table) in &*b.ast.tables {
                new_table_inner(txn.get(), table, Alias::new(table_name));
            }
            (config.init)(txn.get());
            set_user_version(txn.get(), S::VERSION).unwrap();
        }

        let user_version = user_version(txn.get()).unwrap();
        // We can not migrate databases older than `S`
        if user_version < S::VERSION {
            return None;
        }
        assert_eq!(
            foreign_key_check(txn.get()),
            None,
            "foreign key constraint violated"
        );

        Some(Migrator {
            manager: config.manager,
            transaction: Rc::new(txn),
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
    transaction: Rc<OwnedTransaction>,
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
        self,
        m: impl Send + FnOnce(&mut TransactionMigrate<S>) -> M,
    ) -> Migrator<M::To>
    where
        M: SchemaMigration<'x, From = S>,
    {
        if user_version(self.transaction.get()).unwrap() == S::VERSION {
            check_schema::<S>(&self.transaction);

            let mut txn = TransactionMigrate {
                inner: Transaction::new(self.transaction.clone()),
                scope: Default::default(),
                rename_map: HashMap::new(),
            };
            let m = m(&mut txn);

            let mut builder = SchemaBuilder {
                drop: vec![],
                foreign_key: HashMap::new(),
                inner: txn,
            };
            m.tables(&mut builder);

            for drop in builder.drop {
                let sql = drop.to_string(SqliteQueryBuilder);
                self.transaction.get().execute(&sql, []).unwrap();
            }
            for (to, tmp) in builder.inner.rename_map {
                let rename = sea_query::Table::rename().table(tmp, Alias::new(to)).take();
                let sql = rename.to_string(SqliteQueryBuilder);
                self.transaction.get().execute(&sql, []).unwrap();
            }
            if let Some(fk) = foreign_key_check(self.transaction.get()) {
                (builder.foreign_key.remove(&*fk).unwrap())();
            }
            #[allow(
                unreachable_code,
                reason = "rustc is stupid and thinks this is unreachable"
            )]
            set_user_version(self.transaction.get(), M::To::VERSION).unwrap();
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
    pub fn finish(self) -> Option<Database<S>> {
        let conn = &self.transaction;
        if user_version(conn.get()).unwrap() != S::VERSION {
            return None;
        }
        check_schema::<S>(&self.transaction);

        // adds an sqlite_stat1 table
        self.transaction
            .get()
            .execute_batch("PRAGMA optimize;")
            .unwrap();

        let schema_version = schema_version(conn.get());
        Rc::into_inner(self.transaction)
            .unwrap()
            .with(|x| x.commit().unwrap());

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

fn check_schema<S: Schema>(conn: &Rc<OwnedTransaction>) {
    let mut b = TableTypBuilder::default();
    S::typs(&mut b);
    pretty_assertions::assert_eq!(
        b.ast,
        read_schema(&crate::Transaction::new(conn.clone())),
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
