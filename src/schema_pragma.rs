use std::{collections::HashMap, convert::Infallible};

use sea_query::Func;

use crate::{
    Expr, FromExpr, Table, Transaction,
    alias::JoinableTable,
    hash,
    private::{Reader, new_column},
};

pub fn strip_raw(inp: &'static str) -> &'static str {
    inp.strip_prefix("r#").unwrap_or(inp)
}

macro_rules! table {
    {$typ:ident, $var:pat => $name:expr, $c:expr,
    $dummy:ident {
        $($field_name:ident: $field_typ:ty,)*
    }} => {
        #[allow(dead_code)]
        pub struct $dummy<'t> {
            $($field_name: Expr<'t, Pragma, $field_typ>,)*
        }
        impl Table for $typ {
            type MigrateFrom = Self;
            type Ext2<'t> = $dummy<'t>;

            fn covariant_ext<'x, 't>(val: &'x Self::Ext2<'static>) -> &'x Self::Ext2<'t> {
                val
            }

            fn build_ext2<'t>(val: &Expr<'t, Self::Schema, Self>) -> Self::Ext2<'t> {
                Self::Ext2 {
                    $($field_name: new_column(val, strip_raw(stringify!($field_name))),)*
                }
            }

            type Schema = Pragma;
            type Referer = ();
            fn get_referer_unchecked() -> Self::Referer {}

            fn name(&self) -> JoinableTable {
                let $var = self;
                $name
            }

            fn typs(_f: &mut hash::TypBuilder<Self::Schema>) {}

            type Conflict = Infallible;
            type UpdateOk = ();
            type Update = ();
            type Insert = ();
            type Lazy<'t> = ();

            fn read(_val: &Self::Insert, _f: &mut Reader<Self::Schema>) {
                unreachable!()
            }

            fn get_conflict_unchecked(
                _txn: &crate::Transaction< Self::Schema>,
                _val: &Self::Insert,
            ) -> Self::Conflict {
                unreachable!()
            }

            fn update_into_try_update(_val: Self::UpdateOk) -> Self::Update {
                unreachable!()
            }

            fn apply_try_update(
                _val: Self::Update,
                _old: Expr<'static, Self::Schema, Self>,
            ) -> Self::Insert {
                unreachable!()
            }

            fn get_lazy<'t>(_txn: &'t Transaction<Self::Schema>, _row: crate::TableRow<Self>) -> Self::Lazy<'t> {
                ()
            }

            const ID: &'static str = "";
            const NAME: &'static str = "";
        }
    };
}

pub struct Pragma;

struct TableList;

table! {
    TableList, _ => JoinableTable::Normal("pragma_table_list".into()), TableList,
    TableListSelect {
        schema: String,
        name: String,
        r#type: String,
        ncol: i64,
        wr: i64,
        strict: i64,
    }
}

struct TableInfo(pub String);

table! {
    TableInfo, val => JoinableTable::Pragma(Func::cust("pragma_table_info").arg(&val.0).arg("main")),
    TableInfo(String::new()),
    TableInfoSelect {
        name: String,
        r#type: String,
        notnull: i64,
        pk: i64,
    }
}

struct ForeignKeyList(pub String);

table! {
    ForeignKeyList, val => JoinableTable::Pragma(Func::cust("pragma_foreign_key_list").arg(&val.0).arg("main")),
    ForeignKeyList(String::new()),
    ForeignKeyListSelect {
        table: String,
        from: String,
        to: String,
    }
}

struct IndexList(String);

table! {
    IndexList, val => JoinableTable::Pragma(Func::cust("pragma_index_list").arg(&val.0).arg("main")),
    IndexList(String::new()),
    IndexListSelect {
        name: String,
        unique: bool,
        partial: bool,
    }
}

struct IndexInfo(String);

table! {IndexInfo, val => JoinableTable::Pragma(Func::cust("pragma_index_info").arg(&val.0).arg("main")),
    IndexInfo(String::new()),
    IndexInfoSelect {
        seqno: i64,
        name: Option<String>,
    }
}

pub fn read_schema<S>(_conn: &Transaction<S>) -> hash::Schema {
    let conn = Transaction::new();

    #[derive(Clone, FromExpr)]
    #[rust_query(From = TableInfo)]
    struct Column {
        name: String,
        r#type: String,
        pk: i64,
        notnull: i64,
    }

    let tables = conn.query(|q| {
        let table = q.join_custom(TableList);
        q.filter(table.schema.eq("main"));
        q.filter(table.r#type.eq("table"));
        q.filter(table.name.eq("sqlite_schema").not());
        // filter out tables such as `sqlite_stat1` and `sqlite_stat4`
        q.filter(table.name.starts_with("sqlite_stat").not());
        q.into_vec(&table.name)
    });

    let mut output = hash::Schema::default();

    for table_name in tables {
        let mut columns: Vec<Column> = conn.query(|q| {
            let table = q.join_custom(TableInfo(table_name.clone()));
            q.into_vec(Column::from_expr(table))
        });

        let fks: HashMap<_, _> = conn
            .query(|q| {
                let fk = q.join_custom(ForeignKeyList(table_name.to_owned()));
                q.into_vec((&fk.from, &fk.table))
            })
            .into_iter()
            .collect();

        let make_type = |col: &Column| match col.r#type.as_str() {
            "INTEGER" => hash::ColumnType::Integer,
            "TEXT" => hash::ColumnType::String,
            "REAL" => hash::ColumnType::Float,
            t => panic!("unknown type {t}"),
        };

        // we only care about columns that are not a unique id and for which we know the type
        columns.retain(|col| {
            if col.pk != 0 {
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
                nullable: col.notnull == 0,
            };
            let old = table_def.columns.insert(col.name, def);
            debug_assert!(old.is_none());
        }

        #[derive(Clone, FromExpr)]
        #[rust_query(From = IndexList)]
        struct Index {
            name: String,
            unique: bool,
            partial: bool,
        }

        let indices = conn.query(|q| {
            let index = q.join_custom(IndexList(table_name.clone()));
            q.into_vec(Index::from_expr(index))
        });

        #[derive(Clone, FromExpr)]
        #[rust_query(From = IndexInfo)]
        struct IndexColumn {
            seqno: i64,
            name: Option<String>,
        }

        for index in indices {
            let false = index.partial else {
                if index.unique {
                    panic!("unique partial index is not supported")
                }
                continue;
            };

            let mut columns = conn.query(|q| {
                let col = q.join_custom(IndexInfo(index.name));
                q.into_vec(IndexColumn::from_expr(col))
            });
            columns.sort_by_key(|x| x.seqno);

            let columns = columns.into_iter().map(|x| x.name).collect();

            let Some(columns) = columns else {
                if index.unique {
                    panic!("unique constraint on row_id or expression is not supported");
                }
                continue;
            };

            table_def.indices.push(hash::Index {
                columns,
                unique: index.unique,
            });
        }

        let old = output.tables.insert(table_name, table_def);
        debug_assert!(old.is_none());
    }

    output
}

pub fn read_index_names_for_table(conn: &Transaction<Pragma>, table_name: &str) -> Vec<String> {
    conn.query(|q| {
        let index = q.join_custom(IndexList(table_name.to_owned()));
        q.into_vec(&index.name)
    })
}
