pub mod config;
pub mod migration;

use std::{
    collections::{BTreeSet, HashMap},
    marker::PhantomData,
    sync::atomic::AtomicI64,
};

use annotate_snippets::{Renderer, renderer::DecorStyle};
use rusqlite::{Connection, config::DbConfig};
use sea_query::{Alias, ColumnDef, IntoTableRef, SqliteQueryBuilder};
use self_cell::MutBorrow;

use crate::{
    Table, Transaction,
    migrate::{
        config::Config,
        migration::{SchemaBuilder, TransactionMigrate},
    },
    pool::Pool,
    schema::{
        from_db, from_macro,
        read::{read_index_names_for_table, read_schema},
    },
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
        let old = self.ast.tables.insert(T::NAME.to_owned(), table);
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

fn new_table_inner(
    conn: &Connection,
    table: &crate::schema::from_macro::Table,
    alias: impl IntoTableRef,
) {
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
            let schema = crate::schema::from_macro::Schema::new::<S>();

            for (table_name, table) in &schema.tables {
                let table_name_ref = Alias::new(table_name);
                new_table_inner(txn.get(), table, table_name_ref);
                for stmt in table.create_indices(table_name) {
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
            indices_fixed: false,
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
    indices_fixed: bool,
    _p: PhantomData<S>,
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
                    TXN.set(Some(TransactionWithRows::new_empty(self.transaction)));
                    let txn = Transaction::new_ref();

                    check_schema::<S>(txn);
                    if !self.indices_fixed {
                        fix_indices::<S>(txn);
                        self.indices_fixed = true;
                    }

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

                    transaction.into_owner()
                })
                .join()
            });
            match res {
                Ok(val) => self.transaction = val,
                Err(payload) => std::panic::resume_unwind(payload),
            }
        }

        Migrator {
            indices_fixed: self.indices_fixed,
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
        if user_version(self.transaction.get()).unwrap() != S::VERSION {
            return None;
        }

        let res = std::thread::scope(|s| {
            s.spawn(|| {
                TXN.set(Some(TransactionWithRows::new_empty(self.transaction)));
                let txn = Transaction::new_ref();

                check_schema::<S>(txn);
                if !self.indices_fixed {
                    fix_indices::<S>(txn);
                    self.indices_fixed = true;
                }

                TXN.take().unwrap().into_owner()
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

    for (name, table) in schema.tables {
        let expected_table = &expected_schema.tables[&name];

        if !check_eq(expected_table, &table) {
            // Delete all indices associated with the table
            for index_name in read_index_names_for_table(&crate::Transaction::new(), &name) {
                let sql = sea_query::Index::drop()
                    .name(index_name)
                    .build(SqliteQueryBuilder);
                txn.execute(&sql);
            }

            // Add the new indices
            for sql in expected_table.create_indices(&name) {
                txn.execute(&sql);
            }
        }
    }

    // check that we solved the mismatch
    let schema = read_schema(txn);
    for (name, table) in schema.tables {
        let expected_table = &expected_schema.tables[&name];
        assert!(check_eq(expected_table, &table));
    }
}

impl<S> Transaction<S> {
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
                .query_map([], |row| row.get("sql"))
                .unwrap()
                .map(|x| x.unwrap())
                .collect()
        })
    }
}
