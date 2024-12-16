use std::{convert::Infallible, marker::PhantomData, ops::Deref};

use rusqlite::ErrorCode;
use sea_query::{
    Alias, DeleteStatement, Expr, InsertStatement, SqliteQueryBuilder, UpdateStatement, Value,
};
use sea_query_rusqlite::RusqliteBinder;
use yoke::{Yoke, Yokeable};

use crate::{
    alias::Field,
    ast::MySelect,
    client::private_exec,
    exec::Query,
    insert::{Reader, Writable},
    migrate::schema_version,
    private::Dummy,
    token::LocalClient,
    value::MyTyp,
    IntoColumn, Table, TableRow,
};

/// [Database] is a proof that the database has been configured.
///
/// For information on how to create transactions, please refer to [LocalClient].
///
/// Creating a [Database] requires going through the steps to migrate an existing database to
/// the required schema, or creating a new database from scratch.
/// Having done the setup to create a compatible database is sadly not a guarantee that the
/// database will stay compatible for the lifetime of the [Database].
///
/// That is why [Database] also stores the `schema_version`. This allows detecting non-malicious
/// modifications to the schema and gives us the ability to panic when this is detected.
/// Such non-malicious modification of the schema can happen for example if another [Database]
/// instance is created with additional migrations (e.g. by another newer instance of your program).
///
/// # Sqlite config
///
/// Sqlite is configured to be in [WAL mode](https://www.sqlite.org/wal.html).
/// The effect of this mode is that there can be any number of readers with one concurrent writer.
/// What is nice about this is that a [Transaction] can always be made immediately.
/// Making a [TransactionMut] has to wait until all other [TransactionMut]s are finished.
///
/// Sqlite is also configured with [`synchronous=NORMAL`](https://www.sqlite.org/pragma.html#pragma_synchronous). This gives better performance by fsyncing less.
/// The database will not lose transactions due to application crashes, but it might due to system crashes or power loss.
pub struct Database<S> {
    pub(crate) manager: r2d2_sqlite::SqliteConnectionManager,
    pub(crate) schema_version: i64,
    pub(crate) schema: PhantomData<S>,
}

/// [Transaction] can be used to query the database.
///
/// From the perspective of a [Transaction] each [TransactionMut] is fully applied or not at all.
/// Futhermore, the effects of [TransactionMut]s have a global order.
/// So if we have mutations `A` and then `B`, it is impossible for a [Transaction] to see the effect of `B` without seeing the effect of `A`.
///
/// All [TableRow] references retrieved from the database live for at most `'a`.
/// This makes these references effectively local to this [Transaction].
#[repr(transparent)]
pub struct Transaction<'a, S> {
    pub(crate) transaction: YokedTransaction,
    pub(crate) _p: PhantomData<fn(&'a S) -> &'a S>,
    pub(crate) _local: PhantomData<LocalClient>,
}

/// Same as [Transaction], but allows inserting new rows.
///
/// [TransactionMut] always uses the latest version of the database, with the effects of all previous [TransactionMut]s applied.
///
/// To make mutations to the database permanent you need to use [TransactionMut::commit].
/// This is to make sure that if a function panics while holding a mutable transaction, it will roll back those changes.
pub struct TransactionMut<'a, S> {
    pub(crate) inner: Transaction<'a, S>,
}

impl<'a, S> Deref for TransactionMut<'a, S> {
    type Target = Transaction<'a, S>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'t, S> Transaction<'t, S> {
    pub(crate) fn transaction(&self) -> &rusqlite::Transaction {
        &self.transaction.get().0
    }

    /// This will check the schema version and panic if it is not as expected
    pub(crate) fn new_checked(txn: YokedTransaction, expected: i64) -> Self {
        if schema_version(&txn.get().0) != expected {
            panic!("The database schema was updated unexpectedly")
        }

        Transaction {
            transaction: txn,
            _p: PhantomData,
            _local: PhantomData,
        }
    }

    /// Execute a query with multiple results.
    ///
    /// Please take a look at the documentation of [Query] for how to use it.
    pub fn query<F, R>(&self, f: F) -> R
    where
        F: for<'a> FnOnce(&'a mut Query<'t, 'a, S>) -> R,
    {
        // Execution already happens in a [Transaction].
        // and thus any [TransactionMut] that it might be borrowed
        // from are borrowed immutably, so the rows can not change.
        private_exec(&self.transaction(), f)
    }

    /// Retrieve a single result from the database.
    ///
    /// Instead of using [Self::query_one] in a loop, it is better to
    /// call [Self::query] and return all results at once.
    pub fn query_one<O>(&self, val: impl Dummy<'t, 't, S, Out = O>) -> O
    where
        S: 'static,
    {
        // Theoretically this doesn't even need to be in a transaction.
        // We already have one though, so we must use it.
        let mut res = private_exec(&self.transaction(), |e| {
            // Cast the static lifetime to any lifetime necessary, this is fine because we know the static lifetime
            // can not be guaranteed by a query scope.
            e.into_vec_private(val)
        });
        res.pop().unwrap()
    }
}

impl<'t, S: 'static> TransactionMut<'t, S> {
    /// Try inserting a value into the database.
    ///
    /// Returns [Ok] with a reference to the new inserted value or an [Err] with conflict information.
    /// The type of conflict information depends on the number of unique constraints on the table:
    /// - 0 unique constraints => [Infallible]
    /// - 1 unique constraint => [TableRow] reference to the conflicting table row.
    /// - 2+ unique constraints => [()] no further information is provided.
    pub fn try_insert<T: Table<Schema = S>, C>(
        &mut self,
        val: impl Writable<'t, T = T, Conflict = C, Schema = S>,
    ) -> Result<TableRow<'t, T>, C> {
        let ast = MySelect::default();

        let reader = Reader {
            ast: &ast,
            _p: PhantomData,
            _p2: PhantomData,
        };
        val.read(reader);

        let select = ast.simple();

        let mut insert = InsertStatement::new();
        let names = ast.select.iter().map(|(_field, name)| *name);
        insert.into_table(Alias::new(T::NAME));
        insert.columns(names);
        insert.select_from(select).unwrap();
        insert.returning_col(Alias::new(T::ID));

        let (sql, values) = insert.build_rusqlite(SqliteQueryBuilder);

        let mut statement = self.transaction().prepare_cached(&sql).unwrap();
        let mut res = statement
            .query_map(&*values.as_params(), |row| {
                Ok(<T as MyTyp>::from_sql(row.get_ref(T::ID)?)?)
            })
            .unwrap();

        match res.next().unwrap() {
            Ok(id) => Ok(id),
            Err(rusqlite::Error::SqliteFailure(kind, Some(_val)))
                if kind.code == ErrorCode::ConstraintViolation =>
            {
                // val looks like "UNIQUE constraint failed: playlist_track.playlist, playlist_track.track"
                let conflict = self.query_one(val.get_conflict_unchecked());
                Err(conflict.unwrap())
            }
            Err(err) => Err(err).unwrap(),
        }
    }

    /// This is a convenience function to make using [TransactionMut::try_insert]
    /// easier for tables without unique constraints.
    ///
    /// The new row is added to the table and the row reference is returned.
    pub fn insert<T: Table<Schema = S>>(
        &mut self,
        val: impl Writable<'t, T = T, Conflict = Infallible, Schema = S>,
    ) -> TableRow<'t, T> {
        let Ok(row) = self.try_insert(val);
        row
    }

    /// This is a convenience function to make using [TransactionMut::try_insert]
    /// easier for tables with exactly one unique constraints.
    ///
    /// The new row is inserted and the reference to the row is returned OR
    /// an existing row is found which conflicts with the new row and a reference
    /// to the conflicting row is returned.
    pub fn find_or_insert<T: Table<Schema = S>>(
        &mut self,
        val: impl Writable<'t, T = T, Conflict = TableRow<'t, T>, Schema = S>,
    ) -> TableRow<'t, T> {
        match self.try_insert(val) {
            Ok(row) => row,
            Err(row) => row,
        }
    }

    /// Try updating a row in the database to have new column values.
    ///
    /// Updating can fail just like [TransactionMut::try_insert] because of unique constraint conflicts.
    /// This happens when the new values are in conflict with an existing different row.
    ///
    /// When the update succeeds, this function returns [Ok<()>], when it fails it returns [Err] with one of
    /// three conflict types:
    /// - 0 unique constraints => [Infallible]
    /// - 1 unique constraint => [TableRow] reference to the conflicting table row.
    /// - 2+ unique constraints => [()] no further information is provided.
    pub fn try_update<T: Table<Schema = S>, C>(
        &mut self,
        row: impl IntoColumn<'t, S, Typ = T>,
        val: impl Writable<'t, T = T, Conflict = C, Schema = S>,
    ) -> Result<(), C> {
        let ast = MySelect::default();

        let reader = Reader {
            ast: &ast,
            _p: PhantomData,
            _p2: PhantomData,
        };
        val.read(reader);

        let select = ast.simple();
        let (query, args) = select.build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.transaction().prepare_cached(&query).unwrap();

        let row_id = self.query_one(row).idx;
        let mut update = UpdateStatement::new()
            .table(Alias::new(T::NAME))
            .cond_where(Expr::val(row_id).equals(Alias::new(T::ID)))
            .to_owned();

        stmt.query_row(&*args.as_params(), |row| {
            for (_, field) in ast.select.iter() {
                let Field::Str(name) = field else { panic!() };

                let val = match row.get_unwrap::<&str, rusqlite::types::Value>(*name) {
                    rusqlite::types::Value::Null => Value::BigInt(None),
                    rusqlite::types::Value::Integer(x) => Value::BigInt(Some(x)),
                    rusqlite::types::Value::Real(x) => Value::Double(Some(x)),
                    rusqlite::types::Value::Text(x) => Value::String(Some(Box::new(x))),
                    rusqlite::types::Value::Blob(_) => todo!(),
                };
                update.value(*field, Expr::val(val));
            }
            Ok(())
        })
        .unwrap();

        let (query, args) = update.build_rusqlite(SqliteQueryBuilder);

        let mut stmt = self.transaction().prepare_cached(&query).unwrap();
        match stmt.execute(&*args.as_params()) {
            Ok(1) => Ok(()),
            Ok(n) => panic!("unexpected number of updates: {n}"),
            Err(rusqlite::Error::SqliteFailure(kind, Some(_val)))
                if kind.code == ErrorCode::ConstraintViolation =>
            {
                // val looks like "UNIQUE constraint failed: playlist_track.playlist, playlist_track.track"
                let conflict = self.query_one(val.get_conflict_unchecked());
                Err(conflict.unwrap())
            }
            Err(err) => Err(err).unwrap(),
        }
    }

    /// This is a convenience function to use [TransactionMut::try_update] on tables without
    /// unique constraints.
    pub fn update<T: Table<Schema = S>>(
        &mut self,
        row: impl IntoColumn<'t, S, Typ = T>,
        val: impl Writable<'t, T = T, Conflict = Infallible, Schema = S>,
    ) {
        let Ok(()) = self.try_update(row, val);
    }

    /// This is a convenience function to use [TransactionMut::try_update] on tables with
    /// exactly one unique constraint.
    ///
    /// This function works slightly different in that it does not receive a row reference.
    /// Instead it tries to update the row that would conflict if the new row would be inserted.
    /// When such a conflicting row is found, it is updated to the new column values and [Ok] is
    /// returned with a reference to the found row.
    /// If it can not find a conflicting row, then nothing happens and the function returns [Err]
    pub fn find_and_update<T: Table<Schema = S>>(
        &mut self,
        val: impl Writable<'t, T = T, Conflict = TableRow<'t, T>, Schema = S>,
    ) -> Result<TableRow<'t, T>, ()> {
        match self.query_one(val.get_conflict_unchecked()) {
            Some(row) => {
                self.try_update(row, val).unwrap();
                Ok(row)
            }
            None => Err(()),
        }
    }

    /// Make the changes made in this [TransactionMut] permanent.
    ///
    /// If the [TransactionMut] is dropped without calling this function, then the changes are rolled back.
    pub fn commit(self) {
        self.deletor().commit();
    }

    pub fn deletor(self) -> Deletor<S> {
        Deletor {
            transaction: self.inner.transaction,
            _p: PhantomData,
        }
    }
}

#[derive(Yokeable)]
pub struct TransactionYoke<'a>(pub rusqlite::Transaction<'a>);

type YokedTransaction = yoke::Yoke<TransactionYoke<'static>, Box<rusqlite::Connection>>;

pub struct Deletor<S> {
    transaction: YokedTransaction,
    _p: PhantomData<S>,
}

impl<S> Deletor<S> {
    pub fn try_delete<T: Table>(&mut self, val: TableRow<'_, T>) -> Result<bool, T::Referer> {
        let stmt = DeleteStatement::new()
            .from_table(Alias::new(T::NAME))
            .cond_where(Expr::col(Alias::new(T::ID)).eq(val.idx))
            .to_owned();

        let (query, args) = stmt.build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.transaction.get().0.prepare_cached(&query).unwrap();

        match stmt.execute(&*args.as_params()) {
            Ok(0) => Ok(false),
            Ok(1) => Ok(true),
            Ok(n) => {
                panic!("unexpected number of deletes {n}")
            }
            Err(rusqlite::Error::SqliteFailure(kind, Some(_val)))
                if kind.code == ErrorCode::ConstraintViolation =>
            {
                // Some foreign key constraint got violated
                Err(T::get_referer_unchecked())
            }
            Err(err) => Err(err).unwrap(),
        }
    }

    /// This allows you to do anything you want with the internal [rusqlite::Transaction]
    ///
    /// **Warning:** [Transaction::unchecked_transaction] makes it possible to break the
    /// invariants that [rust_query] relies on to avoid panics at run-time. It should
    /// therefore be avoided whenever possible.
    ///
    /// The specific version of rusqlite used is not stable. This means the [rusqlite]
    /// version might change as part of a non breaking version update of [rust_query].
    #[cfg(feature = "unchecked_transaction")]
    pub fn unchecked_transaction(&mut self) -> &rusqlite::Transaction {
        &self.transaction.get().0
    }

    /// Make the changes made in this [TransactionMut] permanent.
    ///
    /// If the [Deletor] is dropped without calling this function, then the changes are rolled back.
    pub fn commit(self) {
        let _: Yoke<(), Box<rusqlite::Connection>> = self
            .transaction
            .map_project(|transaction, _| transaction.0.commit().unwrap());
    }
}
