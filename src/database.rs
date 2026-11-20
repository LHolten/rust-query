use std::{convert::Infallible, marker::PhantomData, sync::atomic::AtomicI64};

use self_cell::MutBorrow;

use crate::{
    Transaction,
    migrate::{Renderable, Schema},
    pool::Pool,
    transaction::{OwnedTransaction, TXN},
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
    pub(crate) pool: Pool,
    pub(crate) schema_version: AtomicI64,
    pub(crate) schema: PhantomData<S>,
    pub(crate) mut_lock: parking_lot::FairMutex<()>,
}

impl<S: Send + Sync + Schema> Database<S> {
    #[doc = include_str!("database/transaction.md")]
    pub fn transaction<R: Send>(&self, f: impl Send + FnOnce(&'static Transaction<S>) -> R) -> R {
        let res = std::thread::scope(|scope| scope.spawn(|| self.transaction_local(f)).join());
        match res {
            Ok(val) => val.unwrap_or_else(|e| e.to_panic()),
            Err(payload) => std::panic::resume_unwind(payload),
        }
    }

    /// Same as [Self::transaction], but can only be used on a new thread.
    pub(crate) fn transaction_local<R>(
        &self,
        f: impl FnOnce(&'static Transaction<S>) -> R,
    ) -> Result<R, Renderable> {
        let conn = self.pool.pop();

        let owned = OwnedTransaction::new(MutBorrow::new(conn), |conn| {
            Some(conn.borrow_mut().transaction().unwrap())
        });

        let res = f(Transaction::new_checked(owned, &self.schema_version)?);

        let owned = TXN.take().unwrap().into_owner();
        self.pool.push(owned.into_owner().into_inner());

        Ok(res)
    }

    #[doc = include_str!("database/transaction_mut.md")]
    pub fn transaction_mut<O: Send, E: Send>(
        &self,
        f: impl Send + FnOnce(&'static mut Transaction<S>) -> Result<O, E>,
    ) -> Result<O, E> {
        let join_res =
            std::thread::scope(|scope| scope.spawn(|| self.transaction_mut_local(f)).join());

        match join_res {
            Ok(val) => val.unwrap_or_else(|e| e.to_panic()),
            Err(payload) => std::panic::resume_unwind(payload),
        }
    }

    pub(crate) fn transaction_mut_local<O, E>(
        &self,
        f: impl FnOnce(&'static mut Transaction<S>) -> Result<O, E>,
    ) -> Result<Result<O, E>, Renderable> {
        // Acquire the lock before creating the connection.
        // Technically we can acquire the lock later, but we don't want to waste
        // file descriptors on transactions that need to wait anyway.
        let guard = self.mut_lock.lock();

        let conn = self.pool.pop();

        let owned = OwnedTransaction::new(MutBorrow::new(conn), |conn| {
            let txn = conn
                .borrow_mut()
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap();
            Some(txn)
        });

        let full_transaction = Transaction::new_checked(owned, &self.schema_version)?;
        // if this panics then the transaction is rolled back and the guard is dropped.
        let res = f(full_transaction);

        // Drop the guard before commiting to let sqlite go to the next transaction
        // more quickly while guaranteeing that the database will unlock soon.
        drop(guard);

        let owned = TXN.take().unwrap().into_owner();

        let conn = if res.is_ok() {
            owned.with(|x| x.commit().unwrap())
        } else {
            owned.with(|x| x.rollback().unwrap())
        };
        self.pool.push(conn);

        Ok(res)
    }

    #[doc = include_str!("database/transaction_mut_ok.md")]
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
    /// The `foreign_keys` pragma is always enabled here, even if [crate::migration::ForeignKeys::SQLite] is not used.
    ///
    /// Note that many systems have a limit on the number of file descriptors that can
    /// exist in a single process. On my machine the soft limit is (1024) by default.
    /// If this limit is reached, it may cause a panic in this method.
    pub fn rusqlite_connection(&self) -> rusqlite::Connection {
        let conn = self.pool.pop();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        conn
    }
}
