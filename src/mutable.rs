use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use crate::{Table, TableRow, Transaction};

/// [Mutable] access to columns of a single table row.
///
/// The whole row is retrieved and can be inspected from Rust code.
/// However, only rows that are not used in a `#[unique]`
/// constraint can be updated using [Mutable].
///
/// To update columns with a unique constraint, please use [Transaction::update] for now.
///
/// [Mutable] only executes an `UPDATE` statement when it is dropped.
/// This delay can not be observed because the transaction is borrowed mutably.
pub struct Mutable<'transaction, T: Table> {
    inner: Option<T::Mutable>,
    row_id: TableRow<T>,
    any_update: bool,
    _txn: PhantomData<&'transaction mut Transaction<T::Schema>>,
}

impl<'transaction, T: Table> Mutable<'transaction, T> {
    pub(crate) fn new(inner: T::Mutable, row_id: TableRow<T>) -> Self {
        Self {
            inner: Some(inner),
            row_id,
            any_update: false,
            _txn: PhantomData,
        }
    }

    /// Turn the [Mutable] into a [TableRow].
    ///
    /// This will end the lifetime of the [Mutable], which is useful since
    /// [Mutable] does not have a non lexical lifetime, because of the [Drop] impl.
    ///
    /// If you do not need the [TableRow], then it is also possible to just call [drop].
    pub fn into_table_row(self) -> TableRow<T> {
        self.row_id
    }

    /// Update unique constraint columns.
    ///
    /// This can result in a conflict with other rows.
    pub fn unique(
        mut self,
        f: impl FnOnce(&mut <T::Mutable as Deref>::Target),
    ) -> Result<Self, T::Conflict> {
        f(T::mutable_as_unique(self.inner.as_mut().unwrap()));
        let txn = Transaction::new_ref();
        #[expect(deprecated)]
        txn.update(
            self.row_id,
            T::mutable_into_update(self.inner.take().unwrap()),
        )?;
        Ok(txn.mutable(self.row_id))
    }
}

impl<'transaction, T: Table> Deref for Mutable<'transaction, T> {
    type Target = T::Mutable;

    fn deref(&self) -> &Self::Target {
        self.inner.as_ref().unwrap()
    }
}

impl<'transaction, T: Table> DerefMut for Mutable<'transaction, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.any_update = true;
        self.inner.as_mut().unwrap()
    }
}

impl<'transaction, T: Table> Drop for Mutable<'transaction, T> {
    fn drop(&mut self) {
        if self.any_update {
            let Some(inner) = self.inner.take() else {
                return;
            };
            let update = T::mutable_into_update(inner);
            #[expect(deprecated)]
            let Ok(_) = Transaction::new_ref().update(self.row_id, update) else {
                panic!("mutable can not fail, no unique is updated")
            };
        }
    }
}
