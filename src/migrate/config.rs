use std::path::Path;

#[cfg(doc)]
use crate::migrate::{Database, Migrator};

/// [Config] is used to open a database from a file or in memory.
///
/// This is the first step in the [Config] -> [Migrator] -> [Database] chain to
/// get a [Database] instance.
///
/// # Sqlite config
///
/// Sqlite is configured to be in [WAL mode](https://www.sqlite.org/wal.html).
/// The effect of this mode is that there can be any number of readers with one concurrent writer.
/// What is nice about this is that an immutable [crate::Transaction] can always be made immediately.
/// Making a mutable [crate::Transaction] has to wait until all other mutable [crate::Transaction]s are finished.
pub struct Config {
    pub(super) manager: r2d2_sqlite::SqliteConnectionManager,
    pub(super) init: Box<dyn FnOnce(&rusqlite::Transaction)>,
    /// Configure how often SQLite will synchronize the database to disk.
    ///
    /// The default is [Synchronous::Full].
    pub synchronous: Synchronous,
    /// Configure how foreign keys should be checked.
    ///
    /// The default is [ForeignKeys::SQLite], but this is likely to change to [ForeignKeys::Rust].
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
    #[cfg_attr(test, mutants::skip)] // hard to test
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Synchronous::Full => "FULL",
            Synchronous::Normal => "NORMAL",
        }
    }
}

/// Which method should be used to check foreign-key constraints.
///
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
    pub(crate) fn as_str(self) -> &'static str {
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
    /// All locking is done by SQLite, so connections can even be made using different client implementations.
    ///
    /// IMPORTANT: rust-query uses SQLite in WAL mode. While a connection to the database is open there will
    /// be an additional file with the same name as the database, but with `-wal` appended.
    /// This "write ahead log" is automatically removed when the last connection to the database closes cleanly.
    /// Any `-wal` file should be considered an integral part of the database and as such should be kept together.
    /// For more details see <https://sqlite.org/howtocorrupt.html>.
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
