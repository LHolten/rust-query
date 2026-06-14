pub mod config;
mod fix_by_copy;
pub mod migration;
#[cfg(test)]
mod test;

use std::{
    cell::RefCell, collections::HashMap, marker::PhantomData, mem::take, sync::atomic::AtomicI64,
};

use annotate_snippets::{Group, Renderer, renderer::DecorStyle};
use self_cell::MutBorrow;

use crate::{
    Table, Transaction,
    lower::{self, list_writer::Alias},
    migrate::{
        config::Config,
        fix_by_copy::fix_by_copy,
        migration::{SchemaBuilder, TransactionMigrate},
    },
    pool::Pool,
    schema::{from_macro, read::read_schema},
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
        let pool = Pool::new(config);

        let conn = pool.pop();
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

            for (table_name, table) in schema.tables {
                let table = table.to_db();
                let create = table.create(lower::JoinableTable::Table(table_name), "id");
                txn.get().execute(&create, []).unwrap();
                for stmt in table.delayed_indices(table_name) {
                    txn.get().execute(&stmt, []).unwrap();
                }
            }
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
            pool,
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
    pool: Pool,
    transaction: OwnedTransaction,
    // Initialized to the user version when the transaction starts.
    // This is set to None if the schema user_version is updated.
    // Fixups are only applied if the user_version is None.
    // Indices are fixed before this is set to None.
    user_version: Option<i64>,
    _p: PhantomData<S>,
}

impl<S: Schema> Migrator<S> {
    fn with_transaction(mut self, f: impl Send + FnOnce(&'static mut Transaction<S>)) -> Self {
        assert!(self.user_version.is_none_or(|x| x == S::VERSION));
        let res = std::thread::scope(|s| {
            s.spawn(|| {
                TXN.set(Some(TransactionWithRows::new_empty(self.transaction)));
                let txn = Transaction::new_ref();

                // check if this is the first migration that is applied
                if self.user_version.take().is_some() {
                    // we check the schema before doing any migrations
                    check_schema::<S>(txn)?;
                    // fixing indices before migrations can help with migration performance
                    fix_by_copy::<S>(txn, fix_by_copy::Detail::Indexes);
                }

                f(txn);

                let transaction = TXN.take().unwrap();

                Ok::<_, Renderable>(transaction.into_owner())
            })
            .join()
        });
        match res {
            Ok(val) => self.transaction = val.unwrap_or_else(|e| e.to_panic()),
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
                let txn = builder.inner.inner;

                for drop in builder.drop {
                    txn.execute(&drop);
                }
                for (to, tmp) in builder.inner.rename_map {
                    txn.execute(&format!("ALTER TABLE main.{tmp} RENAME TO {}", Alias(to)));
                }
                for stmt in builder.inner.extra_index {
                    txn.execute(&stmt);
                }

                // Change transaction schema because we are now on the new version already
                fix_by_copy::<M::To>(&Transaction::new(), fix_by_copy::Detail::ForeignKeys);

                let transaction = TXN.take().unwrap();
                if let Some(fk) = foreign_key_check(transaction.get()) {
                    (builder.foreign_key.remove(&*fk).unwrap())();
                }

                TXN.set(Some(transaction));
            });
        }

        Migrator {
            user_version: self.user_version,
            pool: self.pool,
            transaction: self.transaction,
            _p: PhantomData,
        }
    }

    /// Mutate the database as part of migrations.
    ///
    /// The closure will only be executed if the database got migrated to schema version `S`
    /// by this [Migrator] instance.
    /// If [Migrator::fixup] is used before all [Migrator::migrate], then the closures is only executed
    /// when the database is created.
    pub fn fixup(mut self, f: impl Send + FnOnce(&'static mut Transaction<S>)) -> Self {
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
            check_schema::<S>(txn).unwrap_or_else(|e| e.as_sanity())
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
            pool: self.pool,
            schema_version: AtomicI64::new(schema_version),
            schema: PhantomData,
            mut_lock: parking_lot::FairMutex::new(()),
        })
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

pub(crate) fn check_schema<S: Schema>(txn: &Transaction<S>) -> Result<(), Renderable> {
    let from_macro = crate::schema::from_macro::Schema::new::<S>();
    let from_db = read_schema(txn);
    let report = from_db.diff(from_macro, S::SOURCE, S::PATH, S::VERSION);
    if report.is_empty() {
        Ok(())
    } else {
        Err(Renderable(report))
    }
}

pub struct Renderable(Vec<Group<'static>>);

impl Renderable {
    /// [Renderable] should be made into a panic on the thread of the caller.
    pub fn to_panic(self) -> ! {
        let renderer = RENDERER
            .with_borrow(Clone::clone)
            .decor_style(DecorStyle::Unicode);
        panic!("{}", renderer.render(&self.0))
    }

    pub fn as_sanity(self) -> ! {
        unreachable!(
            "THIS IS A RUST-QUERY BUG {}",
            Renderer::plain().render(&self.0)
        );
    }
}

thread_local! {
    static RENDERER: RefCell<Renderer> = const { RefCell::new(Renderer::styled()) }
}

pub fn with_test_renderer<R>(f: impl FnOnce() -> R) -> R {
    struct TestRenderGuard(Option<Renderer>);
    impl Drop for TestRenderGuard {
        fn drop(&mut self) {
            RENDERER.set(take(&mut self.0).unwrap());
        }
    }
    let _g = TestRenderGuard(Some(
        RENDERER.replace(Renderer::plain().anonymized_line_numbers(true)),
    ));
    f()
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
