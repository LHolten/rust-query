use std::{
    collections::{BTreeMap, HashMap},
    convert::Infallible,
    marker::PhantomData,
    ops::Deref,
};

use crate::{
    Lazy, Table, TableRow, Transaction, aggregate,
    lower::{self, list_writer::Alias},
    transaction::try_insert_private,
};

pub trait Migrateable: Table<MigrateFrom: Table<Schema = Self::FromSchema>> {
    type Migration;
    type FromSchema;
    type MigrateConflict;

    #[doc(hidden)]
    fn prepare(val: Self::Migration, prev: Lazy<'_, Self::MigrateFrom>) -> Self;
    #[doc(hidden)]
    fn map_conflict(val: TableRow<Self::MigrateFrom>) -> Self::MigrateConflict;
}

/// Transaction type for use in migrations.
pub struct TransactionMigrate<FromSchema> {
    pub(super) inner: Transaction<FromSchema>,
    pub(super) scope: lower::Scope,
    pub(super) rename_map: HashMap<&'static str, lower::TmpTable>,
    // creating non unique indices is delayed so that they don't need to be renamed
    pub(super) extra_index: Vec<String>,
}

impl<FromSchema> Deref for TransactionMigrate<FromSchema> {
    type Target = Transaction<FromSchema>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// This type is used to specify what should happen with a row during migration.
#[non_exhaustive]
pub enum MigrateWith<'t, M> {
    /// The row should be migrated to have this new value.
    New(M),
    #[doc(hidden)]
    Remove(FkErrHandler<'t>),
}

pub(crate) struct FkErrHandler<'t>(pub Box<dyn 't + FnOnce() -> Infallible>);

impl<'t, M> MigrateWith<'t, M> {
    /// The row should be removed.
    ///
    /// The closure is called when there is a foreign key error due to the row being removed.
    pub fn remove_or_else(f: impl 't + FnOnce() -> Infallible) -> Self {
        Self::Remove(FkErrHandler(Box::new(f)))
    }
}

impl<'t, M: Migrateable<MigrateFrom: Table<Referer = Infallible>>> MigrateWith<'t, M> {
    /// The row should be removed and the table has the `#[no_reference]` attribute.
    pub fn remove() -> Self {
        Self::remove_or_else(|| unreachable!("there are no foreign keys to this table"))
    }
}

impl<FromSchema: 'static> TransactionMigrate<FromSchema> {
    fn new_table_name<T: Table>(&mut self) -> lower::TmpTable {
        *self.rename_map.entry(T::NAME).or_insert_with(|| {
            let new_table_name = self.scope.tmp_table();
            let table = crate::schema::from_macro::Table::new::<T>().to_db();
            self.inner
                .execute(&table.create(lower::JoinableTable::Tmp(new_table_name)));
            self.extra_index.extend(table.delayed_indices(T::NAME));
            new_table_name
        })
    }

    fn unmigrated<T: Migrateable<FromSchema = FromSchema>>(
        &self,
        new_name: lower::TmpTable,
    ) -> impl Iterator<Item = TableRow<T::MigrateFrom>> {
        self.inner.query(|rows| {
            let old = rows.join_private::<T::MigrateFrom>();
            rows.filter(aggregate(|rows| {
                let new = rows.join_tmp::<T::MigrateFrom>(new_name);
                rows.filter(new.eq(&old));
                rows.exists().not()
            }));
            rows.into_iter(old)
        })
    }

    /// Migrate some rows to the new schema.
    ///
    /// This will return an error when there is a conflict.
    /// The error type depends on the number of unique constraints that the
    /// migration can violate:
    /// - 0 => [Infallible]
    /// - 1.. => [TableRow] (row in the old table that could not be migrated)
    ///
    /// The closure should return [MigrateWith] to indicate what should happen with each row.
    pub fn migrate_optional<'t, 'x, T: Migrateable<FromSchema = FromSchema>>(
        &'t mut self,
        mut f: impl FnMut(Lazy<'t, T::MigrateFrom>) -> MigrateWith<'x, T::Migration>,
    ) -> Result<Migrated<'x, T>, T::MigrateConflict> {
        let new_name = self.new_table_name::<T>();

        let mut error_map = BTreeMap::new();

        // We will do insertions here while retrieving rows from the database.
        // This is fine because we do not care if the query uses old or new data.
        // The only problematic case is if sqlite decides to repeat a returned row.
        // That would be very strange though, since we are not updating the old table.
        // See https://sqlite.org/isolation.html for more information.
        for row in self.unmigrated::<T>(new_name) {
            match f(self.lazy(row)) {
                MigrateWith::New(new) => {
                    // TODO: deduplicate this self.lazy call
                    let val = T::prepare(new, self.lazy(row));
                    try_insert_private::<T>(
                        lower::JoinableTable::Tmp(new_name),
                        Some(row.inner.idx),
                        val,
                    )
                    .map_err(|_| T::map_conflict(row))?;
                }
                MigrateWith::Remove(fn_once) => {
                    error_map.insert(row.inner.idx, fn_once);
                }
            };
        }

        Ok(Migrated {
            _p: PhantomData,
            f: Box::new(|b| {
                b.foreign_key::<T>(error_map);
            }),
            _local: PhantomData,
        })
    }

    /// Migrate all rows to the new schema.
    ///
    /// Same as [Self::migrate_optional], but it does not require wrapping all migrated
    /// rows in [MigrateWith::New].
    ///
    /// This is most likely the variant that you want to use, unless you have a table without
    /// unique constraint, see [Self::migrate_ok].
    pub fn migrate<'t, T: Migrateable<FromSchema = FromSchema>>(
        &'t mut self,
        mut f: impl FnMut(Lazy<'t, T::MigrateFrom>) -> T::Migration,
    ) -> Result<Migrated<'static, T>, T::MigrateConflict> {
        self.migrate_optional(|x| MigrateWith::New(f(x)))
    }

    /// Migrate all rows to the new schema, without unique constraint conflicts.
    ///
    /// Same as [Self::migrate], but can only be used when the migration is known to
    /// never cause unique constraint conflicts.
    pub fn migrate_ok<'t, T: Migrateable<FromSchema = FromSchema, MigrateConflict = Infallible>>(
        &'t mut self,
        f: impl FnMut(Lazy<'t, T::MigrateFrom>) -> T::Migration,
    ) -> Migrated<'static, T> {
        let Ok(res) = self.migrate(f);
        res
    }
}

/// [Migrated] provides a proof of migration.
///
/// This only needs to be provided for tables that are migrated from a previous table.
pub struct Migrated<'t, T: Migrateable> {
    _p: PhantomData<T>,
    f: Box<dyn 't + FnOnce(&mut SchemaBuilder<'t, T::FromSchema>)>,
    _local: PhantomData<*const ()>,
}

impl<'t, T: Migrateable> Migrated<'t, T> {
    #[doc(hidden)]
    pub fn apply(self, b: &mut SchemaBuilder<'t, T::FromSchema>) {
        (self.f)(b)
    }
}

pub struct SchemaBuilder<'t, FromSchema> {
    pub(super) inner: TransactionMigrate<FromSchema>,
    pub(super) drop: Vec<String>,
    pub(super) foreign_key: HashMap<&'static str, BTreeMap<i64, FkErrHandler<'t>>>,
}

impl<'t, FromSchema: 'static> SchemaBuilder<'t, FromSchema> {
    pub fn foreign_key<To: Table>(&mut self, err: BTreeMap<i64, FkErrHandler<'t>>) {
        self.inner.new_table_name::<To>();

        self.foreign_key.insert(To::NAME, err);
    }

    pub fn create_empty<To: Table>(&mut self) {
        self.inner.new_table_name::<To>();
    }

    pub fn drop_table<T: Table>(&mut self) {
        self.drop.push(format!("DROP TABLE {}", Alias(T::NAME)));
    }
}
