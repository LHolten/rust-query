use std::{
    cell::OnceCell,
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use crate::{IntoExpr, Table, TableRow, Transaction};

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
    cell: OnceCell<MutableInner<T>>,
    row_id: TableRow<T>,
    _txn: PhantomData<&'transaction mut Transaction<T::Schema>>,
}

struct MutableInner<T: Table> {
    val: T::Mutable,
    any_update: bool,
}

impl<T: Table> MutableInner<T> {
    fn new(row_id: TableRow<T>) -> Self {
        Self {
            val: Transaction::new_ref()
                .query_one(T::select_mutable(row_id.into_expr()))
                .0,
            any_update: false,
        }
    }
}

impl<'transaction, T: Table> Mutable<'transaction, T> {
    pub(crate) fn new(inner: T::Mutable, row_id: TableRow<T>) -> Self {
        Self {
            cell: OnceCell::from(MutableInner {
                val: inner,
                any_update: false,
            }),
            row_id,
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
    pub fn unique<O>(
        &mut self,
        f: impl FnOnce(&mut <T::Mutable as Deref>::Target) -> O,
    ) -> Result<O, T::Conflict> {
        let res = f(T::mutable_as_unique(self));
        let txn = Transaction::new_ref();
        // taking `self.cell` means that values are read from the database again the next time
        // that the `Mutable` is dereferenced.
        let update = T::mutable_into_update(self.cell.take().unwrap().val);
        #[expect(deprecated)]
        txn.update(self.row_id, update)?;
        Ok(res)
    }
}

impl<'transaction, T: Table> Deref for Mutable<'transaction, T> {
    type Target = T::Mutable;

    fn deref(&self) -> &Self::Target {
        &self.cell.get_or_init(|| MutableInner::new(self.row_id)).val
    }
}

impl<'transaction, T: Table> DerefMut for Mutable<'transaction, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // initialize the cell
        let _ = self.deref();
        let inner = self.cell.get_mut().unwrap();
        inner.any_update = true;
        &mut inner.val
    }
}

impl<'transaction, T: Table> Drop for Mutable<'transaction, T> {
    fn drop(&mut self) {
        let Some(cell) = self.cell.take() else {
            return;
        };
        if cell.any_update {
            let update = T::mutable_into_update(cell.val);
            #[expect(deprecated)]
            let Ok(_) = Transaction::new_ref().update(self.row_id, update) else {
                panic!("mutable can not fail, no unique is updated")
            };
        }
    }
}
