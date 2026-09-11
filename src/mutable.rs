use std::{
    any::Any,
    ops::{Deref, DerefMut},
};

use crate::{
    IntoExpr, Table, TableRow, Transaction, scoped_transaction::MutTemp,
    transaction::try_update_private,
};

/// [Mutable] access to columns of a single table row.
///
/// The whole row is retrieved and can be inspected/updated from Rust code.
///
/// Only rows that are not used in a `#[unique]` constraint can be updated directly by dereferencing [Mutable].
/// To update columns with a unique constraint, you have to use [Mutable::unique].
pub struct Mutable<'transaction, T: Table> {
    pub(crate) temp: &'transaction mut MutTemp<T>,
}

impl<'transaction, T: Table> Mutable<'transaction, T> {
    pub(crate) fn new(temp: &'transaction mut dyn Any) -> Self {
        Self {
            temp: (temp as &mut dyn Any).downcast_mut().unwrap(),
        }
    }

    /// Turn the [Mutable] into a [TableRow].
    ///
    /// This will end the lifetime of the [Mutable], which is useful since
    /// [Mutable] does not have a non lexical lifetime, because of the [Drop] impl.
    ///
    /// If you do not need the [TableRow], then it is also possible to just call [drop].
    pub fn table_row(&self) -> TableRow<T> {
        self.temp.row_id
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
        if let Some(update) = self.temp.inner.take() {
            try_update_private(self.temp.row_id, update)
                .expect("flushing non unique update should always work");
        }

        let data = Transaction::new_ref().query_one(T::into_select(self.temp.row_id.into_expr()));
        let mut data = T::select_mutable(data);

        // no need to catch panics here because we already guaranteed a valid state.
        let out = f(T::mutable_as_unique(&mut data));

        // only apply the update if there was no panic
        try_update_private(self.temp.row_id, data)?;

        Ok(out)
    }
}

impl<'transaction, T: Table> Deref for Mutable<'transaction, T> {
    type Target = T::Mutable;

    fn deref(&self) -> &Self::Target {
        self.temp.inner.get_or_init(|| {
            let data =
                Transaction::new_ref().query_one(T::into_select(self.temp.row_id.into_expr()));
            T::select_mutable(data)
        })
    }
}

impl<'transaction, T: Table> DerefMut for Mutable<'transaction, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        let _ = Deref::deref(self);
        self.temp.inner.get_mut().unwrap()
    }
}
