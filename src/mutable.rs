use std::{
    cell::OnceCell,
    ops::{Deref, DerefMut},
};

use crate::{IntoExpr, Table, TableRow, Transaction, transaction::try_update_private};

/// [Mutable] access to columns of a single table row.
///
/// The whole row is retrieved and can be inspected/updated from Rust code.
///
/// Only rows that are not used in a `#[unique]` constraint can be updated directly by dereferencing [Mutable].
/// To update columns with a unique constraint, you have to use [Mutable::unique].
pub struct Mutable<T: Table> {
    pub(crate) inner: OnceCell<T::Mutable>,
    pub(crate) row_id: TableRow<T>,
}

impl<T: Table> Mutable<T> {
    pub(crate) fn new(row_id: TableRow<T>) -> Box<Self> {
        Box::new(Mutable {
            inner: OnceCell::new(),
            row_id,
        })
    }

    /// Get the [TableRow] of the row that this [Mutable] refers to.
    pub fn table_row(&self) -> TableRow<T> {
        self.row_id
    }

    /// Update unique constraint columns.
    ///
    /// When the update succeeds, this function returns [Ok], when it fails it returns [Err] with one of
    /// three conflict types:
    /// - 0 unique constraints => [std::convert::Infallible]
    /// - 1 unique constraint => [TableRow] reference to the conflicting table row.
    /// - 2+ unique constraints => [crate::Conflict]
    ///
    /// If any of the changes made inside the closure conflict with an existing row, then all changes
    /// made inside the closure are reverted.
    ///
    /// If the closure panics, then all changes made inside the closure are also reverted.
    /// Applying those changes is not possible, as conflicts can not be reported if there is a panic.
    pub fn unique<O>(
        &mut self,
        f: impl FnOnce(&mut <T::Mutable as Deref>::Target) -> O,
    ) -> Result<O, T::Conflict> {
        // taking the data puts it in a guaranteed valid state
        if let Some(update) = self.inner.take() {
            try_update_private(self.row_id, update)
                .expect("flushing non unique update should always work");
        }

        let data = Transaction::new_ref().query_one(T::into_select(self.row_id.into_expr()));
        let mut data = T::select_mutable(data);

        // no need to catch panics here because we already guaranteed a valid state.
        let out = f(T::mutable_as_unique(&mut data));

        // only apply the update if there was no panic
        try_update_private(self.row_id, data)?;

        Ok(out)
    }
}

impl<T: Table> Deref for Mutable<T> {
    type Target = T::Mutable;

    fn deref(&self) -> &Self::Target {
        self.inner.get_or_init(|| {
            let data = Transaction::new_ref().query_one(T::into_select(self.row_id.into_expr()));
            T::select_mutable(data)
        })
    }
}

impl<T: Table> DerefMut for Mutable<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        let _ = Deref::deref(self);
        self.inner.get_mut().unwrap()
    }
}

impl<T: Table> Drop for Mutable<T> {
    fn drop(&mut self) {
        if let Some(update) = self.inner.take() {
            let Ok(_) = try_update_private(self.row_id, update) else {
                panic!("mutable can not fail, no unique is updated")
            };
        }
    }
}
