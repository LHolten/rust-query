use std::ops::{Deref, DerefMut};

use crate::{Table, TableRow, Transaction};

pub struct Mutable<'transaction, T: Table> {
    pub(crate) inner: Option<T::Mutable>,
    pub(crate) row_id: TableRow<T>,
    pub(crate) any_update: bool,
    pub(crate) txn: &'transaction mut Transaction<T::Schema>,
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
        let update = T::mutable_into_update(self.inner.take().unwrap());
        self.txn.update_ok(self.row_id, update);
    }
}
