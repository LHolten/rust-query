use std::collections::HashMap;

use ref_cast::RefCast;
use rusqlite::Connection;

use crate::{client::QueryBuilder, db::Col, hash, value::Value, Table};

macro_rules! field {
    ($name:ident: $typ:ty) => {
        pub fn $name(&self) -> Col<$typ, T> {
            Col::new(stringify!($name), self.0.clone())
        }
    };
    ($name:ident($name_str:literal): $typ:ty) => {
        pub fn $name(&self) -> Col<$typ, T> {
            Col::new($name_str, self.0.clone())
        }
    };
}

pub struct Pragma;

pub struct TableList;

#[repr(transparent)]
#[derive(RefCast)]
pub struct TableListDummy<T>(T);

impl<T: Clone> TableListDummy<T> {
    field! {schema: String}
    field! {name: String}
    field! {r#type("type"): String}
    field! {ncol: i64}
    field! {wr: i64}
    field! {strict: i64}
}

impl Table for TableList {
    type Dummy<T> = TableListDummy<T>;
    type Schema = Pragma;

    fn name(&self) -> String {
        "pragma_table_list".to_owned()
    }

    fn typs(_f: &mut crate::TypBuilder) {}
}

pub struct TableInfo(pub String);

#[repr(transparent)]
#[derive(RefCast)]
pub struct TableInfoDummy<T>(T);

impl<T: Clone> TableInfoDummy<T> {
    field! {name: String}
    field! {r#type("type"): String}
    field! {notnull: i64}
    field! {pk: i64}
}

impl Table for TableInfo {
    type Dummy<T> = TableInfoDummy<T>;
    type Schema = Pragma;

    fn name(&self) -> String {
        format!("pragma_table_info('{}', 'main')", self.0)
    }

    fn typs(_f: &mut crate::TypBuilder) {}
}
pub struct ForeignKeyList(pub String);

#[repr(transparent)]
#[derive(RefCast)]
pub struct ForeignKeyListDummy<T>(T);

impl<T: Clone> ForeignKeyListDummy<T> {
    field! {table: String}
    field! {from: String}
    field! {to: String}
}

impl Table for ForeignKeyList {
    type Dummy<T> = ForeignKeyListDummy<T>;
    type Schema = Pragma;

    fn name(&self) -> String {
        format!("pragma_foreign_key_list('{}', 'main')", self.0)
    }

    fn typs(_f: &mut crate::TypBuilder) {}
}

pub struct IndexList(String);

#[repr(transparent)]
#[derive(RefCast)]
pub struct IndexListDummy<T>(T);

impl<T: Clone> IndexListDummy<T> {
    field! {name: String}
    field! {unique: bool}
    field! {origin: String}
    field! {partial: bool}
}

impl Table for IndexList {
    type Dummy<T> = IndexListDummy<T>;
    type Schema = Pragma;

    fn name(&self) -> String {
        format!("pragma_index_list('{}', 'main')", self.0)
    }

    fn typs(_f: &mut crate::TypBuilder) {}
}

pub struct IndexInfo(String);

#[repr(transparent)]
#[derive(RefCast)]
pub struct IndexInfoDummy<T>(T);

impl<T: Clone> IndexInfoDummy<T> {
    field! {name: Option<String>}
}

impl Table for IndexInfo {
    type Dummy<T> = IndexInfoDummy<T>;
    type Schema = Pragma;

    fn name(&self) -> String {
        format!("pragma_index_info('{}', 'main')", self.0)
    }

    fn typs(_f: &mut crate::TypBuilder) {}
}

pub fn read_schema(conn: &Connection) -> hash::Schema {
    #[derive(Clone)]
    struct Column {
        name: String,
        typ: String,
        pk: bool,
        notnull: bool,
    }

    let tables = conn.new_query(|q| {
        let table = q.flat_table(TableList);
        q.filter(table.schema().eq("main"));
        q.filter(table.r#type().eq("table"));
        q.filter(table.name().eq("sqlite_schema").not());
        q.into_vec(|row| row.get(table.name()))
    });

    let mut output = hash::Schema::default();

    for table in tables {
        let mut columns = conn.new_query(|q| {
            let table = q.flat_table(TableInfo(table.clone()));

            q.into_vec(|row| Column {
                name: row.get(table.name()),
                typ: row.get(table.r#type()),
                pk: row.get(table.pk()) != 0,
                notnull: row.get(table.notnull()) != 0,
            })
        });

        let fks: HashMap<_, _> = conn
            .new_query(|q| {
                let fk = q.flat_table(ForeignKeyList(table.to_owned()));
                q.into_vec(|row| {
                    // we just assume that the to column is the primary key..
                    (row.get(fk.from()), row.get(fk.table()))
                })
            })
            .into_iter()
            .collect();

        let make_type = |col: &Column| match col.typ.as_str() {
            "INTEGER" => hash::ColumnType::Integer,
            "TEXT" => hash::ColumnType::String,
            "REAL" => hash::ColumnType::Float,
            t => panic!("unknown type {t}"),
        };

        // we only care about columns that are not a unique id and for which we know the type
        columns.retain(|col| {
            if col.pk {
                assert_eq!(col.name, "id");
                return false;
            }
            true
        });

        let mut table_def = hash::Table::default();
        for col in columns {
            let def = hash::Column {
                fk: fks.get(&col.name).map(|x| (x.clone(), "id".to_owned())),
                typ: make_type(&col),
                name: col.name,
                nullable: !col.notnull,
            };
            table_def.columns.insert(def)
        }

        let uniques = conn.new_query(|q| {
            let index = q.flat_table(IndexList(table.clone()));
            q.filter(index.unique());
            q.filter(index.origin().eq("u"));
            q.filter(index.partial().not());
            q.into_vec(|row| row.get(index.name()))
        });

        for unique in uniques {
            let columns = conn.new_query(|q| {
                let col = q.flat_table(IndexInfo(unique));
                let name = q.filter_some(col.name());
                q.into_vec(|row| row.get(name))
            });

            let mut unique_def = hash::Unique::default();
            for column in columns {
                unique_def.columns.insert(column);
            }
            table_def.uniques.insert(unique_def);
        }

        output.tables.insert((table, table_def))
    }
    output
}
