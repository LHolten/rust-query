use std::{collections::HashMap, convert::Infallible, marker::PhantomData, ops::Deref};

use crate::{
    Lazy, Table, TableRow, Transaction, aggregate,
    lower::{self, JoinableTableWithId, list_writer::Alias},
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
pub struct TransactionMigrate<FromSchema: 'static> {
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

impl<FromSchema: 'static> TransactionMigrate<FromSchema> {
    fn new_table_name<T: Table>(&mut self) -> lower::TmpTable {
        *self.rename_map.entry(T::NAME).or_insert_with(|| {
            let new_table_name = self.scope.tmp_table();
            let table = crate::schema::from_macro::Table::new::<T>().into_db();
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
                // manually construct Joinable because we are using the new definition in the old schema.
                // the result type is also the old type even though it represents the new table.
                let new = rows.join(crate::private::Joinable::new(JoinableTableWithId {
                    name: lower::JoinableTable::Tmp(new_name),
                    main_column: <T as Table>::ID,
                }));
                rows.filter(old.eq(&new));
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
    /// The closure returns [Option] to indicate if each row must be kept.
    pub fn migrate_optional<'t, T: Migrateable<FromSchema = FromSchema>>(
        &'t mut self,
        mut f: impl FnMut(Lazy<'t, T::MigrateFrom>) -> Option<T::Migration>,
    ) -> Result<MigratedOptional<T>, T::MigrateConflict> {
        let new_name = self.new_table_name::<T>();

        // We will do insertions here while retrieving rows from the database.
        // This is fine because we do not care if the query uses old or new data.
        // The only problematic case is if sqlite decides to repeat a returned row.
        // That would be very strange though, since we are not updating the old table.
        // See https://sqlite.org/isolation.html for more information.
        for row in self.unmigrated::<T>(new_name) {
            if let Some(new) = f(self.lazy(row)) {
                // TODO: deduplicate this self.lazy call
                let val = T::prepare(new, self.lazy(row));
                try_insert_private::<T>(
                    lower::JoinableTable::Tmp(new_name),
                    Some(row.inner.idx),
                    val,
                )
                .map_err(|_| T::map_conflict(row))?;
            };
        }

        Ok(MigratedOptional { inner: PhantomData })
    }

    /// Migrate all rows to the new schema.
    ///
    /// Same as [Self::migrate_optional], but it does not require wrapping all migrated
    /// rows in [Some].
    ///
    /// This is most likely the variant that you want to use, unless you have a table without
    /// unique constraint, see [Self::migrate_ok].
    pub fn migrate<'t, T: Migrateable<FromSchema = FromSchema>>(
        &'t mut self,
        mut f: impl FnMut(Lazy<'t, T::MigrateFrom>) -> T::Migration,
    ) -> Result<Migrated<'static, T>, T::MigrateConflict> {
        self.migrate_optional(|x| Some(f(x)))
            .map(|x| x.map_fk_err(|| unreachable!("all rows are migrated")))
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
    f: FkErrHandler<'t>,
    _local: PhantomData<*const ()>,
}

impl<'t, To: Migrateable> Migrated<'t, To> {
    #[doc(hidden)]
    pub fn apply(self, b: &mut SchemaBuilder<'t, To::FromSchema>) {
        b.foreign_key::<To>(self.f);
    }
}

pub struct SchemaBuilder<'t, FromSchema: 'static> {
    pub(super) inner: TransactionMigrate<FromSchema>,
    pub(super) drop: Vec<String>,
    pub(super) foreign_key: HashMap<&'static str, FkErrHandler<'t>>,
}

impl<'t, FromSchema: 'static> SchemaBuilder<'t, FromSchema> {
    pub fn foreign_key<To: Table>(&mut self, err: FkErrHandler<'t>) {
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

/// Proof that a table is at least partially migrated.
///
/// This type can be turned into [Migrated] by providing an error
/// handler.
pub struct MigratedOptional<T: Migrateable> {
    inner: PhantomData<Migrated<'static, T>>,
}

impl<T: Migrateable> MigratedOptional<T> {
    /// The closure is called when there is a foreign key error due to some row being removed.
    pub fn map_fk_err<'t>(self, f: impl 't + FnOnce() -> Infallible) -> Migrated<'t, T> {
        Migrated {
            _p: PhantomData,
            f: FkErrHandler(Box::new(f)),
            _local: PhantomData,
        }
    }
}

impl<T: Migrateable<Referer = Infallible>> MigratedOptional<T> {
    /// The table has the `#[no_reference]` attribute, so partial migration is always ok.
    pub fn no_reference(self) -> Migrated<'static, T> {
        self.map_fk_err(|| unreachable!("no references exist to this table"))
    }
}

pub(crate) struct FkErrHandler<'t>(pub Box<dyn 't + FnOnce() -> Infallible>);
