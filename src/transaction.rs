use std::{convert::Infallible, marker::PhantomData, ops::Deref, rc::Rc};

use rusqlite::ErrorCode;
use sea_query::{
    Alias, CommonTableExpression, DeleteStatement, Expr, InsertStatement, IntoTableRef,
    SelectStatement, SimpleExpr, SqliteQueryBuilder, UpdateStatement, WithClause,
};
use sea_query_rusqlite::RusqliteBinder;

use crate::{
    IntoExpr, IntoSelect, Table, TableRow, ast::MySelect, client::LocalClient,
    migrate::schema_version, private::Reader, query::Query, rows::Rows, value::SecretFromSql,
    writable::TableInsert,
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

impl<S> Database<S> {
    pub fn rusqlite_connection(&self) -> rusqlite::Connection {
        use r2d2::ManageConnection;
        self.manager.connect().unwrap()
    }
}

/// [Transaction] can be used to query the database.
///
/// From the perspective of a [Transaction] each [TransactionMut] is fully applied or not at all.
/// Futhermore, the effects of [TransactionMut]s have a global order.
/// So if we have mutations `A` and then `B`, it is impossible for a [Transaction] to see the effect of `B` without seeing the effect of `A`.
///
/// All [TableRow] references retrieved from the database live for at most `'a`.
/// This makes these references effectively local to this [Transaction].
pub struct Transaction<'t, S> {
    pub(crate) transaction: Rc<rusqlite::Transaction<'t>>,
    pub(crate) _p: PhantomData<fn(&'t ()) -> &'t ()>,
    pub(crate) _p2: PhantomData<S>,
    pub(crate) _local: PhantomData<LocalClient>,
}

impl<'t, S> Transaction<'t, S> {
    pub(crate) fn new(raw: Rc<rusqlite::Transaction<'t>>) -> Self {
        Self {
            transaction: raw,
            _p: PhantomData,
            _p2: PhantomData,
            _local: PhantomData,
        }
    }
}

/// Same as [Transaction], but allows inserting new rows.
///
/// [TransactionMut] always uses the latest version of the database, with the effects of all previous [TransactionMut]s applied.
///
/// To make mutations to the database permanent you need to use [TransactionMut::commit].
/// This is to make sure that if a function panics while holding a mutable transaction, it will roll back those changes.
pub struct TransactionMut<'t, S> {
    pub(crate) inner: Transaction<'t, S>,
}

impl<'t, S> Deref for TransactionMut<'t, S> {
    type Target = Transaction<'t, S>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'t, S> Transaction<'t, S> {
    /// This will check the schema version and panic if it is not as expected
    pub(crate) fn new_checked(txn: rusqlite::Transaction<'t>, expected: i64) -> Self {
        if schema_version(&txn) != expected {
            panic!("The database schema was updated unexpectedly")
        }

        Self::new(Rc::new(txn))
    }

    /// Execute a query with multiple results.
    ///
    /// Please take a look at the documentation of [Query] for how to use it.
    pub fn query<F, R>(&self, f: F) -> R
    where
        F: for<'inner> FnOnce(&mut Query<'t, 'inner, S>) -> R,
    {
        // Execution already happens in a [Transaction].
        // and thus any [TransactionMut] that it might be borrowed
        // from is borrowed immutably, which means the rows can not change.
        let conn: &rusqlite::Connection = &self.transaction;
        let ast = MySelect::default();
        let q = Rows {
            phantom: PhantomData,
            ast,
            _p: PhantomData,
        };
        f(&mut Query {
            q,
            phantom: PhantomData,
            conn,
        })
    }

    /// Retrieve a single result from the database.
    ///
    /// Instead of using [Self::query_one] in a loop, it is better to
    /// call [Self::query] and return all results at once.
    pub fn query_one<'e, O>(&self, val: impl IntoSelect<'t, 't, S, Out = O>) -> O {
        // Theoretically this doesn't even need to be in a transaction.
        // We already have one though, so we must use it.
        let mut res = self.query(|e| {
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
    /// - 2+ unique constraints => `()` no further information is provided.
    pub fn insert<T: Table<Schema = S>>(
        &mut self,
        val: impl TableInsert<'t, T = T>,
    ) -> Result<TableRow<'t, T>, T::Conflict<'t>> {
        try_insert_private(
            &self.transaction,
            Alias::new(T::NAME).into_table_ref(),
            None,
            val.into_insert(),
        )
    }

    /// This is a convenience function to make using [TransactionMut::insert]
    /// easier for tables without unique constraints.
    ///
    /// The new row is added to the table and the row reference is returned.
    pub fn insert_ok<T: Table<Schema = S, Conflict<'t> = Infallible>>(
        &mut self,
        val: impl TableInsert<'t, T = T>,
    ) -> TableRow<'t, T> {
        let Ok(row) = self.insert(val);
        row
    }

    /// This is a convenience function to make using [TransactionMut::insert]
    /// easier for tables with exactly one unique constraints.
    ///
    /// The new row is inserted and the reference to the row is returned OR
    /// an existing row is found which conflicts with the new row and a reference
    /// to the conflicting row is returned.
    pub fn find_or_insert<T: Table<Schema = S, Conflict<'t> = TableRow<'t, T>>>(
        &mut self,
        val: impl TableInsert<'t, T = T>,
    ) -> TableRow<'t, T> {
        match self.insert(val) {
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
    /// - 2+ unique constraints => `()` no further information is provided.
    pub fn try_update<T: Table<Schema = S>>(
        &mut self,
        row: impl IntoExpr<'t, S, Typ = T>,
        val: T::TryUpdate<'t>,
    ) -> Result<(), T::Conflict<'t>> {
        let id = MySelect::default();
        Reader::new(&id).col(T::ID, &row);
        let id = id.build_select(false);

        let val = T::apply_try_update(val, row.into_expr());
        let ast = MySelect::default();
        T::read(&val, Reader::new(&ast));

        let select = ast.build_select(false);
        let cte = CommonTableExpression::new()
            .query(select)
            .columns(ast.select.iter().map(|x| x.1))
            .table_name(Alias::new("cte"))
            .to_owned();
        let with_clause = WithClause::new().cte(cte).to_owned();

        let mut update = UpdateStatement::new()
            .table(Alias::new(T::NAME))
            .cond_where(Expr::col(Alias::new(T::ID)).in_subquery(id))
            .to_owned();

        for (_, col) in ast.select.iter() {
            let select = SelectStatement::new()
                .from(Alias::new("cte"))
                .column(*col)
                .to_owned();
            let value = SimpleExpr::SubQuery(
                None,
                Box::new(sea_query::SubQueryStatement::SelectStatement(select)),
            );
            update.value(*col, value);
        }

        let (query, args) = update.with(with_clause).build_rusqlite(SqliteQueryBuilder);

        let mut stmt = self.transaction.prepare_cached(&query).unwrap();
        match stmt.execute(&*args.as_params()) {
            Ok(1) => Ok(()),
            Ok(n) => panic!("unexpected number of updates: {n}"),
            Err(rusqlite::Error::SqliteFailure(kind, Some(_val)))
                if kind.code == ErrorCode::ConstraintViolation =>
            {
                // val looks like "UNIQUE constraint failed: playlist_track.playlist, playlist_track.track"
                let conflict = self.query_one(T::get_conflict_unchecked(&val));
                Err(conflict.unwrap())
            }
            Err(err) => Err(err).unwrap(),
        }
    }

    /// This is a convenience function to use [TransactionMut::try_update] for updates
    /// that can not cause unique constraint violations.
    ///
    /// This method can be used for all tables, it just does not allow modifying
    /// columns that are part of unique constraints.
    pub fn update<T: Table<Schema = S>>(
        &mut self,
        row: impl IntoExpr<'t, S, Typ = T>,
        val: T::Update<'t>,
    ) {
        match self.try_update(row, T::update_into_try_update(val)) {
            Ok(val) => val,
            Err(_) => {
                unreachable!("update can not fail")
            }
        }
    }

    /// Make the changes made in this [TransactionMut] permanent.
    ///
    /// If the [TransactionMut] is dropped without calling this function, then the changes are rolled back.
    pub fn commit(self) {
        Rc::into_inner(self.inner.transaction)
            .unwrap()
            .commit()
            .unwrap();
    }

    pub fn downgrade(self) -> TransactionWeak<'t, S> {
        TransactionWeak { inner: self }
    }
}

/// This is the weak version of [TransactionMut].
///
/// The reason that it is called `weak` is because [TransactionWeak] can not guarantee
/// that [TableRow]s prove the existence of their particular row.
///
/// [TransactionWeak] is useful because it allowes deleting rows.
pub struct TransactionWeak<'t, S> {
    inner: TransactionMut<'t, S>,
}

impl<'t, S: 'static> TransactionWeak<'t, S> {
    /// Try to delete a row from the database.
    ///
    /// This will return an [Err] if there is a row that references the row that is being deleted.
    /// When this method returns [Ok] it will contain a [bool] that is either
    /// - `true` if the row was just deleted.
    /// - `false` if the row was deleted previously in this transaction.
    pub fn delete<T: Table<Schema = S>>(
        &mut self,
        val: TableRow<'t, T>,
    ) -> Result<bool, T::Referer> {
        let stmt = DeleteStatement::new()
            .from_table(Alias::new(T::NAME))
            .cond_where(Expr::col(Alias::new(T::ID)).eq(val.inner.idx))
            .to_owned();

        let (query, args) = stmt.build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.inner.transaction.prepare_cached(&query).unwrap();

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

    /// Delete a row from the database.
    ///
    /// This is the infallible version of [TransactionWeak::delete].
    ///
    /// To be able to use this method you have to mark the table as `#[no_reference]` in the schema.
    pub fn delete_ok<T: Table<Referer = Infallible, Schema = S>>(
        &mut self,
        val: TableRow<'t, T>,
    ) -> bool {
        let Ok(res) = self.delete(val);
        res
    }

    /// This allows you to do anything you want with the internal [rusqlite::Transaction]
    ///
    /// The specific version of rusqlite used is not stable. This means the [rusqlite]
    /// version might change as part of a non breaking version update of [rust_query].
    pub fn unchecked_transaction(&mut self) -> &rusqlite::Transaction {
        &self.inner.transaction
    }

    /// Make the changes made in this [TransactionWeak] permanent.
    ///
    /// If the [TransactionWeak] is dropped without calling this function, then the changes are rolled back.
    pub fn commit(self) {
        self.inner.commit();
    }
}

pub fn try_insert_private<'t, T: Table>(
    transaction: &Rc<rusqlite::Transaction<'t>>,
    table: sea_query::TableRef,
    idx: Option<i64>,
    val: T::Insert<'t>,
) -> Result<TableRow<'t, T>, T::Conflict<'t>> {
    let ast = MySelect::default();
    let reader = Reader::new(&ast);
    T::read(&val, reader);
    if let Some(idx) = idx {
        reader.col(T::ID, idx);
    }

    let select = ast.simple();

    let mut insert = InsertStatement::new();
    let names = ast.select.iter().map(|(_field, name)| *name);
    insert.into_table(table);
    insert.columns(names);
    insert.select_from(select).unwrap();
    insert.returning_col(Alias::new(T::ID));

    let (sql, values) = insert.build_rusqlite(SqliteQueryBuilder);

    let mut statement = transaction.prepare_cached(&sql).unwrap();
    let mut res = statement
        .query_map(&*values.as_params(), |row| {
            Ok(TableRow::<'_, T>::from_sql(row.get_ref(T::ID)?)?)
        })
        .unwrap();

    match res.next().unwrap() {
        Ok(id) => Ok(id),
        Err(rusqlite::Error::SqliteFailure(kind, Some(_val)))
            if kind.code == ErrorCode::ConstraintViolation =>
        {
            // val looks like "UNIQUE constraint failed: playlist_track.playlist, playlist_track.track"
            let conflict =
                Transaction::new(transaction.clone()).query_one(T::get_conflict_unchecked(&val));
            Err(conflict.unwrap())
        }
        Err(err) => Err(err).unwrap(),
    }
}
