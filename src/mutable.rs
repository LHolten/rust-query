use std::{
    cell::OnceCell,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    panic::AssertUnwindSafe,
};

use crate::{IntoExpr, Table, TableRow, Transaction};

/// [Mutable] access to columns of a single table row.
///
/// The whole row is retrieved and can be inspected from Rust code.
/// However, only rows that are not used in a `#[unique]`
/// constraint can be updated directly by dereferencing [Mutable].
///
/// To update columns with a unique constraint, you have to use [Mutable::unique].
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
            val: Transaction::new_ref().query_one(T::select_mutable(row_id.into_expr())),
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
    /// When the update succeeds, this function returns [Ok], when it fails it returns [Err] with one of
    /// three conflict types:
    /// - 0 unique constraints => [std::convert::Infallible]
    /// - 1 unique constraint => [TableRow] reference to the conflicting table row.
    /// - 2+ unique constraints => `()` no further information is provided.
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
        // this drops the old mutable, causing all previous writes to be applied.
        *self = Mutable {
            cell: OnceCell::new(),
            row_id: self.row_id,
            _txn: PhantomData,
        };
        // we need to catch panics so that we can restore `self` to a valid state.
        // if we don't do this then the Drop impl is likely to panic.
        let res = std::panic::catch_unwind(AssertUnwindSafe(|| f(T::mutable_as_unique(self))));
        // taking `self.cell` puts the Mutable in a guaranteed valid state.
        // it doesn't matter if the update succeeds or not as long as we only deref after the update.
        let update = T::mutable_into_update(self.cell.take().unwrap().val);
        let out = match res {
            Ok(out) => out,
            Err(payload) => std::panic::resume_unwind(payload),
        };
        // only apply the update if there was no panic
        #[expect(deprecated)]
        Transaction::new_ref().update(self.row_id, update)?;

        Ok(out)
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
        let _ = Mutable::deref(self);
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

#[cfg(test)]
mod tests {

    use crate::{Database, migration::Config};

    #[test]
    fn mutable_shenanigans() {
        #[crate::migration::schema(Test)]
        pub mod vN {
            pub struct Foo {
                pub alpha: i64,
                #[unique]
                pub bravo: i64,
            }
        }
        use v0::*;

        let err = std::panic::catch_unwind(move || {
            let db = Database::new(Config::open_in_memory());
            db.transaction_mut_ok(|txn| {
                txn.insert(Foo { alpha: 1, bravo: 1 }).unwrap();
                let row = txn.insert(Foo { alpha: 1, bravo: 2 }).unwrap();
                let mut mutable = txn.mutable(row);
                mutable.alpha = 100;
                mutable
                    .unique(|x| {
                        x.bravo = 1;
                    })
                    .unwrap_err();
                assert_eq!(mutable.alpha, 100);
                assert_eq!(mutable.bravo, 2);

                let row = mutable.into_table_row();
                let view = txn.lazy(row);
                assert_eq!(view.alpha, 100);
                assert_eq!(view.bravo, 2);

                let mut mutable = txn.mutable(row);
                mutable.alpha = 200;
                mutable
                    .unique(|x| {
                        x.bravo = 1;
                        panic!("error in unique")
                    })
                    .unwrap();
            });
        })
        .unwrap_err();
        assert_eq!(*err.downcast_ref::<&str>().unwrap(), "error in unique");
    }
}
