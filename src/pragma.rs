use std::collections::HashMap;

use rusqlite::Connection;

use crate::{
    client::QueryBuilder,
    hash,
    value::{Db, Value},
    Builder, Table,
};

pub struct Pragma;

pub struct TableList;

pub struct TableListDummy<'a> {
    pub schema: Db<'a, String>,
    pub name: Db<'a, String>,
    pub r#type: Db<'a, String>,
    pub ncol: Db<'a, i64>,
    pub wr: Db<'a, i64>,
    pub strict: Db<'a, i64>,
}

impl Table for TableList {
    type Dummy<'names> = TableListDummy<'names>;
    type Schema = Pragma;

    fn name(&self) -> String {
        "pragma_table_list".to_owned()
    }

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        TableListDummy {
            schema: f.col("schema"),
            name: f.col("name"),
            r#type: f.col("type"),
            ncol: f.col("ncol"),
            wr: f.col("wr"),
            strict: f.col("strict"),
        }
    }

    fn typs(_f: &mut crate::TypBuilder) {}
}

pub struct TableInfo(pub String);

pub struct TableInfoDummy<'a> {
    pub name: Db<'a, String>,
    pub r#type: Db<'a, String>,
    pub notnull: Db<'a, i64>,
    pub pk: Db<'a, i64>,
}

impl Table for TableInfo {
    type Dummy<'t> = TableInfoDummy<'t>;
    type Schema = Pragma;

    fn name(&self) -> String {
        format!("pragma_table_info('{}', 'main')", self.0)
    }

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        TableInfoDummy {
            name: f.col("name"),
            r#type: f.col("type"),
            notnull: f.col("notnull"),
            pk: f.col("pk"),
        }
    }

    fn typs(_f: &mut crate::TypBuilder) {}
}
pub struct ForeignKeyList(pub String);

pub struct ForeignKeyListDummy<'a> {
    pub table: Db<'a, String>,
    pub from: Db<'a, String>,
    pub to: Db<'a, String>,
}

impl Table for ForeignKeyList {
    type Dummy<'t> = ForeignKeyListDummy<'t>;
    type Schema = Pragma;

    fn name(&self) -> String {
        format!("pragma_foreign_key_list('{}', 'main')", self.0)
    }

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        ForeignKeyListDummy {
            table: f.col("table"),
            from: f.col("from"),
            to: f.col("to"),
        }
    }

    fn typs(_f: &mut crate::TypBuilder) {}
}

pub struct IndexList(String);

pub struct IndexListDummy<'a> {
    pub name: Db<'a, String>,
    pub unique: Db<'a, bool>,
    pub origin: Db<'a, String>,
    pub partial: Db<'a, bool>,
}

impl Table for IndexList {
    type Dummy<'t> = IndexListDummy<'t>;
    type Schema = Pragma;

    fn name(&self) -> String {
        format!("pragma_index_list('{}', 'main')", self.0)
    }

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        IndexListDummy {
            name: f.col("name"),
            unique: f.col("unique"),
            origin: f.col("origin"),
            partial: f.col("partial"),
        }
    }

    fn typs(_f: &mut crate::TypBuilder) {}
}

pub struct IndexInfo(String);

pub struct IndexInfoDummy<'a> {
    pub name: Db<'a, Option<String>>,
}

impl Table for IndexInfo {
    type Dummy<'t> = IndexInfoDummy<'t>;
    type Schema = Pragma;

    fn name(&self) -> String {
        format!("pragma_index_info('{}', 'main')", self.0)
    }

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        IndexInfoDummy {
            name: f.col("name"),
        }
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
        q.filter(table.schema.eq("main"));
        q.filter(table.r#type.eq("table"));
        q.filter(table.name.eq("sqlite_schema").not());
        q.into_vec(u32::MAX, |row| row.get(&table.name))
    });

    let mut output = hash::Schema::default();

    for table in tables {
        let mut columns = conn.new_query(|q| {
            let table = q.flat_table(TableInfo(table.clone()));

            q.into_vec(u32::MAX, |row| Column {
                name: row.get(table.name),
                typ: row.get(table.r#type),
                pk: row.get(table.pk) != 0,
                notnull: row.get(table.notnull) != 0,
            })
        });

        let fks: HashMap<_, _> = conn
            .new_query(|q| {
                let fk = q.flat_table(ForeignKeyList(table.to_owned()));
                q.into_vec(u32::MAX, |row| {
                    // we just assume that the to column is the primary key..
                    (row.get(fk.from), row.get(fk.table))
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
            q.filter(index.unique);
            q.filter(index.origin.eq("u"));
            q.filter(index.partial.not());
            q.into_vec(u32::MAX, |row| row.get(index.name))
        });

        for unique in uniques {
            let columns = conn.new_query(|q| {
                let col = q.flat_table(IndexInfo(unique));
                let name = q.filter_some(&col.name);
                q.into_vec(u32::MAX, |row| row.get(name))
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
