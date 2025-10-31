use std::{
    cell::RefCell, convert::Infallible, iter::zip, marker::PhantomData, sync::atomic::AtomicI64,
};

use rusqlite::ErrorCode;
use sea_query::{
    Alias, CommonTableExpression, DeleteStatement, Expr, ExprTrait, InsertStatement, IntoTableRef,
    SelectStatement, SqliteQueryBuilder, UpdateStatement, WithClause,
};
use sea_query_rusqlite::RusqliteBinder;
use self_cell::{MutBorrow, self_cell};

use crate::{
    IntoExpr, IntoSelect, Table, TableRow,
    migrate::{Schema, check_schema, schema_version, user_version},
    private::Reader,
    query::{Query, track_stmt},
    rows::Rows,
    value::{DynTypedExpr, SecretFromSql, ValueBuilder},
    writable::TableInsert,
};

/// [Database] is a proof that the database has been configured.
///
/// Creating a [Database] requires going through the steps to migrate an existing database to
/// the required schema, or creating a new database from scratch (See also [crate::migration::Config]).
/// Please see [Database::migrator] to get started.
///
/// Having done the setup to create a compatible database is sadly not a guarantee that the
/// database will stay compatible for the lifetime of the [Database] struct.
/// That is why [Database] also stores the `schema_version`. This allows detecting non-malicious
/// modifications to the schema and gives us the ability to panic when this is detected.
/// Such non-malicious modification of the schema can happen for example if another [Database]
/// instance is created with additional migrations (e.g. by another newer instance of your program).
pub struct Database<S> {
    pub(crate) manager: r2d2_sqlite::SqliteConnectionManager,
    pub(crate) schema_version: AtomicI64,
    pub(crate) schema: PhantomData<S>,
    pub(crate) mut_lock: parking_lot::FairMutex<()>,
}

use rusqlite::Connection;
type RTransaction<'x> = Option<rusqlite::Transaction<'x>>;

self_cell!(
    pub struct OwnedTransaction {
        owner: MutBorrow<Connection>,

        #[covariant]
        dependent: RTransaction,
    }
);

/// SAFETY:
/// `RTransaction: !Send` because it borrows from `Connection` and `Connection: !Sync`.
/// `OwnedTransaction` can be `Send` because we know that `dependent` is the only
/// borrow of `owner` and `OwnedTransaction: !Sync` so `dependent` can not be borrowed
/// from multiple threads.
unsafe impl Send for OwnedTransaction {}

assert_not_impl_any! {OwnedTransaction: Sync}

thread_local! {
    pub(crate) static TXN: RefCell<Option<OwnedTransaction>> = const { RefCell::new(None) };
}

impl OwnedTransaction {
    pub fn get(&self) -> &rusqlite::Transaction<'_> {
        self.borrow_dependent().as_ref().unwrap()
    }

    pub fn with(mut self, f: impl FnOnce(rusqlite::Transaction<'_>)) {
        self.with_dependent_mut(|_, b| f(b.take().unwrap()))
    }
}

impl<S: Send + Sync + Schema> Database<S> {
    /// Create a [Transaction]. This operation always completes immediately as it does not need to wait on other transactions.
    ///
    /// This function will panic if the schema was modified compared to when the [Database] value
    /// was created. This can happen for example by running another instance of your program with
    /// additional migrations.
    pub fn transaction<R: Send>(&self, f: impl Send + FnOnce(&'static Transaction<S>) -> R) -> R {
        let res = std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    use r2d2::ManageConnection;

                    let conn = self.manager.connect().unwrap();
                    let owned = OwnedTransaction::new(MutBorrow::new(conn), |conn| {
                        Some(conn.borrow_mut().transaction().unwrap())
                    });

                    f(Transaction::new_checked(owned, &self.schema_version))
                })
                .join()
        });
        match res {
            Ok(val) => val,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    }

    /// Create a mutable [Transaction].
    /// This operation needs to wait for all other mutable [Transaction]s for this database to be finished.
    ///
    /// Note: you can create a deadlock if you are holding on to another lock while trying to
    /// get a mutable transaction!
    ///
    /// Whether the transaction is commited depends on the result of the closure.
    /// The transaction is only commited if the closure return [Ok]. In the case that it returns [Err]
    /// or when the closure panics, a rollback is performed.
    ///
    /// This function will panic if the schema was modified compared to when the [Database] value
    /// was created. This can happen for example by running another instance of your program with
    /// additional migrations.
    pub fn transaction_mut<O: Send, E: Send>(
        &self,
        f: impl Send + FnOnce(&'static mut Transaction<S>) -> Result<O, E>,
    ) -> Result<O, E> {
        use r2d2::ManageConnection;
        let conn = self.manager.connect().unwrap();

        // Acquire the lock just before creating the transaction
        let guard = self.mut_lock.lock();

        let owned = OwnedTransaction::new(MutBorrow::new(conn), |conn| {
            let txn = conn
                .borrow_mut()
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap();
            Some(txn)
        });
        let join_res = std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let res = f(Transaction::new_checked(owned, &self.schema_version));
                    let owned = TXN.take().unwrap();
                    (res, owned)
                })
                .join()
        });

        // Drop the guard before commiting to let sqlite go to the next transaction
        // more quickly while guaranteeing that the database will unlock soon.
        drop(guard);

        let (res, owned) = match join_res {
            Ok(val) => val,
            Err(payload) => std::panic::resume_unwind(payload),
        };

        if res.is_ok() {
            owned.with(|x| x.commit().unwrap());
        } else {
            owned.with(|x| x.rollback().unwrap());
        }
        res
    }

    /// Same as [Self::transaction_mut], but always commits the transaction.
    ///
    /// The only exception is that if the closure panics, a rollback is performed.
    pub fn transaction_mut_ok<R: Send>(
        &self,
        f: impl Send + FnOnce(&'static mut Transaction<S>) -> R,
    ) -> R {
        self.transaction_mut(|txn| Ok::<R, Infallible>(f(txn)))
            .unwrap()
    }

    /// Create a new [rusqlite::Connection] to the database.
    ///
    /// You can do (almost) anything you want with this connection as it is almost completely isolated from all other
    /// [rust_query] connections. The only thing you should not do here is changing the schema.
    /// Schema changes are detected with the `schema_version` pragma and will result in a panic when creating a new
    /// [rust_query] transaction.
    ///
    /// The `foreign_keys` pragma is always enabled here, even if [crate::migrate::ForeignKeys::SQLite] is not used.
    pub fn rusqlite_connection(&self) -> rusqlite::Connection {
        use r2d2::ManageConnection;
        let conn = self.manager.connect().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        conn
    }
}

/// [Transaction] can be used to query and update the database.
///
/// From the perspective of a [Transaction] each other [Transaction] is fully applied or not at all.
/// Futhermore, the effects of [Transaction]s have a global order.
/// So if we have mutations `A` and then `B`, it is impossible for a [Transaction] to see the effect of `B` without seeing the effect of `A`.
pub struct Transaction<S> {
    pub(crate) _p2: PhantomData<S>,
    pub(crate) _local: PhantomData<*const ()>,
}

impl<S> Transaction<S> {
    pub(crate) fn new() -> Self {
        Self {
            _p2: PhantomData,
            _local: PhantomData,
        }
    }
}

impl<S: Schema> Transaction<S> {
    /// This will check the schema version and panic if it is not as expected
    pub(crate) fn new_checked(txn: OwnedTransaction, expected: &AtomicI64) -> &'static mut Self {
        let schema_version = schema_version(txn.get());
        // If the schema version is not the expected version then we
        // check if the changes are acceptable.
        if schema_version != expected.load(std::sync::atomic::Ordering::Relaxed) {
            if user_version(txn.get()).unwrap() != S::VERSION {
                panic!("The database user_version changed unexpectedly")
            }

            TXN.set(Some(txn));
            check_schema::<S>();
            expected.store(schema_version, std::sync::atomic::Ordering::Relaxed);
        } else {
            TXN.set(Some(txn));
        }

        const {
            assert!(size_of::<Self>() == 0);
        }
        // no memory is leaked because Self is zero sized
        Box::leak(Box::new(Self::new()))
    }
}

impl<S> Transaction<S> {
    /// Execute a query with multiple results.
    ///
    /// ```
    /// # use rust_query::{private::doctest::*};
    /// # get_txn(|txn| {
    /// let user_names = txn.query(|rows| {
    ///     let user = rows.join(User);
    ///     rows.into_vec(&user.name)
    /// });
    /// assert_eq!(user_names, vec!["Alice".to_owned()]);
    /// # });
    /// ```
    pub fn query<F, R>(&self, f: F) -> R
    where
        F: for<'inner> FnOnce(&mut Query<'inner, S>) -> R,
    {
        // Execution already happens in a [Transaction].
        // and thus any [TransactionMut] that it might be borrowed
        // from is borrowed immutably, which means the rows can not change.

        TXN.with_borrow(|txn| {
            let conn = txn.as_ref().unwrap().get();
            let q = Rows {
                phantom: PhantomData,
                ast: Default::default(),
                _p: PhantomData,
            };
            f(&mut Query {
                q,
                phantom: PhantomData,
                conn,
            })
        })
    }

    /// Retrieve a single result from the database.
    ///
    /// ```
    /// # use rust_query::{private::doctest::*, IntoExpr};
    /// # rust_query::private::doctest::get_txn(|txn| {
    /// let res = txn.query_one("test".into_expr());
    /// assert_eq!(res, "test");
    /// # });
    /// ```
    ///
    /// Instead of using [Self::query_one] in a loop, it is better to
    /// call [Self::query] and return all results at once.
    pub fn query_one<O: 'static>(&self, val: impl IntoSelect<'static, S, Out = O>) -> O {
        self.query(|e| e.into_iter(val.into_select()).next().unwrap())
    }
}

impl<S: 'static> Transaction<S> {
    /// Try inserting a value into the database.
    ///
    /// Returns [Ok] with a reference to the new inserted value or an [Err] with conflict information.
    /// The type of conflict information depends on the number of unique constraints on the table:
    /// - 0 unique constraints => [Infallible]
    /// - 1 unique constraint => [Expr] reference to the conflicting table row.
    /// - 2+ unique constraints => `()` no further information is provided.
    ///
    /// ```
    /// # use rust_query::{private::doctest::*, IntoExpr};
    /// # rust_query::private::doctest::get_txn(|mut txn| {
    /// let res = txn.insert(User {
    ///     name: "Bob",
    /// });
    /// assert!(res.is_ok());
    /// let res = txn.insert(User {
    ///     name: "Bob",
    /// });
    /// assert!(res.is_err(), "there is a unique constraint on the name");
    /// # });
    /// ```
    pub fn insert<T: Table<Schema = S>>(
        &mut self,
        val: impl TableInsert<T = T>,
    ) -> Result<TableRow<T>, T::Conflict> {
        try_insert_private(T::NAME.into_table_ref(), None, val.into_insert())
    }

    /// This is a convenience function to make using [Transaction::insert]
    /// easier for tables without unique constraints.
    ///
    /// The new row is added to the table and the row reference is returned.
    pub fn insert_ok<T: Table<Schema = S, Conflict = Infallible>>(
        &mut self,
        val: impl TableInsert<T = T>,
    ) -> TableRow<T> {
        let Ok(row) = self.insert(val);
        row
    }

    /// This is a convenience function to make using [Transaction::insert]
    /// easier for tables with exactly one unique constraints.
    ///
    /// The new row is inserted and the reference to the row is returned OR
    /// an existing row is found which conflicts with the new row and a reference
    /// to the conflicting row is returned.
    ///
    /// ```
    /// # use rust_query::{private::doctest::*, IntoExpr};
    /// # rust_query::private::doctest::get_txn(|mut txn| {
    /// let bob = txn.insert(User {
    ///     name: "Bob",
    /// }).unwrap();
    /// let bob2 = txn.find_or_insert(User {
    ///     name: "Bob", // this will conflict with the existing row.
    /// });
    /// assert_eq!(bob, bob2);
    /// # });
    /// ```
    pub fn find_or_insert<T: Table<Schema = S, Conflict = TableRow<T>>>(
        &mut self,
        val: impl TableInsert<T = T>,
    ) -> TableRow<T> {
        match self.insert(val) {
            Ok(row) => row,
            Err(row) => row,
        }
    }

    /// Try updating a row in the database to have new column values.
    ///
    /// Updating can fail just like [Transaction::insert] because of unique constraint conflicts.
    /// This happens when the new values are in conflict with an existing different row.
    ///
    /// When the update succeeds, this function returns [Ok], when it fails it returns [Err] with one of
    /// three conflict types:
    /// - 0 unique constraints => [Infallible]
    /// - 1 unique constraint => [Expr] reference to the conflicting table row.
    /// - 2+ unique constraints => `()` no further information is provided.
    ///
    /// ```
    /// # use rust_query::{private::doctest::*, IntoExpr, Update};
    /// # rust_query::private::doctest::get_txn(|mut txn| {
    /// let bob = txn.insert(User {
    ///     name: "Bob",
    /// }).unwrap();
    /// txn.update(bob, User {
    ///     name: Update::set("New Bob"),
    /// }).unwrap();
    /// # });
    /// ```
    pub fn update<T: Table<Schema = S>>(
        &mut self,
        row: impl IntoExpr<'static, S, Typ = T>,
        val: T::Update,
    ) -> Result<(), T::Conflict> {
        let mut id = ValueBuilder::default();
        let row = row.into_expr();
        let (id, _) = id.simple_one(DynTypedExpr::erase(&row));

        let val = T::apply_try_update(val, row);
        let mut reader = Reader::default();
        T::read(&val, &mut reader);
        let (col_names, col_exprs): (Vec<_>, Vec<_>) = reader.builder.into_iter().collect();

        let (select, col_fields) = ValueBuilder::default().simple(col_exprs);
        let cte = CommonTableExpression::new()
            .query(select)
            .columns(col_fields.clone())
            .table_name(Alias::new("cte"))
            .to_owned();
        let with_clause = WithClause::new().cte(cte).to_owned();

        let mut update = UpdateStatement::new()
            .table(("main", T::NAME))
            .cond_where(Expr::col(("main", T::NAME, T::ID)).in_subquery(id))
            .to_owned();

        for (name, field) in zip(col_names, col_fields) {
            let select = SelectStatement::new()
                .from(Alias::new("cte"))
                .column(field)
                .to_owned();
            let value = sea_query::Expr::SubQuery(
                None,
                Box::new(sea_query::SubQueryStatement::SelectStatement(select)),
            );
            update.value(Alias::new(name), value);
        }

        let (query, args) = update.with(with_clause).build_rusqlite(SqliteQueryBuilder);

        TXN.with_borrow(|txn| {
            let txn = txn.as_ref().unwrap().get();

            let mut stmt = txn.prepare_cached(&query).unwrap();
            match stmt.execute(&*args.as_params()) {
                Ok(1) => Ok(()),
                Ok(n) => panic!("unexpected number of updates: {n}"),
                Err(rusqlite::Error::SqliteFailure(kind, Some(_val)))
                    if kind.code == ErrorCode::ConstraintViolation =>
                {
                    // val looks like "UNIQUE constraint failed: playlist_track.playlist, playlist_track.track"
                    Err(T::get_conflict_unchecked(self, &val))
                }
                Err(err) => panic!("{err:?}"),
            }
        })
    }

    /// This is a convenience function to use [Transaction::update] for updates
    /// that can not cause unique constraint violations.
    ///
    /// This method can be used for all tables, it just does not allow modifying
    /// columns that are part of unique constraints.
    pub fn update_ok<T: Table<Schema = S>>(
        &mut self,
        row: impl IntoExpr<'static, S, Typ = T>,
        val: T::UpdateOk,
    ) {
        match self.update(row, T::update_into_try_update(val)) {
            Ok(val) => val,
            Err(_) => {
                unreachable!("update can not fail")
            }
        }
    }

    /// Convert the [Transaction] into a [TransactionWeak] to allow deletions.
    pub fn downgrade(&'static mut self) -> &'static mut TransactionWeak<S> {
        // TODO: clean this up
        Box::leak(Box::new(TransactionWeak { inner: PhantomData }))
    }
}

/// This is the weak version of [Transaction].
///
/// The reason that it is called `weak` is because [TransactionWeak] can not guarantee
/// that [TableRow]s prove the existence of their particular row.
///
/// [TransactionWeak] is useful because it allowes deleting rows.
pub struct TransactionWeak<S> {
    inner: PhantomData<Transaction<S>>,
}

impl<S: Schema> TransactionWeak<S> {
    /// Try to delete a row from the database.
    ///
    /// This will return an [Err] if there is a row that references the row that is being deleted.
    /// When this method returns [Ok] it will contain a [bool] that is either
    /// - `true` if the row was just deleted.
    /// - `false` if the row was deleted previously in this transaction.
    pub fn delete<T: Table<Schema = S>>(&mut self, val: TableRow<T>) -> Result<bool, T::Referer> {
        let schema = crate::hash::Schema::new::<S>();

        // This is a manual check that foreign key constraints are not violated.
        // We do this manually because we don't want to enabled foreign key constraints for the whole
        // transaction (and is not possible to enable for part of a transaction).
        let mut checks = vec![];
        for (table_name, table) in &schema.tables {
            for col in table.columns.iter().filter_map(|(col_name, col)| {
                col.fk
                    .as_ref()
                    .is_some_and(|(t, c)| t == T::NAME && c == T::ID)
                    .then_some(col_name)
            }) {
                let stmt = SelectStatement::new()
                    .expr(
                        val.in_subquery(
                            SelectStatement::new()
                                .from(Alias::new(table_name))
                                .column(Alias::new(col))
                                .take(),
                        ),
                    )
                    .take();
                checks.push(stmt.build_rusqlite(SqliteQueryBuilder));
            }
        }

        let stmt = DeleteStatement::new()
            .from_table(("main", T::NAME))
            .cond_where(Expr::col(("main", T::NAME, T::ID)).eq(val.inner.idx))
            .take();

        let (query, args) = stmt.build_rusqlite(SqliteQueryBuilder);

        TXN.with_borrow(|txn| {
            let txn = txn.as_ref().unwrap().get();

            for (query, args) in checks {
                let mut stmt = txn.prepare_cached(&query).unwrap();
                match stmt.query_one(&*args.as_params(), |r| r.get(0)) {
                    Ok(true) => return Err(T::get_referer_unchecked()),
                    Ok(false) => {}
                    Err(err) => panic!("{err:?}"),
                }
            }

            let mut stmt = txn.prepare_cached(&query).unwrap();
            match stmt.execute(&*args.as_params()) {
                Ok(0) => Ok(false),
                Ok(1) => Ok(true),
                Ok(n) => {
                    panic!("unexpected number of deletes {n}")
                }
                Err(err) => panic!("{err:?}"),
            }
        })
    }

    /// Delete a row from the database.
    ///
    /// This is the infallible version of [TransactionWeak::delete].
    ///
    /// To be able to use this method you have to mark the table as `#[no_reference]` in the schema.
    pub fn delete_ok<T: Table<Referer = Infallible, Schema = S>>(
        &mut self,
        val: TableRow<T>,
    ) -> bool {
        let Ok(res) = self.delete(val);
        res
    }

    /// This allows you to do (almost) anything you want with the internal [rusqlite::Transaction].
    ///
    /// Note that there are some things that you should not do with the transaction, such as:
    /// - Changes to the schema, these will result in a panic as described in [Database].
    /// - Making changes that violate foreign-key constraints (see below).
    ///
    /// Sadly it is not possible to enable (or disable) the `foreign_keys` pragma during a transaction.
    /// This means that whether this pragma is enabled depends on which [crate::migrate::ForeignKeys]
    /// option is used and can not be changed.
    pub fn rusqlite_transaction<R>(&mut self, f: impl FnOnce(&rusqlite::Transaction) -> R) -> R {
        TXN.with_borrow(|txn| f(txn.as_ref().unwrap().get()))
    }
}

pub fn try_insert_private<T: Table>(
    table: sea_query::TableRef,
    idx: Option<i64>,
    val: T::Insert,
) -> Result<TableRow<T>, T::Conflict> {
    let mut reader = Reader::default();
    T::read(&val, &mut reader);
    if let Some(idx) = idx {
        reader.col(T::ID, idx);
    }
    let (col_names, col_exprs): (Vec<_>, Vec<_>) = reader.builder.into_iter().collect();
    let is_empty = col_names.is_empty();

    let (select, _) = ValueBuilder::default().simple(col_exprs);

    let mut insert = InsertStatement::new();
    insert.into_table(table);
    insert.columns(col_names.into_iter().map(Alias::new));
    if is_empty {
        // select always has at least one column, so we leave it out when there are no columns
        insert.or_default_values();
    } else {
        insert.select_from(select).unwrap();
    }
    insert.returning_col(T::ID);

    let (sql, values) = insert.build_rusqlite(SqliteQueryBuilder);

    TXN.with_borrow(|txn| {
        let txn = txn.as_ref().unwrap().get();
        track_stmt(txn, &sql, &values);

        let mut statement = txn.prepare_cached(&sql).unwrap();
        let mut res = statement
            .query_map(&*values.as_params(), |row| {
                Ok(TableRow::<T>::from_sql(row.get_ref(T::ID)?)?)
            })
            .unwrap();

        match res.next().unwrap() {
            Ok(id) => Ok(id),
            Err(rusqlite::Error::SqliteFailure(kind, Some(_val)))
                if kind.code == ErrorCode::ConstraintViolation =>
            {
                // val looks like "UNIQUE constraint failed: playlist_track.playlist, playlist_track.track"
                Err(T::get_conflict_unchecked(&Transaction::new(), &val))
            }
            Err(err) => panic!("{err:?}"),
        }
    })
}
