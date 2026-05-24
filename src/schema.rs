//! This can be used to define the layout of a table
//! The layout is hashable and the hashes are independent
//! of the column ordering and some other stuff.

pub mod canonical;
mod check_constraint;
mod diff;
pub mod from_db;
pub mod from_macro;
pub mod read;
#[cfg(test)]
mod test;
pub mod tokenizer;

use crate::{
    lower::{
        self, emit,
        list_writer::{Alias, ListWriter},
    },
    schema::{
        canonical::ColumnType,
        from_macro::{Schema, Table},
    },
};

impl ColumnType {
    pub fn rusqlite_type(&self) -> &'static str {
        match self {
            ColumnType::Integer => "INTEGER",
            ColumnType::Real => "REAL",
            ColumnType::Text => "TEXT",
            ColumnType::Blob => "BLOB",
            ColumnType::Unknown(_) => unreachable!(),
        }
    }
}

mod normalize {
    use crate::schema::{canonical, from_db};

    impl from_db::Index {
        pub fn normalize(self) -> Option<canonical::Unique> {
            self.unique.then_some(canonical::Unique {
                columns: self.columns.into_iter().collect(),
            })
        }
    }

    #[cfg(feature = "dev")]
    impl crate::schema::from_macro::Table {
        fn normalize(self) -> canonical::Table {
            canonical::Table {
                columns: self.columns.into_iter().map(|(k, v)| (k, v.def)).collect(),
                indices: self
                    .indices
                    .into_iter()
                    .filter_map(|idx| idx.def.normalize())
                    .collect(),
            }
        }
    }

    #[cfg(feature = "dev")]
    impl crate::schema::from_macro::Schema {
        pub(crate) fn normalize(self) -> canonical::Schema {
            canonical::Schema {
                tables: self
                    .tables
                    .into_iter()
                    .map(|(k, v)| (k.to_owned(), v.normalize()))
                    .collect(),
            }
        }
    }
}

impl Table {
    pub(crate) fn new<T: crate::Table>() -> Self {
        let mut f = crate::schema::from_macro::TypBuilder::default();
        T::typs(&mut f);
        f.ast.span = T::SPAN;
        f.ast
    }
}

impl Schema {
    pub(crate) fn new<S: crate::migrate::Schema>() -> Self {
        let mut b = crate::migrate::TableTypBuilder::default();
        S::typs(&mut b);
        b.ast.span = S::SPAN;
        b.ast
    }
}

impl Table {
    pub fn to_db(self) -> from_db::Table {
        from_db::Table {
            columns: self
                .columns
                .into_iter()
                .map(|(name, col)| (name, col.def))
                .collect(),
            indices: self.indices.into_iter().map(|idx| idx.def).collect(),
        }
    }
}

impl from_db::Table {
    pub fn create(&self, table: lower::JoinableTable, primary: &'static str) -> String {
        let mut stmt = emit::Stmt::default();

        stmt.write("CREATE TABLE ");
        table.emit(&mut stmt);

        stmt.write(" (");
        let mut list = ListWriter::new(&mut stmt, ", ");
        list.item()
            .write(Alias(primary))
            .write(" INTEGER PRIMARY KEY");
        for (name, col) in &self.columns {
            let item = list.item().write(Alias(&name));
            item.write(" ").write(col.typ.rusqlite_type());
            if !col.nullable {
                item.write(" NOT NULL");
            }
            if let Some(check) = &col.check {
                item.write(format!(" CHECK ({check})"));
            }
            if let Some((table, fk)) = &col.fk {
                item.write(format!(" REFERENCES {} ({})", Alias(table), Alias(fk)));
            }
        }
        for index in &self.indices {
            // only unique indexes are allows on table definitions.
            // by making these part of the table, we don't need to rename them
            // after the migration
            if index.unique {
                let item = list.item().write("UNIQUE (");
                // Write columns in original order to allow user to control it for optimization.
                let mut unique_list = ListWriter::new(item, ", ");
                for col in &index.columns {
                    unique_list.item().write(Alias(col));
                }
                item.write(")");
                // TODO: check what happens if there are no columns in the unique constraint.
            }
        }
        stmt.write(") STRICT");
        assert!(stmt.params.is_empty());
        stmt.sql
    }

    /// This gives the sql to create the remaining non unique indices
    /// Indices can not be renamed in sqlite.
    /// These are named, so we delay creating them until after the old indices
    /// are deleted.
    pub fn delayed_indices(&self, table_name: &str) -> impl Iterator<Item = String> {
        self.indices
            .iter()
            .filter(|x| !x.unique)
            .enumerate()
            .map(move |(index_num, index)| {
                let stmt =
                    index.create_not_unique(&format!("{table_name}_index_{index_num}"), table_name);
                assert!(stmt.params.is_empty());
                stmt.sql
            })
    }
}

impl from_db::Index {
    pub fn create_not_unique(&self, index_name: &str, table_name: &str) -> emit::Stmt {
        assert!(!self.unique);

        let mut stmt = emit::Stmt::default();
        stmt.write("CREATE INDEX ")
            .write(Alias(index_name))
            .write(" ON ")
            .write(Alias(table_name))
            .write(" (");
        // Preserve the original order of columns in the unique constraint.
        // This lets users optimize queries by using index prefixes.
        let mut list = ListWriter::new(&mut stmt, ", ");
        for col in &self.columns {
            list.item().write(Alias(col));
        }
        stmt.write(")");
        // TODO: check what happens if there are no columns in the index
        stmt
    }
}

#[cfg(feature = "dev")]
pub mod dev {
    use std::{
        hash::{Hash, Hasher},
        io::{Read, Write},
    };

    use k12::{
        KangarooTwelve, KangarooTwelveCore,
        digest::{ExtendableOutput, core_api::CoreWrapper},
    };

    pub struct KangarooHasher {
        inner: CoreWrapper<KangarooTwelveCore<'static>>,
    }

    impl Default for KangarooHasher {
        fn default() -> Self {
            let core = KangarooTwelveCore::new(&[]);
            let hasher = KangarooTwelve::from_core(core);
            Self { inner: hasher }
        }
    }

    impl Hasher for KangarooHasher {
        fn finish(&self) -> u64 {
            let mut xof = self.inner.clone().finalize_xof();
            let mut buf = [0; 8];
            xof.read_exact(&mut buf).unwrap();
            u64::from_le_bytes(buf)
        }

        fn write(&mut self, bytes: &[u8]) {
            self.inner.write_all(bytes).unwrap();
        }
    }

    /// Calculate the hash of a shema.
    ///
    /// This is useful in a test to make sure that old schema versions are not accidentally modified.
    pub fn hash_schema<S: crate::migrate::Schema>() -> String {
        let mut hasher = KangarooHasher::default();
        super::Schema::new::<S>().normalize().hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }
}
