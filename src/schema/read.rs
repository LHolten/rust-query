use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet, HashMap},
};

use rust_query_macros::schema;

use crate::{
    FromExpr, Transaction,
    lower::{JoinableTable, ord_rc::OrdRc},
    private::{IntoJoinable, Joinable},
    schema::{canonical, check_constraint, from_db},
    value::DbTyp,
};

#[schema(Pragma)]
pub mod vN {
    pub struct SqliteSchema {
        pub r#type: String,
        pub name: String,
        pub tbl_name: String,
        pub sql: String,
    }

    #[rename("pragma_table_list")]
    pub struct TableList {
        pub schema: String,
        pub name: String,
        pub r#type: String,
        pub ncol: i64,
        pub wr: i64,
        pub strict: bool,
    }

    #[rename("pragma_table_info")]
    pub struct TableInfo {
        pub name: String,
        pub r#type: String,
        pub notnull: bool,
        pub pk: i64,
    }

    #[rename("pragma_foreign_key_list")]
    #[primary_key("pragma_has_no_pk")]
    pub struct ForeignKeyList {
        pub id: i64,
        pub seq: i64,
        pub table: String,
        pub from: String,
        pub to: Option<String>,
    }

    #[rename("pragma_index_list")]
    pub struct IndexList {
        pub name: String,
        pub unique: bool,
        pub partial: bool,
    }

    #[rename("pragma_index_info")]
    pub struct IndexInfo {
        pub seqno: i64,
        pub name: Option<String>,
    }
}
pub use v0::*;

pub fn read_schema(conn: &'static Transaction<Pragma>) -> from_db::Schema {
    #[derive(Clone, FromExpr)]
    #[rust_query(From = TableInfo)]
    struct Column {
        name: String,
        r#type: String,
        pk: i64,
        notnull: bool,
    }

    let tables = conn.query(|q| {
        let table = q.join(TableList);
        q.filter(table.schema.eq("main"));
        q.filter(table.r#type.eq("table"));
        q.filter(table.name.neq("sqlite_schema"));
        // filter out tables such as `sqlite_stat1` and `sqlite_stat4`
        q.filter(table.name.starts_with("sqlite_stat").not());
        q.into_vec((&table.name, &table.strict))
    });

    let table_sql: HashMap<_, _> = conn.query(|q| {
        let table = q.join(SqliteSchema);
        q.filter(table.r#type.eq("table"));
        q.into_iter((&table.name, &table.sql)).collect()
    });

    struct Basic {
        primary_key: String,
        columns: Vec<Column>,
        fks: BTreeMap<String, ForeignKey>,
    }

    let mut basic = BTreeMap::new();
    for (table_name, strict) in tables {
        assert!(strict, "all tables must be STRICT");

        let mut columns: Vec<Column> = conn.query(|q| {
            let table = q.join(with_arg(TableInfo, &table_name));
            q.into_vec(Column::from_expr(table))
        });

        let fks = read_fks(conn, &table_name);

        let mut primary_key = None;
        for col in columns.extract_if(.., |col| col.pk != 0) {
            if primary_key.is_some() {
                panic!("multi column primary key is not supported");
            }
            assert!(
                !fks.contains_key(&col.name),
                "primary key is not allowed to have a foreign key constraint"
            );
            assert_eq!(col.r#type, "INTEGER", "primary key must be `INTEGER` type");
            assert_eq!(
                check_constraint::get_check_constraint(&table_sql[&table_name], &col.name),
                None,
                "primary key can not have check constraint"
            );
            primary_key = Some(col.name);
        }
        let Some(primary_key) = primary_key else {
            panic!("table must have a primary key");
        };

        basic.insert(
            table_name,
            Basic {
                primary_key,
                columns,
                fks,
            },
        );
    }
    let pks: BTreeMap<_, _> = basic
        .iter()
        .map(|(name, basic)| (name.clone(), basic.primary_key.clone()))
        .collect();

    let mut output = from_db::Schema::default();

    for (
        table_name,
        Basic {
            primary_key,
            columns,
            mut fks,
        },
    ) in basic
    {
        let mut out_columns = BTreeMap::new();
        for col in columns {
            let def = canonical::Column {
                fk: fks.remove(&col.name).map(|x| {
                    let to = x.column.unwrap_or_else(|| pks[&x.table].clone());
                    (x.table, to)
                }),
                typ: col.r#type.parse().unwrap(),
                nullable: !col.notnull,
                check: check_constraint::get_check_constraint(&table_sql[&table_name], &col.name),
            };
            let old = out_columns.insert(col.name, def);
            debug_assert!(old.is_none());
        }
        debug_assert!(fks.is_empty());

        #[derive(Clone, FromExpr)]
        #[rust_query(From = IndexList)]
        struct Index {
            name: String,
            unique: bool,
            partial: bool,
        }

        let indices = conn.query(|q| {
            let index = q.join(with_arg(IndexList, &table_name));
            q.into_vec(Index::from_expr(index))
        });

        #[derive(Clone, FromExpr)]
        #[rust_query(From = IndexInfo)]
        struct IndexColumn {
            seqno: i64,
            name: Option<String>,
        }

        let mut out_indices = BTreeSet::new();
        for index in indices {
            let false = index.partial else {
                if index.unique {
                    panic!("unique partial index is not supported")
                }
                continue;
            };

            let mut columns = conn.query(|q| {
                let col = q.join(with_arg(IndexInfo, &index.name));
                q.into_vec(IndexColumn::from_expr(col))
            });
            columns.sort_by_key(|x| x.seqno);

            let columns = columns
                .into_iter()
                .map(|x| x.name.map(Cow::Owned))
                .collect();

            let Some(columns) = columns else {
                if index.unique {
                    panic!("unique constraint on rowid or expression is not supported");
                }
                continue;
            };

            out_indices.insert(from_db::Index {
                columns,
                unique: index.unique,
            });
        }

        let old = output.tables.insert(
            table_name,
            from_db::Table {
                primary_key,
                columns: out_columns,
                indices: out_indices,
            },
        );
        debug_assert!(old.is_none());
    }

    output
}

struct ForeignKey {
    table: String,
    column: Option<String>,
}

fn read_fks(conn: &'static Transaction<Pragma>, table_name: &str) -> BTreeMap<String, ForeignKey> {
    let fks: BTreeMap<_, _> = conn.query(|rows| {
        let fk = rows.join(with_arg(ForeignKeyList, table_name));
        rows.into_iter((&fk.id, &fk.table)).collect()
    });

    fks.into_iter()
        .map(|(id, table)| {
            let from_to: Vec<(String, Option<String>)> = conn.query(|rows| {
                let fk = rows.join(with_arg(ForeignKeyList, table_name));
                rows.filter(fk.id.eq(id));
                rows.order_by()
                    .asc(&fk.seq)
                    .into_iter((&fk.from, &fk.to))
                    .collect()
            });
            let [(from, to)] = from_to
                .try_into()
                .expect("foreign key must target one column");

            (from, ForeignKey { table, column: to })
        })
        .collect()
}

fn with_arg<T: DbTyp>(
    joinable: impl IntoJoinable<'static, Pragma, Typ = T>,
    arg: &str,
) -> Joinable<'static, Pragma, T> {
    let mut joinable = joinable.into_joinable();
    let JoinableTable::Table(_, pragma_arg) = &mut joinable.table.name else {
        panic!()
    };
    *pragma_arg = Some(OrdRc::new(arg.to_owned()));
    joinable
}
