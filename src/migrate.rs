pub mod config;
pub mod migration;
#[cfg(test)]
mod test;

use std::{
    collections::{BTreeSet, HashMap},
    marker::PhantomData,
    sync::atomic::AtomicI64,
};

use annotate_snippets::{Renderer, renderer::DecorStyle};
use rusqlite::config::DbConfig;
use sea_query::{Alias, ColumnDef, IntoIden, SqliteQueryBuilder};
use self_cell::MutBorrow;

use crate::{
    Table, Transaction,
    alias::Scope,
    migrate::{
        config::Config,
        migration::{SchemaBuilder, TransactionMigrate},
    },
    pool::Pool,
    schema::{from_db, from_macro, read::read_schema},
    transaction::{Database, OwnedTransaction, TXN, TransactionWithRows},
};

pub struct TableTypBuilder<S> {
    pub(crate) ast: from_macro::Schema,
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
        let table = from_macro::Table::new::<T>();
        let old = self.ast.tables.insert(T::NAME, table);
        debug_assert!(old.is_none());
    }
}

pub trait Schema: Sized + 'static {
    const VERSION: i64;
    const SOURCE: &str;
    const PATH: &str;
    const SPAN: (usize, usize);
    fn typs(b: &mut TableTypBuilder<Self>);
}

fn new_table_inner(table: &crate::schema::from_macro::Table, alias: impl IntoIden) -> String {
    let alias = alias.into_iden();
    let mut create = table.create();
    create
        .table(alias.clone())
        .col(ColumnDef::new(Alias::new("id")).integer().primary_key());
    let mut sql = create.to_string(SqliteQueryBuilder);
    sql.push_str(" STRICT");
    sql
}

pub trait SchemaMigration<'a> {
    type From: Schema;
    type To: Schema;

    fn tables(self, b: &mut SchemaBuilder<'a, Self::From>);
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

        let mut user_version = Some(user_version(txn.get()).unwrap());

        // check if this database is newly created
        if schema_version(txn.get()) == 0 {
            user_version = None;

            let schema = crate::schema::from_macro::Schema::new::<S>();

            for (&table_name, table) in &schema.tables {
                txn.get()
                    .execute(&new_table_inner(table, table_name), [])
                    .unwrap();
                for stmt in table.delayed_indices(table_name) {
                    txn.get().execute(&stmt, []).unwrap();
                }
            }
            (config.init)(txn.get());
        } else if user_version.unwrap() < S::VERSION {
            // We can not migrate databases older than `S`
            return None;
        }

        debug_assert_eq!(
            foreign_key_check(txn.get()),
            None,
            "foreign key constraint violated"
        );

        Some(Migrator {
            user_version,
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
    // Initialized to the user version when the transaction starts.
    // This is set to None if the schema user_version is updated.
    // Fixups are only applied if the user_version is None.
    // Indices are fixed before this is set to None.
    user_version: Option<i64>,
    _p: PhantomData<S>,
}

impl<S: Schema> Migrator<S> {
    fn with_transaction(mut self, f: impl Send + FnOnce(&mut Transaction<S>)) -> Self {
        assert!(self.user_version.is_none_or(|x| x == S::VERSION));
        let res = std::thread::scope(|s| {
            s.spawn(|| {
                TXN.set(Some(TransactionWithRows::new_empty(self.transaction)));
                let txn = Transaction::new_ref();

                // check if this is the first migration that is applied
                if self.user_version.take().is_some() {
                    // we check the schema before doing any migrations
                    check_schema::<S>(txn);
                    // fixing indices before migrations can help with migration performance
                    fix_indices::<S>(txn);
                }

                f(txn);

                let transaction = TXN.take().unwrap();

                transaction.into_owner()
            })
            .join()
        });
        match res {
            Ok(val) => self.transaction = val,
            Err(payload) => std::panic::resume_unwind(payload),
        }
        self
    }

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
        if self.user_version.is_none_or(|x| x == S::VERSION) {
            self = self.with_transaction(|txn| {
                let mut txn = TransactionMigrate {
                    inner: txn.copy(),
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
                // adding non unique indexes is fine to do after checking foreign keys
                for stmt in builder.inner.extra_index {
                    transaction.get().execute(&stmt, []).unwrap();
                }

                TXN.set(Some(transaction));
            });
        }

        Migrator {
            user_version: self.user_version,
            manager: self.manager,
            transaction: self.transaction,
            _p: PhantomData,
        }
    }

    /// Mutate the database as part of migrations.
    ///
    /// The closure will only be executed if the database got migrated to schema version `S`
    /// by this [Migrator] instance.
    /// If [Migrator::fixup] is used before [Migrator::migrate], then the closures is only executed
    /// when the database is created.
    pub fn fixup(mut self, f: impl Send + FnOnce(&mut Transaction<S>)) -> Self {
        if self.user_version.is_none() {
            self = self.with_transaction(f);
        }
        self
    }

    /// Commit the migration transaction and return a [Database].
    ///
    /// Returns [None] if the database schema version is newer than `S`.
    ///
    /// This function will panic if the schema on disk does not match what is expected for its `user_version`.
    pub fn finish(mut self) -> Option<Database<S>> {
        if self.user_version.is_some_and(|x| x != S::VERSION) {
            return None;
        }

        // This checks that the schema is correct and fixes indices etc
        self = self.with_transaction(|txn| {
            // sanity check, this should never fail
            check_schema::<S>(txn);
        });

        // adds an sqlite_stat1 table
        self.transaction
            .get()
            .execute_batch("PRAGMA optimize;")
            .unwrap();

        set_user_version(self.transaction.get(), S::VERSION).unwrap();
        let schema_version = schema_version(self.transaction.get());
        self.transaction.with(|x| x.commit().unwrap());

        Some(Database {
            manager: Pool::new(self.manager),
            schema_version: AtomicI64::new(schema_version),
            schema: PhantomData,
            mut_lock: parking_lot::FairMutex::new(()),
        })
    }
}

fn fix_indices<S: Schema>(txn: &Transaction<S>) {
    let schema = read_schema(txn);
    let expected_schema = crate::schema::from_macro::Schema::new::<S>();

    fn check_eq(expected: &from_macro::Table, actual: &from_db::Table) -> bool {
        let expected: BTreeSet<_> = expected.indices.iter().map(|idx| &idx.def).collect();
        let actual: BTreeSet<_> = actual.indices.values().collect();
        expected == actual
    }

    for (&table_name, expected_table) in &expected_schema.tables {
        let table = &schema.tables[table_name];

        if !check_eq(expected_table, &table) {
            // Unique constraints that are part of a table definition
            // can not be dropped, so we assume the worst and just recreate
            // the whole table.

            let scope = Scope::default();
            let tmp_name = scope.tmp_table();

            txn.execute(&new_table_inner(expected_table, tmp_name));

            let mut columns: Vec<_> = expected_table
                .columns
                .keys()
                .map(|x| Alias::new(x))
                .collect();
            columns.push(Alias::new("id"));

            txn.execute(
                &sea_query::InsertStatement::new()
                    .into_table(tmp_name)
                    .columns(columns.clone())
                    .select_from(
                        sea_query::SelectStatement::new()
                            .from(table_name)
                            .columns(columns)
                            .take(),
                    )
                    .unwrap()
                    .build(SqliteQueryBuilder)
                    .0,
            );

            txn.execute(
                &sea_query::TableDropStatement::new()
                    .table(table_name)
                    .build(SqliteQueryBuilder),
            );

            txn.execute(
                &sea_query::TableRenameStatement::new()
                    .table(tmp_name, table_name)
                    .build(SqliteQueryBuilder),
            );
            // Add the new non-unique indices
            for sql in expected_table.delayed_indices(table_name) {
                txn.execute(&sql);
            }
        }
    }

    // check that we solved the mismatch
    let schema = read_schema(txn);
    for (name, table) in schema.tables {
        let expected_table = &expected_schema.tables[&*name];
        assert!(check_eq(expected_table, &table));
    }
}

impl<S> Transaction<S> {
    #[track_caller]
    pub(crate) fn execute(&self, sql: &str) {
        TXN.with_borrow(|txn| txn.as_ref().unwrap().get().execute(sql, []))
            .unwrap();
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

pub(crate) fn check_schema<S: Schema>(txn: &Transaction<S>) {
    let from_macro = crate::schema::from_macro::Schema::new::<S>();
    let from_db = read_schema(txn);
    let report = from_db.diff(from_macro, S::SOURCE, S::PATH, S::VERSION);
    if !report.is_empty() {
        let renderer = if cfg!(test) {
            Renderer::plain().anonymized_line_numbers(true)
        } else {
            Renderer::styled()
        }
        .decor_style(DecorStyle::Unicode);
        panic!("{}", renderer.render(&report));
    }
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

impl<S> Transaction<S> {
    #[cfg(test)]
    pub(crate) fn schema(&self) -> Vec<String> {
        TXN.with_borrow(|x| {
            x.as_ref()
                .unwrap()
                .get()
                .prepare("SELECT sql FROM 'main'.'sqlite_schema'")
                .unwrap()
                .query_map([], |row| row.get::<_, Option<String>>("sql"))
                .unwrap()
                .flat_map(|x| x.unwrap())
                .collect()
        })
    }
}
