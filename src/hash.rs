//! This can be used to define the layout of a table
//! The layout is hashable and the hashes are independent
//! of the column ordering and some other stuff.

use std::{collections::BTreeMap, marker::PhantomData};

use sea_query::TableCreateStatement;

use crate::value::{EqTyp, MyTyp};

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum ColumnType {
    Integer = 0,
    Float = 1,
    String = 2,
    Blob = 3,
}

impl ColumnType {
    pub fn sea_type(&self) -> sea_query::ColumnType {
        use sea_query::ColumnType as T;
        match self {
            ColumnType::Integer => T::Integer,
            ColumnType::Float => T::custom("REAL"),
            ColumnType::String => T::Text,
            ColumnType::Blob => T::Blob,
        }
    }
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Column {
    pub typ: ColumnType,
    pub nullable: bool,
    pub fk: Option<(String, String)>,
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Index {
    // column order matters for performance
    pub columns: Vec<String>,
    pub unique: bool,
}

impl Index {
    fn normalize(&mut self) -> bool {
        // column order doesn't matter for correctness
        self.columns.sort();
        // non-unique indexes don't matter for correctness
        self.unique
    }
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Table {
    pub columns: BTreeMap<String, Column>,
    pub indices: Vec<Index>,
}

impl Table {
    pub(crate) fn new<T: crate::Table>() -> Self {
        let mut f = crate::hash::TypBuilder::default();
        T::typs(&mut f);
        f.ast
    }

    fn normalize(&mut self) {
        self.indices.retain_mut(Index::normalize);
        self.indices.sort();
    }
}

impl Table {
    pub fn create(&self) -> TableCreateStatement {
        use sea_query::*;
        let mut create = Table::create();
        for (name, col) in &self.columns {
            let name = Alias::new(name);
            let mut def = ColumnDef::new_with_type(name.clone(), col.typ.sea_type());
            if col.nullable {
                def.null();
            } else {
                def.not_null();
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
        for index_spec in &*self.indices {
            let mut index = sea_query::Index::create();
            if index_spec.unique {
                index.unique();
            }
            // Preserve the original order of columns in the unique constraint.
            // This lets users optimize queries by using index prefixes.
            for col in &index_spec.columns {
                index.col(Alias::new(col));
            }
            create.index(&mut index);
        }
        create
    }
}

#[derive(Debug, Hash, Default, PartialEq, Eq)]
pub struct Schema {
    pub tables: BTreeMap<String, Table>,
}

impl Schema {
    pub(crate) fn new<S: crate::migrate::Schema>() -> Self {
        let mut b = crate::migrate::TableTypBuilder::default();
        S::typs(&mut b);
        b.ast
    }

    pub(crate) fn normalize(mut self) -> Self {
        self.tables.values_mut().for_each(Table::normalize);
        self
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

pub struct TypBuilder<S> {
    pub(crate) ast: Table,
    _p: PhantomData<S>,
}

impl<S> Default for TypBuilder<S> {
    fn default() -> Self {
        Self {
            ast: Default::default(),
            _p: Default::default(),
        }
    }
}

impl<S> TypBuilder<S> {
    pub fn col<T: SchemaType<S>>(&mut self, name: &'static str) {
        let mut item = Column {
            typ: T::TYP,
            nullable: T::NULLABLE,
            fk: None,
        };
        if let Some((table, fk)) = T::FK {
            item.fk = Some((table.to_owned(), fk.to_owned()))
        }
        let old = self.ast.columns.insert(name.to_owned(), item);
        debug_assert!(old.is_none());
    }

    pub fn index(&mut self, cols: &[&'static str], unique: bool) {
        let mut index = Index {
            columns: Vec::default(),
            unique,
        };
        for &col in cols {
            index.columns.push(col.to_owned());
        }
        self.ast.indices.push(index);
    }

    pub fn check_unique_compatible<T: EqTyp>(&mut self) {}
}

struct Null;
struct NotNull;

// TODO: maybe remove this trait?
// currently this prevents storing booleans and nested `Option`.
#[diagnostic::on_unimplemented(
    message = "Can not use `{Self}` as a column type in schema `{S}`",
    note = "Table names can be used as schema column types as long as they are not #[no_reference]"
)]
trait SchemaType<S>: MyTyp {
    type N;
}

impl<S> SchemaType<S> for String {
    type N = NotNull;
}
impl<S> SchemaType<S> for Vec<u8> {
    type N = NotNull;
}
impl<S> SchemaType<S> for i64 {
    type N = NotNull;
}
impl<S> SchemaType<S> for f64 {
    type N = NotNull;
}
impl<S, T: SchemaType<S, N = NotNull>> SchemaType<S> for Option<T> {
    type N = Null;
}
// only tables with `Referer = ()` are valid columns
#[diagnostic::do_not_recommend]
impl<T: crate::Table<Referer = ()>> SchemaType<T::Schema> for T {
    type N = NotNull;
}
