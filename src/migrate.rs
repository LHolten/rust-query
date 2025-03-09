use std::{
    collections::HashMap, convert::Infallible, marker::PhantomData, ops::Deref, path::Path, rc::Rc,
    sync::atomic::AtomicBool,
};

use rusqlite::{Connection, config::DbConfig};
use sea_query::{
    Alias, ColumnDef, InsertStatement, IntoTableRef, SqliteQueryBuilder, TableDropStatement,
    TableRenameStatement,
};
use sea_query_rusqlite::RusqliteBinder;

use crate::{
    Expr, IntoExpr, IntoSelect, IntoSelectExt, Select, Table, TableRow, Transaction,
    alias::{Scope, TmpTable},
    ast::MySelect,
    client::LocalClient,
    dummy_impl::{Cacher, Prepared, Row, SelectImpl},
    hash,
    private::TableInsert,
    rows::Rows,
    schema_pragma::read_schema,
    transaction::{Database, try_insert_private},
    value::DynTypedExpr,
    writable::Reader,
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

pub struct CacheAndRead<'column, 'transaction, S> {
    cacher: Cacher,
    _p: PhantomData<fn(&'column ())>,
    _p3: PhantomData<S>,
    columns: Vec<(&'static str, DynPrepared<'transaction>)>,
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

impl<'t, 'a, S: 'static> CacheAndRead<'t, 'a, S> {
    pub fn col<O: 'a + IntoExpr<'a, S>>(
        &mut self,
        name: &'static str,
        val: impl IntoSelect<'t, 'a, S, Out = O>,
    ) {
        let mut p = val.into_select().inner.prepare(&mut self.cacher);
        let p = DynPrepared::new(move |row| p.call(row).into_expr().inner.erase());
        self.columns.push((name, p));
    }
}

pub trait TableMigration<'column, 't> {
    type From: Table;
    type To;

    fn prepare(
        self,
        prev: Expr<'column, <Self::From as Table>::Schema, Self::From>,
        cacher: &mut CacheAndRead<'column, 't, <Self::From as Table>::Schema>,
    );
}

pub trait TableCreation<'t> {
    type FromSchema;
    type Conflict;
    type T: Table<Conflict<'t> = Self::Conflict>;

    #[doc(hidden)]
    fn read(&self, f: Reader<'_, 't, Self::FromSchema>);
    #[doc(hidden)]
    fn get_conflict_unchecked(&self) -> Select<'t, 't, Self::FromSchema, Option<Self::Conflict>>;
}

pub struct Wrapper<X>(X);
impl<'t, X> TableInsert<'t> for Wrapper<X>
where
    X: TableCreation<'t>,
{
    type Schema = <X::T as Table>::Schema;
    type Conflict = X::Conflict;
    type T = X::T;

    fn read(&self, f: Reader<'_, 't, Self::Schema>) {
        self.0.read(Reader {
            ast: f.ast,
            _p: PhantomData,
            _p2: PhantomData,
        });
    }

    fn get_conflict_unchecked(&self) -> Select<'t, 't, Self::Schema, Option<Self::Conflict>> {
        let dummy = self.0.get_conflict_unchecked();
        Select {
            inner: dummy.inner,
            _p: PhantomData,
            _p2: PhantomData,
        }
    }
}

pub struct SchemaBuilder<'t> {
    // this is used to create temporary table names
    scope: Scope,
    conn: Rc<rusqlite::Transaction<'t>>,
    drop: Vec<TableDropStatement>,
    rename: Vec<TableRenameStatement>,
    foreign_key: HashMap<&'static str, Box<dyn FnOnce() -> Infallible>>,
    _p: PhantomData<fn(&'t ()) -> &'t ()>,
}

impl<'t> SchemaBuilder<'t> {
    pub fn migrate_table<M>(
        &mut self,
        m: Box<dyn 't + FnOnce(::rust_query::Expr<'t, <M::From as Table>::Schema, M::From>) -> M>,
    ) where
        M: TableMigration<'t, 't, To: Table>,
    {
        let new_table_name = self.create_empty_inner::<M::To>();

        let mut q = Rows::<<M::From as Table>::Schema> {
            phantom: PhantomData,
            ast: MySelect::default(),
            _p: PhantomData,
        };

        let db_id = M::From::join(&mut q);
        let migration = m(db_id.clone());

        let mut cache_and_read = CacheAndRead {
            columns: Vec::new(),
            cacher: Cacher::new(),
            _p: PhantomData,
            _p3: PhantomData,
        };

        // keep the ID the same
        cache_and_read.col(M::From::ID, db_id.clone());
        migration.prepare(db_id, &mut cache_and_read);

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
            let reader = Reader::<<M::From as Table>::Schema> {
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

        self.drop.push(
            sea_query::Table::drop()
                .table(Alias::new(M::From::NAME))
                .take(),
        );
    }

    pub fn get_migrate_list<'x, From: Table, To: Table>(&mut self) -> Vec<Entry<'x, 't, From, To>> {
        let name = self.create_empty_inner::<To>();
        let raw_txn = self.conn.clone();
        Transaction::new(self.conn.clone()).query(move |rows| {
            let x = From::join(rows);
            rows.into_vec(x.map_select(move |row| Entry {
                _p: PhantomData,
                row,
                txn: raw_txn.clone(),
                table: name,
            }))
        })
    }

    pub fn foreign_key<To: Table>(&mut self, err: Box<dyn FnOnce() -> Infallible>) {
        self.foreign_key.insert(To::NAME, err);
    }

    pub fn create_empty<To: Table>(&mut self) {
        self.create_empty_inner::<To>();
    }

    fn create_empty_inner<To: Table>(&mut self) -> TmpTable {
        let new_table_name = self.scope.tmp_table();
        new_table::<To>(&self.conn, new_table_name);

        self.rename.push(
            sea_query::Table::rename()
                .table(new_table_name, Alias::new(To::NAME))
                .take(),
        );
        new_table_name
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
    type Args<'x>;

    fn tables(self, b: &mut SchemaBuilder<'a>);
    fn new_tables<'x>(b: &mut SchemaBuilder<'a>) -> Self::Args<'x>;
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
        let conn = Rc::new(conn);

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
            assert_eq!(
                foreign_key_check::<S>(&conn),
                None,
                "foreign key constraint violated"
            );
        }

        Some(Migrator {
            manager: config.manager,
            transaction: conn,
            _p: PhantomData,
            _local: PhantomData,
            _p0: PhantomData,
        })
    }
}

/// [Migrator] is used to apply database migrations.
///
/// When all migrations are done, it can be turned into a [Database] instance with
/// [Migrator::finish].
pub struct Migrator<'t, S> {
    manager: r2d2_sqlite::SqliteConnectionManager,
    transaction: Rc<rusqlite::Transaction<'t>>,
    _p0: PhantomData<fn(&'t ()) -> &'t ()>,
    _p: PhantomData<S>,
    // We want to make sure that Migrator is always used with the same LocalClient
    // so we make it local to the current thread.
    // This is mostly important because the LocalClient can have a reference to our transaction.
    _local: PhantomData<LocalClient>,
}

pub struct Entry<'x, 't, OldT, T> {
    _p: PhantomData<(fn(&'t ()) -> &'t (), T, &'x ())>,
    row: TableRow<'t, OldT>,
    txn: Rc<rusqlite::Transaction<'t>>,
    table: TmpTable,
}

impl<'t, OldT, T> Deref for Entry<'_, 't, OldT, T> {
    type Target = TableRow<'t, OldT>;

    fn deref(&self) -> &Self::Target {
        &self.row
    }
}

impl<'t, OldT: Table, T: Table> Entry<'_, 't, OldT, T> {
    pub fn try_insert(
        self,
        val: impl TableCreation<'t, FromSchema = OldT::Schema, T = T, Conflict = T::Conflict<'t>>,
    ) -> Result<TableRow<'t, T>, T::Conflict<'t>> {
        // TODO: preserve the row id here
        try_insert_private(&self.txn, self.table.into_table_ref(), Wrapper(val))
    }
}

impl<'t, S: Schema> Migrator<'t, S> {
    /// Apply a database migration if the current schema is `S` and return a [Migrator] for the next schema `N`.
    ///
    /// This function will panic if the schema on disk does not match what is expected for its `user_version`.
    pub fn migrate<New: Schema, M>(
        self,
        m: impl FnOnce(&Transaction<'t, S>, M::Args<'_>) -> M,
    ) -> Migrator<'t, New>
    where
        M: Migration<'t, From = S, To = New>,
    {
        if user_version(&self.transaction).unwrap() == S::VERSION {
            let mut builder = SchemaBuilder {
                scope: Default::default(),
                conn: self.transaction.clone(),
                drop: vec![],
                rename: vec![],
                foreign_key: HashMap::new(),
                _p: PhantomData,
            };

            let args = M::new_tables(&mut builder);

            let m = m(&Transaction::new(self.transaction.clone()), args);

            m.tables(&mut builder);

            for drop in builder.drop {
                let sql = drop.to_string(SqliteQueryBuilder);
                self.transaction.execute(&sql, []).unwrap();
            }
            for rename in builder.rename {
                let sql = rename.to_string(SqliteQueryBuilder);
                self.transaction.execute(&sql, []).unwrap();
            }
            if let Some(fk) = foreign_key_check::<New>(&self.transaction) {
                (builder.foreign_key.remove(&*fk).unwrap())();
            }
            set_user_version(&self.transaction, New::VERSION).unwrap();
        }

        Migrator {
            manager: self.manager,
            transaction: self.transaction,
            _p: PhantomData,
            _local: PhantomData,
            _p0: PhantomData,
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
        Rc::into_inner(self.transaction).unwrap().commit().unwrap();

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

fn foreign_key_check<S: Schema>(conn: &Rc<rusqlite::Transaction>) -> Option<String> {
    let error = conn
        .prepare("PRAGMA foreign_key_check")
        .unwrap()
        .query_map([], |row| row.get(2))
        .unwrap()
        .next();
    if let Some(error) = error {
        return Some(error.unwrap());
    }

    let mut b = TableTypBuilder::default();
    S::typs(&mut b);
    pretty_assertions::assert_eq!(
        b.ast,
        read_schema(&crate::Transaction::new(conn.clone())),
        "schema is different (expected left, but got right)",
    );
    return None;
}
