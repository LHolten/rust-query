use std::{
    collections::{HashMap, HashSet},
    convert::Infallible,
    marker::PhantomData,
    ops::Deref,
};

use sea_query::{Alias, IntoTableRef, TableDropStatement};

use crate::{
    IntoExpr, Lazy, Table, TableRow, Transaction,
    alias::{Scope, TmpTable},
    migrate::new_table_inner,
    transaction::{TXN, try_insert_private},
};

pub trait Migration {
    type FromSchema: 'static;
    type From: Table<Schema = Self::FromSchema>;
    type To: Table<MigrateFrom = Self::From>;
    type Conflict;

    #[doc(hidden)]
    fn prepare(
        val: Self,
        prev: crate::Expr<'static, Self::FromSchema, Self::From>,
    ) -> <Self::To as Table>::Insert;
    #[doc(hidden)]
    fn map_conflict(val: TableRow<Self::From>) -> Self::Conflict;
}

/// Transaction type for use in migrations.
pub struct TransactionMigrate<FromSchema> {
    pub(super) inner: Transaction<FromSchema>,
    pub(super) scope: Scope,
    pub(super) rename_map: HashMap<&'static str, TmpTable>,
    // creating indices is delayed so that they don't need to be renamed
    pub(super) extra_index: Vec<String>,
}

impl<FromSchema> Deref for TransactionMigrate<FromSchema> {
    type Target = Transaction<FromSchema>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<FromSchema: 'static> TransactionMigrate<FromSchema> {
    fn new_table_name<T: Table>(&mut self) -> TmpTable {
        *self.rename_map.entry(T::NAME).or_insert_with(|| {
            let new_table_name = self.scope.tmp_table();
            TXN.with_borrow(|txn| {
                let conn = txn.as_ref().unwrap().get();
                let table = crate::schema::Table::new::<T>();
                new_table_inner(conn, &table, new_table_name);
                self.extra_index.extend(table.create_indices(T::NAME));
            });
            new_table_name
        })
    }

    fn unmigrated<M: Migration<FromSchema = FromSchema>>(
        &self,
        new_name: TmpTable,
    ) -> impl Iterator<Item = TableRow<M::From>> {
        let data = self.inner.query(|rows| {
            let old = rows.join_private::<M::From>();
            rows.into_vec(old)
        });

        let migrated = Transaction::new().query(|rows| {
            let new = rows.join_tmp::<M::To>(new_name);
            rows.into_vec(new)
        });
        let migrated: HashSet<_> = migrated.into_iter().map(|x| x.inner.idx).collect();

        data.into_iter()
            .filter(move |row| !migrated.contains(&row.inner.idx))
    }

    /// Migrate some rows to the new schema.
    ///
    /// This will return an error when there is a conflict.
    /// The error type depends on the number of unique constraints that the
    /// migration can violate:
    /// - 0 => [Infallible]
    /// - 1.. => `TableRow<T::From>` (row in the old table that could not be migrated)
    pub fn migrate_optional<'t, M: Migration<FromSchema = FromSchema>>(
        &'t mut self,
        mut f: impl FnMut(Lazy<'t, M::From>) -> Option<M>,
    ) -> Result<(), M::Conflict> {
        let new_name = self.new_table_name::<M::To>();

        for row in self.unmigrated::<M>(new_name) {
            if let Some(new) = f(self.lazy(row)) {
                try_insert_private::<M::To>(
                    new_name.into_table_ref(),
                    Some(row.inner.idx),
                    M::prepare(new, row.into_expr()),
                )
                .map_err(|_| M::map_conflict(row))?;
            };
        }
        Ok(())
    }

    /// Migrate all rows to the new schema.
    ///
    /// Conflict errors work the same as in [Self::migrate_optional].
    ///
    /// However, this method will return [Migrated] when all rows are migrated.
    /// This can then be used as proof that there will be no foreign key violations.
    pub fn migrate<'t, M: Migration<FromSchema = FromSchema>>(
        &'t mut self,
        mut f: impl FnMut(Lazy<'t, M::From>) -> M,
    ) -> Result<Migrated<'static, FromSchema, M::To>, M::Conflict> {
        self.migrate_optional::<M>(|x| Some(f(x)))?;

        Ok(Migrated {
            _p: PhantomData,
            f: Box::new(|_| {}),
            _local: PhantomData,
        })
    }

    /// Helper method for [Self::migrate].
    ///
    /// It can only be used when the migration is known to never cause unique constraint conflicts.
    pub fn migrate_ok<'t, M: Migration<FromSchema = FromSchema, Conflict = Infallible>>(
        &'t mut self,
        f: impl FnMut(Lazy<'t, M::From>) -> M,
    ) -> Migrated<'static, FromSchema, M::To> {
        let Ok(res) = self.migrate(f);
        res
    }
}

/// [Migrated] provides a proof of migration.
///
/// This only needs to be provided for tables that are migrated from a previous table.
pub struct Migrated<'t, FromSchema, T> {
    _p: PhantomData<T>,
    f: Box<dyn 't + FnOnce(&mut SchemaBuilder<'t, FromSchema>)>,
    _local: PhantomData<*const ()>,
}

impl<'t, FromSchema: 'static, T: Table> Migrated<'t, FromSchema, T> {
    /// Don't migrate the remaining rows.
    ///
    /// This can cause foreign key constraint violations, which is why an error callback needs to be provided.
    pub fn map_fk_err(err: impl 't + FnOnce() -> Infallible) -> Self {
        Self {
            _p: PhantomData,
            f: Box::new(|x| x.foreign_key::<T>(err)),
            _local: PhantomData,
        }
    }

    #[doc(hidden)]
    pub fn apply(self, b: &mut SchemaBuilder<'t, FromSchema>) {
        (self.f)(b)
    }
}

pub struct SchemaBuilder<'t, FromSchema> {
    pub(super) inner: TransactionMigrate<FromSchema>,
    pub(super) drop: Vec<TableDropStatement>,
    pub(super) foreign_key: HashMap<&'static str, Box<dyn 't + FnOnce() -> Infallible>>,
}

impl<'t, FromSchema: 'static> SchemaBuilder<'t, FromSchema> {
    pub fn foreign_key<To: Table>(&mut self, err: impl 't + FnOnce() -> Infallible) {
        self.inner.new_table_name::<To>();

        self.foreign_key.insert(To::NAME, Box::new(err));
    }

    pub fn create_empty<To: Table>(&mut self) {
        self.inner.new_table_name::<To>();
    }

    pub fn drop_table<T: Table>(&mut self) {
        let name = Alias::new(T::NAME);
        let step = sea_query::Table::drop().table(name).take();
        self.drop.push(step);
    }
}
