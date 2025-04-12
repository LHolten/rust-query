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
/// Creating a [Database] requires going through the steps to migrate an existing database to
/// the required schema, or creating a new database from scratch (See also [crate::migration::Config]).
/// Having done the setup to create a compatible database is sadly not a guarantee that the
/// database will stay compatible for the lifetime of the [Database] struct.
///
/// That is why [Database] also stores the `schema_version`. This allows detecting non-malicious
/// modifications to the schema and gives us the ability to panic when this is detected.
/// Such non-malicious modification of the schema can happen for example if another [Database]
/// instance is created with additional migrations (e.g. by another newer instance of your program).
///
/// For information on how to create transactions, please refer to [LocalClient].
pub struct Database<S> {
    pub(crate) manager: r2d2_sqlite::SqliteConnectionManager,
    pub(crate) schema_version: i64,
    pub(crate) schema: PhantomData<S>,
}

impl<S> Database<S> {
    /// Create a new [rusqlite::Connection] to the database.
    ///
    /// You can do (almost) anything you want with this connection as it is almost completely isolated from all other
    /// [rust_query] connections. The only thing you should not do here is changing the schema.
    /// Schema changes are detected with the `schema_version` pragma and will result in a panic when creating a new
    /// transaction.
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
    /// ```
    /// # use rust_query::{private::doctest::*, Table};
    /// # let mut client = get_client();
    /// # let txn = get_txn(&mut client);
    /// let user_names = txn.query(|rows| {
    ///     let user = User::join(rows);
    ///     rows.into_vec(user.name())
    /// });
    /// assert_eq!(user_names, vec!["Alice".to_owned()]);
    /// ```
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
    /// ```
    /// # use rust_query::{private::doctest::*, IntoExpr};
    /// # let mut client = rust_query::private::doctest::get_client();
    /// # let txn = rust_query::private::doctest::get_txn(&mut client);
    /// let res = txn.query_one("test".into_expr());
    /// assert_eq!(res, "test");
    /// ```
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
    /// - 1 unique constraint => [Expr] reference to the conflicting table row.
    /// - 2+ unique constraints => `()` no further information is provided.
    ///
    /// ```
    /// # use rust_query::{private::doctest::*, IntoExpr};
    /// # let mut client = rust_query::private::doctest::get_client();
    /// # let mut txn = rust_query::private::doctest::get_txn(&mut client);
    /// let res = txn.insert(User {
    ///     name: "Bob",
    /// });
    /// assert!(res.is_ok());
    /// let res = txn.insert(User {
    ///     name: "Bob",
    /// });
    /// assert!(res.is_err(), "there is a unique constraint on the name");
    /// ```
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
    ///
    /// ```
    /// # use rust_query::{private::doctest::*, IntoExpr};
    /// # let mut client = rust_query::private::doctest::get_client();
    /// # let mut txn = rust_query::private::doctest::get_txn(&mut client);
    /// let bob = txn.insert(User {
    ///     name: "Bob",
    /// }).unwrap();
    /// let bob2 = txn.find_or_insert(User {
    ///     name: "Bob", // this will conflict with the existing row.
    /// });
    /// assert_eq!(bob, txn.query_one(bob2));
    /// ```
    pub fn find_or_insert<T: Table<Schema = S, Conflict<'t> = crate::Expr<'t, S, T>>>(
        &mut self,
        val: impl TableInsert<'t, T = T>,
    ) -> crate::Expr<'t, S, T> {
        match self.insert(val) {
            Ok(row) => row.into_expr(),
            Err(row) => row,
        }
    }

    /// Try updating a row in the database to have new column values.
    ///
    /// Updating can fail just like [TransactionMut::insert] because of unique constraint conflicts.
    /// This happens when the new values are in conflict with an existing different row.
    ///
    /// When the update succeeds, this function returns [Ok<()>], when it fails it returns [Err] with one of
    /// three conflict types:
    /// - 0 unique constraints => [Infallible]
    /// - 1 unique constraint => [Expr] reference to the conflicting table row.
    /// - 2+ unique constraints => `()` no further information is provided.
    ///
    /// ```
    /// # use rust_query::{private::doctest::*, IntoExpr, Update};
    /// # let mut client = rust_query::private::doctest::get_client();
    /// # let mut txn = rust_query::private::doctest::get_txn(&mut client);
    /// let bob = txn.insert(User {
    ///     name: "Bob",
    /// }).unwrap();
    /// txn.update(bob, User {
    ///     name: Update::set("New Bob"),
    /// }).unwrap();
    /// ```
    pub fn update<T: Table<Schema = S>>(
        &mut self,
        row: impl IntoExpr<'t, S, Typ = T>,
        val: T::Update<'t>,
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
                Err(T::get_conflict_unchecked(&val))
            }
            Err(err) => Err(err).unwrap(),
        }
    }

    /// This is a convenience function to use [TransactionMut::update] for updates
    /// that can not cause unique constraint violations.
    ///
    /// This method can be used for all tables, it just does not allow modifying
    /// columns that are part of unique constraints.
    pub fn update_ok<T: Table<Schema = S>>(
        &mut self,
        row: impl IntoExpr<'t, S, Typ = T>,
        val: T::UpdateOk<'t>,
    ) {
        match self.update(row, T::update_into_try_update(val)) {
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

    /// Convert the [TransactionMut] into a [TransactionWeak] to allow deletions.
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

    /// This allows you to do (almost) anything you want with the internal [rusqlite::Transaction].
    ///
    /// Note that there are some things that you should not do with the transaction, such as:
    /// - Changes to the schema, these will result in a panic as described in [Database].
    /// - Changes to the connection configuration such as disabling foreign key checks.
    ///
    /// **When this method is used to break [rust_query] invariants, all other [rust_query] function calls
    /// may result in a panic.**
    pub fn rusqlite_transaction(&mut self) -> &rusqlite::Transaction {
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
            Err(T::get_conflict_unchecked(&val))
        }
        Err(err) => Err(err).unwrap(),
    }
}
