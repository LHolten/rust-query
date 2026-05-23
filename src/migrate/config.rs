use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicUsize,
};

use rusqlite::config::DbConfig;

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
    pub(super) source: PathBuf,
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
    pub(crate) fn as_str(&self) -> &'static str {
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
    pub(crate) fn as_str(&self) -> &'static str {
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
        Self::open_internal(p.as_ref().to_path_buf())
    }

    /// Creates a new empty database in memory.
    pub fn open_in_memory() -> Self {
        static IDX: AtomicUsize = AtomicUsize::new(0);
        let idx = IDX.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let uri = format!("file:{idx}?mode=memory&cache=shared");
        Self::open_internal(PathBuf::from(uri))
    }

    fn open_internal(source: PathBuf) -> Self {
        Self {
            source,
            synchronous: Synchronous::Full,
            foreign_keys: ForeignKeys::SQLite,
        }
    }

    /// [Self::connect] should always be used through [crate::pool::Pool::pop]!
    /// The pool keeps at least one connection alive to make sure that in memory databases are not dropped.
    pub(crate) fn connect(&self) -> rusqlite::Result<rusqlite::Connection> {
        let inner = rusqlite::Connection::open(&self.source)?;

        inner.pragma_update(None, "journal_mode", "WAL")?;
        inner.pragma_update(None, "synchronous", self.synchronous.as_str())?;
        inner.pragma_update(None, "foreign_keys", self.foreign_keys.as_str())?;
        inner.set_db_config(DbConfig::SQLITE_DBCONFIG_DQS_DDL, false)?;
        inner.set_db_config(DbConfig::SQLITE_DBCONFIG_DQS_DML, false)?;
        inner.set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)?;

        #[cfg(feature = "bundled")]
        inner.create_scalar_function(
            "floor",
            1,
            rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
            |ctx| {
                assert_eq!(ctx.len(), 1, "called with unexpected number of arguments");
                let res = ctx.get::<Option<f64>>(0)?.map(|x| x.floor());
                Ok(res)
            },
        )?;

        #[cfg(feature = "bundled")]
        inner.create_scalar_function(
            "ceil",
            1,
            rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
            |ctx| {
                assert_eq!(ctx.len(), 1, "called with unexpected number of arguments");
                let res = ctx.get::<Option<f64>>(0)?.map(|x| x.ceil());
                Ok(res)
            },
        )?;

        #[cfg(feature = "jiff-02")]
        inner.create_scalar_function(
            "timestamp_add_nanosecond",
            2,
            rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
            |ctx| {
                use crate::value::DbTyp;
                assert_eq!(ctx.len(), 2, "called with unexpected number of arguments");
                if matches!(ctx.get_raw(0), rusqlite::types::ValueRef::Null)
                    || matches!(ctx.get_raw(1), rusqlite::types::ValueRef::Null)
                {
                    return Ok(None);
                }

                let timestamp = jiff::Timestamp::from_sql(ctx.get_raw(0))?;
                let seconds = ctx.get::<i64>(1)?;
                let new = timestamp + jiff::SignedDuration::from_nanos(seconds);
                let rusqlite::types::Value::Text(res) = jiff::Timestamp::out_to_value(new) else {
                    unreachable!("func always returns some string")
                };
                Ok(Some(res))
            },
        )?;

        #[cfg(feature = "jiff-02")]
        inner.create_scalar_function(
            "timestamp_subsec_nanosecond",
            1,
            rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
            |ctx| {
                use crate::value::DbTyp;
                assert_eq!(ctx.len(), 1, "called with unexpected number of arguments");
                if matches!(ctx.get_raw(0), rusqlite::types::ValueRef::Null) {
                    return Ok(None);
                }

                let timestamp = jiff::Timestamp::from_sql(ctx.get_raw(0))?;
                Ok(Some(timestamp.subsec_nanosecond()))
            },
        )?;

        #[cfg(feature = "jiff-02")]
        inner.create_scalar_function(
            "timestamp_to_second",
            1,
            rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
            |ctx| {
                use crate::value::DbTyp;
                assert_eq!(ctx.len(), 1, "called with unexpected number of arguments");
                if matches!(ctx.get_raw(0), rusqlite::types::ValueRef::Null) {
                    return Ok(None);
                }

                let timestamp = jiff::Timestamp::from_sql(ctx.get_raw(0))?;
                Ok(Some(timestamp.as_second()))
            },
        )?;

        #[cfg(feature = "jiff-02")]
        inner.create_scalar_function(
            "timestamp_to_date",
            2,
            rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
            |ctx| {
                use jiff::fmt::temporal;

                use crate::value::DbTyp;
                assert_eq!(ctx.len(), 2, "called with unexpected number of arguments");
                if matches!(ctx.get_raw(0), rusqlite::types::ValueRef::Null)
                    || matches!(ctx.get_raw(1), rusqlite::types::ValueRef::Null)
                {
                    return Ok(None);
                }

                static PARSER: temporal::DateTimeParser = temporal::DateTimeParser::new();

                let timestamp = jiff::Timestamp::from_sql(ctx.get_raw(0))?;
                let timezone = PARSER
                    .parse_time_zone(ctx.get_raw(1).as_str()?)
                    .expect("time zone was serialized with jiff");
                let date = timezone.to_datetime(timestamp).date();
                let rusqlite::types::Value::Text(res) = jiff::civil::Date::out_to_value(date)
                else {
                    unreachable!("func always returns some string")
                };
                Ok(Some(res))
            },
        )?;

        Ok(inner)
    }
}
