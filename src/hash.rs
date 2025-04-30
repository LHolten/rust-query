//! This can be used to define the layout of a table
//! The layout is hashable and the hashes are independent
//! of the column ordering and some other stuff.

use std::{marker::PhantomData, ops::Deref};

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
    pub name: String,
    pub typ: ColumnType,
    pub nullable: bool,
    pub fk: Option<(String, String)>,
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Unique {
    pub columns: MyVec<String>,
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Table {
    pub columns: MyVec<Column>,
    pub uniques: MyVec<Unique>,
}

/// Special [Vec] wrapper with a hash that is independent of the item order
#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct MyVec<T> {
    inner: Vec<T>,
}

impl<T> Default for MyVec<T> {
    fn default() -> Self {
        Self {
            inner: Default::default(),
        }
    }
}

impl<T: Ord> MyVec<T> {
    pub fn insert(&mut self, item: T) {
        let index = self.inner.partition_point(|x| x < &item);
        self.inner.insert(index, item);
    }
}

impl<T> Deref for MyVec<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl Table {
    pub fn create(&self) -> TableCreateStatement {
        use sea_query::*;
        let mut create = Table::create();
        for col in &*self.columns {
            let name = Alias::new(&col.name);
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
        for unique in &*self.uniques {
            let mut index = sea_query::Index::create().unique().take();
            for col in &*unique.columns {
                index.col(Alias::new(col));
            }
            create.index(&mut index);
        }
        create
    }
}

#[derive(Debug, Hash, Default, PartialEq, Eq)]
pub struct Schema {
    pub tables: MyVec<(String, Table)>,
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
        let mut b = crate::migrate::TableTypBuilder::default();
        S::typs(&mut b);
        let mut hasher = KangarooHasher::default();
        b.ast.hash(&mut hasher);
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
            name: name.to_owned(),
            typ: T::TYP,
            nullable: T::NULLABLE,
            fk: None,
        };
        if let Some((table, fk)) = T::FK {
            item.fk = Some((table.to_owned(), fk.to_owned()))
        }
        self.ast.columns.insert(item)
    }

    pub fn unique(&mut self, cols: &[&'static str]) {
        let mut unique = Unique::default();
        for &col in cols {
            unique.columns.insert(col.to_owned());
        }
        self.ast.uniques.insert(unique);
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
