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

use sea_query::{Alias, IndexCreateStatement, SqliteQueryBuilder, TableCreateStatement};

use crate::schema::{
    canonical::ColumnType,
    from_macro::{Index, Schema, Table},
};

impl ColumnType {
    pub fn sea_type(&self) -> sea_query::ColumnType {
        use sea_query::ColumnType as T;
        match self {
            ColumnType::Integer => T::Integer,
            ColumnType::Real => T::custom("REAL"),
            ColumnType::Text => T::Text,
            ColumnType::Blob => T::Blob,
            ColumnType::Any => T::custom("ANY"),
        }
    }
}

mod normalize {
    use crate::schema::{
        canonical, from_db,
        from_macro::{Schema, Table},
    };

    impl from_db::Index {
        pub fn normalize(self) -> Option<canonical::Unique> {
            self.unique.then_some(canonical::Unique {
                columns: self.columns.into_iter().collect(),
            })
        }
    }

    impl Table {
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

    impl Schema {
        pub(crate) fn normalize(self) -> canonical::Schema {
            canonical::Schema {
                tables: self
                    .tables
                    .into_iter()
                    .map(|(k, v)| (k, v.normalize()))
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
    pub fn create(&self) -> TableCreateStatement {
        use sea_query::*;
        let mut create = Table::create();
        for (name, col) in &self.columns {
            let col = &col.def;
            let name = Alias::new(name);
            let mut def = ColumnDef::new_with_type(name.clone(), col.typ.sea_type());
            if col.nullable {
                def.null();
            } else {
                def.not_null();
            }
            if let Some(check) = &col.check {
                def.check(sea_query::Expr::cust(check.clone()));
            }
            create.col(&mut def);
            if let Some((table, fk)) = &col.fk {
                create.foreign_key(
                    ForeignKey::create()
                        .to(Alias::new(table), Alias::new(fk))
                        .from_col(name),
                );
            }
        }
        create
    }

    pub fn create_indices(&self, table_name: &str) -> impl Iterator<Item = String> {
        let index_table_ref = Alias::new(table_name);
        self.indices
            .iter()
            .enumerate()
            .map(move |(index_num, index)| {
                index
                    .create()
                    .table(index_table_ref.clone())
                    .name(format!("{table_name}_index_{index_num}"))
                    .to_string(SqliteQueryBuilder)
            })
    }
}

impl Index {
    pub fn create(&self) -> IndexCreateStatement {
        let mut index = sea_query::Index::create();
        if self.def.unique {
            index.unique();
        }
        // Preserve the original order of columns in the unique constraint.
        // This lets users optimize queries by using index prefixes.
        for col in &self.def.columns {
            index.col(Alias::new(col));
        }
        index
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
