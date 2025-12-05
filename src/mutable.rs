use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use crate::{Table, TableRow, Transaction};

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

    pub fn into_table_row(self) -> TableRow<T> {
        self.row_id
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
            let update = T::mutable_into_update(self.inner.take().unwrap());
            Transaction::new_ref().update_ok(self.row_id, update);
        }
    }
}
