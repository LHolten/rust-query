use std::{
    cell::RefCell, convert::Infallible, marker::PhantomData, rc::Rc, sync::atomic::AtomicI64,
};

use rusqlite::ErrorCode;
use self_cell::{MutBorrow, self_cell};

use crate::{
    IntoExpr, IntoSelect, Table, TableRow,
    error::FromConflict,
    lower::{
        self, JoinableTableWithId, emit,
        list_writer::{Alias, ListWriter},
        ord_rc::OrdRc,
    },
    migrate::{Renderable, Schema, check_schema, schema_version, user_version},
    private::{IntoJoinable, Reader},
    query::{OwnedRows, Query, track_stmt},
    rows::Rows,
    schema::read::Pragma,
    transaction_scope::TransactionScope,
    value::{DbTyp, LazyOf, OptTable, out_to_lazy},
};

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
#[cfg(test)]
static_assertions::assert_not_impl_any! {OwnedTransaction: Sync}

thread_local! {
    pub(crate) static TXN: RefCell<Option<TransactionWithRows>> = const { RefCell::new(None) };
}

impl OwnedTransaction {
    pub(crate) fn get(&self) -> &rusqlite::Transaction<'_> {
        self.borrow_dependent().as_ref().unwrap()
    }

    pub(crate) fn with(
        mut self,
        f: impl FnOnce(rusqlite::Transaction<'_>),
    ) -> rusqlite::Connection {
        self.with_dependent_mut(|_, b| f(b.take().unwrap()));
        self.into_owner().into_inner()
    }
}

type OwnedRowsVec<'x> = slab::Slab<OwnedRows<'x>>;
self_cell!(
    pub struct TransactionWithRows {
        owner: OwnedTransaction,

        #[not_covariant]
        dependent: OwnedRowsVec,
    }
);

impl TransactionWithRows {
    pub(crate) fn new_empty(txn: OwnedTransaction) -> Self {
        Self::new(txn, |_| slab::Slab::new())
    }

    pub(crate) fn get(&self) -> &rusqlite::Transaction<'_> {
        self.borrow_owner().get()
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
    pub(crate) fn new_ref() -> &'static mut Self {
        // no memory is leaked because Self is zero sized
        const { assert!(size_of::<Self>() == 0) };
        Box::leak(Box::new(Self {
            _p2: PhantomData,
            _local: PhantomData,
        }))
    }

    pub(crate) fn pragma(&self) -> &'static Transaction<Pragma> {
        Transaction::new_ref()
    }

    pub(crate) fn copy(&self) -> Self {
        Self {
            _p2: PhantomData,
            _local: PhantomData,
        }
    }
}

impl<S: Schema> Transaction<S> {
    /// This will check the schema version and panic if it is not as expected
    pub(crate) fn new_checked(
        txn: OwnedTransaction,
        expected: &AtomicI64,
    ) -> Result<&'static mut Transaction<S>, Renderable> {
        let schema_version = schema_version(txn.get());
        // If the schema version is not the expected version then we
        // check if the changes are acceptable.
        if schema_version != expected.load(std::sync::atomic::Ordering::Relaxed) {
            if user_version(txn.get()).unwrap() != S::VERSION {
                panic!("The database user_version changed unexpectedly")
            }

            TXN.set(Some(TransactionWithRows::new_empty(txn)));
            check_schema::<S>(Self::new_ref())?;
            expected.store(schema_version, std::sync::atomic::Ordering::Relaxed);
        } else {
            TXN.set(Some(TransactionWithRows::new_empty(txn)));
        }

        Ok(Transaction::new_ref())
    }
}

impl<S: 'static> Transaction<S> {
    /// Create a transaction scope that limits the lifetime of [crate::Mutable].
    pub fn scope<O>(&mut self, f: impl FnOnce(&mut TransactionScope<S>) -> O) -> O {
        let mut txn = TransactionScope {
            _p2: PhantomData,
            tmp: Default::default(),
        };
        f(&mut txn)
    }

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
    pub fn query<'t, R>(&'t self, f: impl FnOnce(&mut Query<'t, '_, S>) -> R) -> R {
        // Execution already happens in a [Transaction].
        // and thus any [TransactionMut] that it might be borrowed
        // from is borrowed immutably, which means the rows can not change.
        let q = Rows {
            phantom: PhantomData,
            ast: Default::default(),
            _p: PhantomData,
        };
        f(&mut Query {
            q,
            phantom: PhantomData,
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
        let mut query = self.query(|e| e.into_iter(val.into_select()));
        let res = query.next().unwrap();
        debug_assert!(query.next().is_none(), "query should return one row");
        res
    }

    /// Retrieve a [crate::Lazy] or `Option<Lazy>` from the database.
    ///
    /// This is very similar to [Self::query_one], except that it retrieves
    /// [crate::Lazy] instead of [TableRow]. As such it only works with
    /// table valued [rust_query::Expr].
    ///
    /// ```
    /// # #[rust_query::migration::schema(M)]
    /// # pub mod vN {
    /// #     pub struct Author {
    /// #         pub name: String,
    /// #     }
    /// #     pub struct Page {
    /// #         pub content: String,
    /// #         pub title: String,
    /// #         pub author: rust_query::TableRow<Author>,
    /// #     }
    /// # }
    /// # use v0::*;
    /// # rust_query::Database::new(rust_query::migration::Config::open_in_memory()).transaction_mut_ok(|mut txn| {
    /// let cat = txn.insert_ok(Author {
    ///     name: "Cat".to_owned()
    /// });
    /// let blog_post = txn.insert_ok(Page {
    ///     content: "Hello world!".to_owned(),
    ///     title: "Hi".to_owned(),
    ///     author: cat,
    /// });
    /// let lazy_post = txn.lazy(blog_post);
    ///
    /// println!("{}:", lazy_post.title);
    /// println!("{}", lazy_post.content);
    /// println!("written by: {}", lazy_post.author.name);
    /// # });
    /// ```
    pub fn lazy<'t, T: OptTable<Schema = S>>(
        &'t self,
        val: impl IntoExpr<'static, S, Typ = T>,
    ) -> LazyOf<'t, T> {
        out_to_lazy(self.query_one(val.into_expr()))
    }

    /// This retrieves an iterator of [crate::Lazy] values.
    ///
    /// Refer to [Rows::join] for the kind of the parameter that is supported here.
    /// Refer to [Transaction::lazy] for the single row version.
    pub fn lazy_iter<'t, T: Table<Schema = S>>(
        &'t self,
        val: impl IntoJoinable<'static, S, Typ = TableRow<T>>,
    ) -> LazyIter<'t, T> {
        let val = val.into_joinable();
        self.query(|rows| {
            let table = rows.join(val);
            LazyIter {
                txn: self,
                iter: rows.into_iter(table),
            }
        })
    }
}

pub struct LazyIter<'t, T: Table> {
    txn: &'t Transaction<T::Schema>,
    iter: crate::query::Iter<'t, TableRow<T>>,
}

impl<'t, T: Table> Iterator for LazyIter<'t, T> {
    type Item = crate::Lazy<'t, T>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|x| self.txn.lazy(x))
    }
}

impl<S: 'static> Transaction<S> {
    /// Try inserting a value into the database.
    ///
    /// Returns [Ok] with a reference to the new inserted value or an [Err] with conflict information.
    /// The type of conflict information depends on the number of unique constraints on the table:
    /// - 0 unique constraints => [Infallible]
    /// - 1 unique constraint => [TableRow] reference to the conflicting table row.
    /// - 2+ unique constraints => [crate::Conflict].
    ///
    /// ```
    /// # use rust_query::{private::doctest::*, IntoExpr};
    /// # rust_query::private::doctest::get_txn(|mut txn| {
    /// let res = txn.insert(User {
    ///     name: "Bob".to_owned(),
    /// });
    /// assert!(res.is_ok());
    /// let res = txn.insert(User {
    ///     name: "Bob".to_owned(),
    /// });
    /// assert!(res.is_err(), "there is a unique constraint on the name");
    /// # });
    /// ```
    pub fn insert<T: Table<Schema = S>>(&mut self, val: T) -> Result<TableRow<T>, T::Conflict> {
        try_insert_private(lower::JoinableTable::Table(T::NAME, None), None, val)
    }

    /// This is a convenience function to make using [Transaction::insert]
    /// easier for tables without unique constraints.
    ///
    /// The new row is added to the table and the row reference is returned.
    pub fn insert_ok<T: Table<Schema = S, Conflict = Infallible>>(
        &mut self,
        val: T,
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
    ///     name: "Bob".to_owned(),
    /// }).unwrap();
    /// let bob2 = txn.find_or_insert(User {
    ///     name: "Bob".to_owned(), // this will conflict with the existing row.
    /// });
    /// assert_eq!(bob, bob2);
    /// # });
    /// ```
    pub fn find_or_insert<T: Table<Schema = S, Conflict = TableRow<T>>>(
        &mut self,
        val: T,
    ) -> TableRow<T> {
        match self.insert(val) {
            Ok(row) => row,
            Err(row) => row,
        }
    }

    /// Convert the [Transaction] into a [TransactionWeak] to allow deletions.
    pub fn downgrade(&'static mut self) -> &'static mut TransactionWeak<S> {
        Box::leak(Box::new(TransactionWeak { _p: PhantomData }))
    }
}

/// This is the weak version of [Transaction].
///
/// The reason that it is called `weak` is because [TransactionWeak] can not guarantee
/// that [TableRow]s prove the existence of their particular row.
///
/// [TransactionWeak] is useful because it allowes deleting rows.
pub struct TransactionWeak<S> {
    _p: PhantomData<Transaction<S>>,
}

impl<S: Schema> TransactionWeak<S> {
    /// Try to delete a row from the database.
    ///
    /// This will return an [Err] if there is a row that references the row that is being deleted.
    /// When this method returns [Ok] it will contain a [bool] that is either
    /// - `true` if the row was just deleted.
    /// - `false` if the row was deleted previously in this transaction.
    pub fn delete<T: Table<Schema = S>>(&mut self, val: TableRow<T>) -> Result<bool, T::Referer> {
        let schema = crate::schema::from_macro::Schema::new::<S>();

        // This is a manual check that foreign key constraints are not violated.
        // We do this manually because we don't want to enabled foreign key constraints for the whole
        // transaction (and is not possible to enable for part of a transaction).
        let mut checks = vec![];
        for (&table_name, table) in &schema.tables {
            for col in table.columns.iter().filter_map(|(col_name, col)| {
                let col = &col.def;
                col.fk
                    .as_ref()
                    .is_some_and(|(t, c)| t == T::NAME && c == T::ID)
                    .then_some(col_name)
            }) {
                let mut stmt = emit::Stmt::default();
                stmt.write("SELECT ");
                stmt.write_param(&OrdRc(Rc::new(val.inner.idx.into())));
                stmt.write(format_args!(
                    " IN (SELECT {0}.{1} FROM {0})",
                    Alias(table_name),
                    Alias(col)
                ));
                checks.push(stmt);
            }
        }

        let mut stmt = emit::Stmt::default();
        stmt.write(format_args!(
            "DELETE FROM {0} WHERE {0}.{1} = ",
            Alias(T::NAME),
            Alias(T::ID)
        ));
        stmt.write_param(&OrdRc::new(val.inner.idx));

        TXN.with_borrow(|txn| {
            let txn = txn.as_ref().unwrap().get();

            for stmt in checks {
                let mut cached = txn.prepare_cached(&stmt.sql).unwrap();
                match cached.query_one(rusqlite::params_from_iter(stmt.params), |r| r.get(0)) {
                    Ok(true) => return Err(T::get_referer_unchecked()),
                    Ok(false) => {}
                    Err(err) => panic!("{err:?}"),
                }
            }

            let mut cached = txn.prepare_cached(&stmt.sql).unwrap();
            match cached.execute(rusqlite::params_from_iter(stmt.params)) {
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
    /// - Changes to the schema, these will result in a panic as described in [crate::Database].
    /// - Making changes that violate foreign-key constraints (see below).
    ///
    /// Sadly it is not possible to enable (or disable) the `foreign_keys` pragma during a transaction.
    /// This means that whether this pragma is enabled depends on which [crate::migration::ForeignKeys]
    /// option is used and can not be changed.
    pub fn rusqlite_transaction<R>(&mut self, f: impl FnOnce(&rusqlite::Transaction) -> R) -> R {
        TXN.with_borrow(|txn| f(txn.as_ref().unwrap().get()))
    }
}

pub fn try_insert_private<T: Table>(
    table: lower::JoinableTable,
    idx: Option<i64>,
    val: T,
) -> Result<TableRow<T>, T::Conflict> {
    let mut reader = Reader::default();
    T::read(&val, &mut reader);
    if let Some(idx) = idx {
        reader.col::<i64>(T::ID, idx);
    }

    let mut stmt = emit::Stmt::default();
    stmt.write("INSERT INTO ");
    table.emit(&mut stmt);

    if reader.builder.is_empty() {
        // values always has at least one column, so we leave it out when there are no columns
        stmt.write(" DEFAULT VALUES");
    } else {
        let (col_names, col_exprs): (Vec<_>, Vec<_>) = reader.builder.clone().into_iter().collect();

        stmt.write(" (");
        let mut list = ListWriter::new(&mut stmt, ", ");
        for col in col_names {
            list.item().write(Alias(col));
        }
        stmt.write(") VALUES (");
        let mut list = ListWriter::new(&mut stmt, ", ");
        for val in col_exprs {
            list.item().write_param(&val);
        }
        stmt.write(")");
    }
    stmt.write(" RETURNING ").write(T::ID);

    let res = TXN.with_borrow(|txn| {
        let txn = txn.as_ref().unwrap().get();
        track_stmt(txn, &stmt.sql, &stmt.params);

        let mut statement = txn.prepare_cached(&stmt.sql).unwrap();
        let mut res = statement
            .query_map(rusqlite::params_from_iter(stmt.params), |row| {
                Ok(TableRow::<T>::from_sql(row.get_ref(T::ID)?.as_i64()?))
            })
            .unwrap();

        res.next().unwrap()
    });

    match res {
        Ok(id) => {
            if let Some(idx) = idx {
                assert_eq!(idx, id.inner.idx);
            }
            Ok(id)
        }
        Err(rusqlite::Error::SqliteFailure(kind, Some(msg)))
            if kind.code == ErrorCode::ConstraintViolation =>
        {
            // val looks like "UNIQUE constraint failed: playlist_track.playlist, playlist_track.track"
            let res = TXN.with_borrow(|txn| {
                let txn = txn.as_ref().unwrap().get();
                <T::Conflict as FromConflict>::from_conflict(
                    txn,
                    JoinableTableWithId {
                        name: table,
                        main_column: T::ID,
                    },
                    reader.builder,
                    msg,
                )
            });
            Err(res)
        }
        Err(err) => panic!("{err:?}"),
    }
}

pub(crate) fn try_update_private<T: Table>(
    row: TableRow<T>,
    val: T::Mutable,
) -> Result<(), T::Conflict> {
    let val = T::mutable_into_insert(val);
    let mut reader = Reader::default();
    T::read(&val, &mut reader);

    let mut stmt = emit::Stmt::default();
    stmt.write("UPDATE ");
    lower::JoinableTable::Table(T::NAME, None).emit(&mut stmt);

    stmt.write(" SET ");
    let mut list = ListWriter::new(&mut stmt, ", ");
    for (key, val) in &reader.builder {
        list.item()
            .write(format_args!("{} = ", Alias(key)))
            .write_param(val);
    }
    list.default(format_args!("{1} = {0}.{1}", Alias(T::NAME), Alias(T::ID)));

    stmt.write(format_args!(
        " WHERE {}.{} = ",
        Alias(T::NAME),
        Alias(T::ID)
    ));
    stmt.write_param(&OrdRc(Rc::new(row.inner.idx.into())));

    let res = TXN.with_borrow(|txn| {
        let txn = txn.as_ref().unwrap().get();

        let mut cached = txn.prepare_cached(&stmt.sql).unwrap();
        cached.execute(rusqlite::params_from_iter(stmt.params))
    });

    match res {
        Ok(1) => Ok(()),
        Ok(n) => panic!("unexpected number of updates: {n}"),
        Err(rusqlite::Error::SqliteFailure(kind, Some(msg)))
            if kind.code == ErrorCode::ConstraintViolation =>
        {
            // `msg` looks like "UNIQUE constraint failed: playlist_track.playlist, playlist_track.track"
            let res = TXN.with_borrow(|txn| {
                let txn = txn.as_ref().unwrap().get();
                <T::Conflict as FromConflict>::from_conflict(
                    txn,
                    lower::JoinableTableWithId {
                        name: lower::JoinableTable::Table(T::NAME, None),
                        main_column: T::ID,
                    },
                    reader.builder,
                    msg,
                )
            });
            Err(res)
        }
        Err(err) => panic!("{err:?}"),
    }
}
