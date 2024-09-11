use std::collections::HashMap;

use ref_cast::RefCast;

use crate::{client::QueryBuilder, db::Col, from_row::AdHoc, hash, Table};

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

pub struct SchemaList;

#[repr(transparent)]
#[derive(RefCast)]
pub struct SchemaListDummy<T>(T);

impl<T: Clone> SchemaListDummy<T> {
    field! {typ("type"): String}
    field! {tbl_name: String}
    field! {sql: Option<String>}
}

impl Table for SchemaList {
    type Dummy<T> = SchemaListDummy<T>;
    type Schema = Pragma;

    fn name(&self) -> String {
        "sqlite_schema".to_owned()
    }

    fn typs(_f: &mut crate::TypBuilder) {}
}

#[derive(PartialEq)]
enum ItemTyp {
    Index,
    Table,
}

pub fn strip_str(name: &str) -> &str {
    name.strip_prefix('"').unwrap().strip_suffix('"').unwrap()
}

pub fn read_schema(conn: &rusqlite::Transaction) -> hash::Schema {
    struct ItemDef {
        typ: ItemTyp,
        tbl_name: String,
        sql: Option<String>,
    }

    let items = conn.new_query(|rows| {
        let item = rows.join(SchemaList);
        crate::private::show_sql(|| {
            rows.into_vec(AdHoc::new(|mut cacher| {
                let typ = cacher.cache(item.typ());
                let tbl_name = cacher.cache(item.tbl_name());
                let sql = cacher.cache(item.sql());
                move |row| ItemDef {
                    typ: match row.get(typ).as_str() {
                        "table" => ItemTyp::Table,
                        "index" => ItemTyp::Index,
                        t => panic!("did not expect {t}"),
                    },
                    tbl_name: row.get(tbl_name),
                    sql: row.get(sql),
                }
            }))
        })
    });

    let mut output = hash::Schema::default();

    for item in items.iter().filter(|x| x.typ == ItemTyp::Table) {
        let mut table_def = HashMap::<&str, hash::Column>::new();

        let prefix = format!("CREATE TABLE \"{}\" ( ", item.tbl_name);
        let suffix = format!(" ) STRICT");
        let sql = item
            .sql
            .as_ref()
            .expect("tables should have sql")
            .strip_prefix(&prefix)
            .expect(&format!("expected sql to start with {prefix}"))
            .strip_suffix(&suffix)
            .expect(&format!("expected sql to end with {prefix}"));

        let mut uniques = Vec::new();

        eprintln!("{sql}");
        for part in sql.split(", ") {
            if part == "\"id\" integer PRIMARY KEY" {
            } else if let Some(part) = part.strip_prefix("UNIQUE (") {
                let part = part.strip_suffix(")").expect(part);
                let columns = part.split(", ").map(strip_str).collect();
                uniques.push(hash::Unique { columns });
            } else if let Some(part) = part.strip_prefix("FOREIGN KEY (") {
                let part = part.strip_suffix(" (\"id\")").unwrap();
                let (this_col, that) = part.split_once(") REFERENCES ").unwrap();
                let col_name = strip_str(this_col);
                assert!(table_def[col_name].fk.is_none());
                table_def.get_mut(col_name).unwrap().fk =
                    Some(("id".into(), strip_str(that).into()))
            } else {
                let (col, typ) = part.split_once(' ').unwrap();
                let (typ, nullable) = typ.split_once(' ').unwrap();
                let nullable = match nullable {
                    "NOT NULL" => false,
                    "NULL" => true,
                    x => panic!("{x}"),
                };
                let typ = match typ {
                    "integer" => hash::ColumnType::Integer { is_bool: false },
                    "real" | "REAL" => hash::ColumnType::Float,
                    "text" => hash::ColumnType::String,
                    _ => panic!(),
                };
                let col_name = strip_str(col);
                let res = table_def.insert(
                    col_name,
                    hash::Column {
                        name: col_name.into(),
                        typ,
                        nullable,
                        fk: None,
                    },
                );
                assert!(res.is_none());
            }
        }

        output.tables.insert((
            item.tbl_name.clone(),
            hash::Table {
                columns: table_def.into_values().collect(),
                uniques: uniques.into_iter().collect(),
            },
        ));
    }

    for item in items.iter().filter(|x: &&ItemDef| x.typ == ItemTyp::Index) {
        assert!(item.sql.is_none());
    }

    output
}
