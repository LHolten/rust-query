use std::{marker::PhantomData, path::Path, rc::Rc, sync::atomic::AtomicBool};

use ouroboros::self_referencing;
use ref_cast::RefCast;
use rusqlite::{config::DbConfig, Connection};
use sea_query::{
    Alias, ColumnDef, InsertStatement, IntoTableRef, SqliteQueryBuilder, TableDropStatement,
    TableRenameStatement,
};
use sea_query_rusqlite::RusqliteBinder;

use crate::{
    alias::{Scope, TmpTable},
    ast::MySelect,
    dummy::{Cached, Cacher},
    hash,
    insert::Reader,
    pragma::read_schema,
    token::ThreadToken,
    transaction::Database,
    value, DynValue, Rows, Table, Transaction, Value,
};

pub type M<'a, From, To> = Box<
    dyn 'a
        + for<'t> FnOnce(
            [&'t &'a (); 0],
            ::rust_query::DynValue<'t, <From as Table>::Schema, From>,
        ) -> Box<dyn TableMigration<'t, 'a, From = From, To = To> + 't>,
>;

pub type C<'a, FromSchema, To> = Box<
    dyn 'a
        + for<'t> FnOnce(
            [&'t &'a (); 0],
            &mut Rows<'t, FromSchema>,
        )
            -> Box<dyn TableCreation<'t, 'a, FromSchema = FromSchema, To = To> + 't>,
>;

#[derive(Default)]
pub struct TableTypBuilder {
    pub(crate) ast: hash::Schema,
}

impl TableTypBuilder {
    pub fn table<T: Table>(&mut self) {
        let mut b = hash::TypBuilder::default();
        T::typs(&mut b);
        self.ast.tables.insert((T::NAME.to_owned(), b.ast));
    }
}

pub trait Schema: Sized + 'static {
    const VERSION: i64;
    fn new() -> Self;
    fn typs(b: &mut TableTypBuilder);
}

pub trait TableMigration<'t, 'a> {
    type From: Table;
    type To: Table;

    fn prepare(
        self: Box<Self>,
        prev: Cached<'t, Self::From>,
        cacher: Cacher<'_, 't, <Self::From as Table>::Schema>,
    ) -> Box<
        dyn FnMut(crate::private::Row<'_, 't, 'a>, Reader<'_, <Self::From as Table>::Schema>) + 't,
    >
    where
        'a: 't;
}

pub trait TableCreation<'t, 'a> {
    type FromSchema;
    type To: Table;

    fn prepare(
        self: Box<Self>,
        cacher: Cacher<'_, 't, Self::FromSchema>,
    ) -> Box<dyn FnMut(crate::private::Row<'_, 't, 'a>, Reader<'_, Self::FromSchema>) + 't>
    where
        'a: 't;
}

struct Wrapper<'t, 'a, From: Table, To>(
    Box<dyn TableMigration<'t, 'a, From = From, To = To> + 't>,
    DynValue<'t, From::Schema, From>,
);

impl<'t, 'a, From: Table, To: Table> TableCreation<'t, 'a> for Wrapper<'t, 'a, From, To> {
    type FromSchema = From::Schema;
    type To = To;

    fn prepare(
        self: Box<Self>,
        mut cacher: Cacher<'_, 't, Self::FromSchema>,
    ) -> Box<dyn FnMut(crate::private::Row<'_, 't, 'a>, Reader<'_, Self::FromSchema>) + 't>
    where
        'a: 't,
    {
        let db_id = cacher.cache(self.1);
        let mut prepared = Box::new(self.0).prepare(db_id, cacher);
        Box::new(move |row, reader| {
            // keep the ID the same
            reader.col(From::ID, row.get(db_id));
            prepared(row, reader);
        })
    }
}

impl<'inner, S> Rows<'inner, S> {
    fn cacher<'t>(&'_ self) -> Cacher<'_, 't, S> {
        Cacher {
            ast: &self.ast,
            _p: PhantomData,
        }
    }
}

pub struct SchemaBuilder<'a> {
    // this is used to create temporary table names
    scope: Scope,
    conn: &'a rusqlite::Transaction<'a>,
    drop: Vec<TableDropStatement>,
    rename: Vec<TableRenameStatement>,
}

impl<'a> SchemaBuilder<'a> {
    pub fn migrate_table<From: Table, To: Table>(&mut self, m: M<'a, From, To>) {
        self.create_inner::<From::Schema, To>(|[], rows| {
            let db_id = From::join(rows);
            let migration = m([], db_id.clone());
            Box::new(Wrapper(migration, db_id))
        });

        self.drop.push(
            sea_query::Table::drop()
                .table(Alias::new(From::NAME))
                .take(),
        );
    }

    pub fn create_from<FromSchema, To: Table>(&mut self, f: C<'a, FromSchema, To>) {
        self.create_inner::<FromSchema, To>(|[], x| f([], x));
    }

    fn create_inner<FromSchema, To: Table>(
        &mut self,
        f: impl for<'t> FnOnce(
            [&'t &'a (); 0],
            &mut Rows<'t, FromSchema>,
        )
            -> Box<dyn TableCreation<'t, 'a, FromSchema = FromSchema, To = To> + 't>,
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
        };
        let create = f([], &mut q);
        let mut prepared = create.prepare(q.cacher());

        let select = q.ast.simple();
        let (sql, values) = select.build_rusqlite(SqliteQueryBuilder);

        // no caching here, migration is only executed once
        let mut statement = self.conn.prepare(&sql).unwrap();
        let mut rows = statement.query(&*values.as_params()).unwrap();

        while let Some(row) = rows.next().unwrap() {
            let row = crate::private::Row {
                _p: PhantomData,
                _p2: PhantomData,
                row,
            };

            let new_ast = MySelect::default();
            let reader = Reader {
                ast: &new_ast,
                _p: PhantomData,
            };
            prepared(row, reader);

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

    pub fn new_table<T: Table>(&mut self) {
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

    fn tables(self, b: &mut SchemaBuilder<'a>);
}

/// [Prepare] is used to open a database from a file or in memory.
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
    pub(crate) transaction: rusqlite::Transaction<'this>,
}

impl Prepare {
    /// Open a database that is stored in a file.
    /// Creates the database if it does not exist.
    ///
    /// Opening the same database multiple times at the same time is fine,
    /// as long as they migrate to or use the same schema.
    /// All locking is done by sqlite, so connections can even be made using different client implementations.
    ///
    /// We currently don't check that the schema is not modified between transactions.
    /// So if that happens then the subsequent queries might fail.
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
    /// The sql code is executed after creating the empty database.
    /// Returns [None] if the database schema is older than `S`.
    /// This function will panic if the resulting schema is different, but the version matches.
    pub fn create_db_sql<S: Schema>(self, sql: &[&str]) -> Option<Migrator<S>> {
        self.migrator::<S>(|conn| {
            let mut b = TableTypBuilder::default();
            S::typs(&mut b);

            for (table_name, table) in &*b.ast.tables {
                new_table_inner(conn, table, Alias::new(table_name));
            }

            for sql in sql {
                conn.execute_batch(sql)
                    .expect("raw sql statement to initilize db failed");
            }
        })
    }

    /// Create empty tables based on the schema if the database was just created.
    /// Returns [None] if the database schema is older than `S`.
    /// This function will panic if the resulting schema is different, but the version matches.
    pub fn create_db_empty<S: Schema>(self) -> Option<Migrator<S>> {
        self.migrator::<S>(|conn| {
            let mut b = TableTypBuilder::default();
            S::typs(&mut b);

            for (table_name, table) in &*b.ast.tables {
                new_table_inner(conn, table, Alias::new(table_name));
            }
        })
    }

    fn migrator<S: Schema>(self, f: impl FnOnce(&rusqlite::Transaction)) -> Option<Migrator<S>> {
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
            set_user_version(conn, S::VERSION).unwrap();
        }

        let user_version = user_version(conn).unwrap();
        // We can not migrate databases older than `S`
        if user_version < S::VERSION {
            return None;
        } else if user_version == S::VERSION {
            foreign_key_check::<S>(conn);
        }

        Some(Migrator {
            manager: self.manager,
            transaction: Rc::new(owned),
            _p: PhantomData,
            _local: PhantomData,
        })
    }
}

/// [Migrator] is used to apply database migrations.
pub struct Migrator<S> {
    manager: r2d2_sqlite::SqliteConnectionManager,
    transaction: Rc<OwnedTransaction>,
    _p: PhantomData<S>,
    // We want to make sure that Migrator is always used with the same ThreadToken
    // so we make it local to the current thread.
    // This is mostly important because the thread token can have a reference to our transaction.
    _local: PhantomData<ThreadToken>,
}

impl<S: Schema> Migrator<S> {
    /// Apply a database migration if the current schema is `S`.
    /// The result is a migrator for the next schema `N`.
    /// This function will panic if the resulting schema is different, but the version matches.
    pub fn migrate<'a, F, M, N: Schema>(self, t: &'a mut ThreadToken, f: F) -> Migrator<N>
    where
        F: FnOnce(&'a Transaction<'a, S>) -> M,
        M: Migration<'a, From = S, To = N>,
    {
        t.stuff = self.transaction.clone();
        let conn = t
            .stuff
            .downcast_ref::<OwnedTransaction>()
            .unwrap()
            .borrow_transaction();

        if user_version(conn).unwrap() == S::VERSION {
            let client = Transaction::ref_cast(conn);

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
    /// Returns [None] if the database schema version is newer than `S`.
    pub fn finish(self, t: &mut ThreadToken) -> Option<Database<S>> {
        // make sure that t doesn't reference our transaction anymore
        t.stuff = Rc::new(());
        // we just erased the reference on the thread token, so we should have the only reference now.
        let mut transaction = Rc::into_inner(self.transaction).unwrap();

        let conn = transaction.borrow_transaction();
        if user_version(conn).unwrap() != S::VERSION {
            return None;
        }

        // Set transaction to commit now that we are happy with the schema.
        transaction.with_transaction_mut(|x| x.set_drop_behavior(rusqlite::DropBehavior::Commit));
        let heads = transaction.into_heads();
        heads
            .conn
            .pragma_update(None, "foreign_keys", "ON")
            .unwrap();

        Some(Database {
            manager: self.manager,
            schema: PhantomData,
        })
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
        read_schema(conn),
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
impl<S> Value<'_, S> for NoTable {
    type Owned = Self;

    fn into_owned(self) -> Self::Owned {
        self
    }
}
