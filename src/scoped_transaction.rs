use std::{
    any::Any,
    cell::{Cell, OnceCell},
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use crate::{
    IntoExpr, Mutable, Table, TableRow, Transaction, private::IntoJoinable,
    transaction::try_update_private, value::OptTable,
};

/// [Transaction] with mutation support and without downgrade support.
///
/// This type can be created using [Transaction::scoped].
pub struct TransactionScoped<S: 'static> {
    pub(crate) _p2: PhantomData<&'static Transaction<S>>,
    pub(crate) tmp: Cell<Vec<Box<dyn Any>>>,
}

impl<S> DerefMut for TransactionScoped<S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.tmp.take();
        Transaction::new_ref()
    }
}

impl<S> Deref for TransactionScoped<S> {
    type Target = Transaction<S>;

    fn deref(&self) -> &Self::Target {
        self.tmp.take();
        Transaction::new_ref()
    }
}

impl<S> TransactionScoped<S> {
    /// Retrieves a [Mutable] or `Option<Mutable>` from the database.
    ///
    /// The [Transaction] is borrowed mutably until the [Mutable] is dropped.
    ///
    /// ```
    /// # #[rust_query::migration::schema(M)]
    /// # pub mod vN {
    /// #     pub struct Player {
    /// #         #[unique]
    /// #         pub number: i64,
    /// #         pub name: String,
    /// #         pub score: i64,
    /// #     }
    /// # }
    /// # use v0::*;
    /// # rust_query::Database::new(rust_query::migration::Config::open_in_memory()).transaction_mut_ok(|txn| {
    /// txn.scoped(|txn| {
    /// let baz_id = txn.insert(Player {number: 1, name: "Baz".to_owned(), score: 0}).unwrap();
    ///
    /// let mut tmp = txn.mutable(baz_id);
    /// tmp.score += 50;
    /// tmp.name = format!("{}{}", tmp.name, tmp.score);
    ///
    /// if let Some(mut player) = txn.mutable(Player.number(1)) {
    ///     player.score += 100;
    /// }
    /// # })});
    /// ```
    pub fn mutable<'t, T: OptTable<Schema = S>>(
        &'t mut self,
        val: impl IntoExpr<'static, S, Typ = T>,
    ) -> T::Mutable<'t> {
        let x = self.query_one(val.into_expr());
        T::into_mutable(self, x)
    }

    /// Retrieve multiple [Mutable] rows from the database.
    ///
    /// Refer to [crate::args::Rows::join] for the kind of the parameter that is supported here.
    /// This may be useful when you need mutable access to multiple rows (potentially at the same time).
    ///
    /// Getting a lazy [Iterator] over mutable rows instead of a [Vec] is not possible, because mutating
    /// while iterating can result in duplicate rows.
    ///
    /// ```
    /// # #[rust_query::migration::schema(M)]
    /// # pub mod vN {
    /// #     #[index(age)]
    /// #     pub struct User { pub age: i64 }
    /// # }
    /// # use v0::*;
    /// # rust_query::Database::new(rust_query::migration::Config::open_in_memory()).transaction_mut_ok(|mut txn| {
    /// # txn.scoped(|txn|{
    /// # txn.insert_ok(User {age: 30});
    /// for mut user in txn.mutable_vec(User.age(20)) {
    ///     user.age += 1;
    /// }
    /// # })});
    /// ```
    pub fn mutable_vec<'t, T: Table<Schema = S>>(
        &'t mut self,
        val: impl IntoJoinable<'static, S, Typ = TableRow<T>>,
    ) -> Vec<Mutable<'t, T>> {
        let val = val.into_joinable();

        let new_mutable = self.query(|rows| {
            let val = rows.join(val);
            rows.into_iter(val).map(|x| MutTemp::new(x) as _).collect()
        });
        self.tmp = Cell::new(new_mutable);

        Cell::get_mut(&mut self.tmp)
            .iter_mut()
            .map(|x| Mutable::new(&mut **x))
            .collect()
    }
}

pub struct MutTemp<T: Table> {
    pub inner: OnceCell<T::Mutable>,
    pub row_id: TableRow<T>,
}

impl<T: Table> MutTemp<T> {
    pub fn new(row_id: TableRow<T>) -> Box<Self> {
        Box::new(MutTemp {
            inner: OnceCell::new(),
            row_id,
        })
    }
}

impl<T: Table> Drop for MutTemp<T> {
    fn drop(&mut self) {
        if let Some(update) = self.inner.take() {
            let Ok(_) = try_update_private(self.row_id, update) else {
                panic!("mutable can not fail, no unique is updated")
            };
        }
    }
}
