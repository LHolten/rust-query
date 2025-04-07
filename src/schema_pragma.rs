use std::{collections::HashMap, convert::Infallible};

use ref_cast::RefCast;

use crate::{Expr, FromExpr, Select, Table, Transaction, db::Col, hash, private::Reader};

macro_rules! field {
    ($name:ident: $typ:ty) => {
        pub fn $name<'x>(&self) -> Expr<'x, Pragma, $typ> {
            Expr::new(Col::new(stringify!($name), self.0.inner.clone()))
        }
    };
    ($name:ident($name_str:literal): $typ:ty) => {
        pub fn $name<'x>(&self) -> Expr<'x, Pragma, $typ> {
            Expr::new(Col::new($name_str, self.0.inner.clone()))
        }
    };
}

macro_rules! table {
    ($typ:ident, $dummy:ident, $var:pat => $name:expr) => {
        impl Table for $typ {
            type MigrateFrom = Self;
            type Ext<T> = $dummy<T>;
            type Schema = Pragma;
            type Referer = ();
            fn get_referer_unchecked() -> Self::Referer {}

            fn name(&self) -> String {
                let $var = self;
                $name
            }

            fn typs(_f: &mut hash::TypBuilder<Self::Schema>) {}

            type Conflict<'t> = Infallible;
            type Update<'t> = ();
            type TryUpdate<'t> = ();
            type Insert<'t> = ();

            fn read<'t>(_val: &Self::Insert<'t>, _f: &Reader<'t, Self::Schema>) {
                unreachable!()
            }

            fn get_conflict_unchecked<'t>(
                _val: &Self::Insert<'t>,
            ) -> Select<'t, 't, Self::Schema, Option<Self::Conflict<'t>>> {
                unreachable!()
            }

            fn update_into_try_update<'t>(_val: Self::Update<'t>) -> Self::TryUpdate<'t> {
                unreachable!()
            }

            fn apply_try_update<'t>(
                _val: Self::TryUpdate<'t>,
                _old: Expr<'t, Self::Schema, Self>,
            ) -> Self::Insert<'t> {
                unreachable!()
            }

            const ID: &'static str = "";
            const NAME: &'static str = "";
        }
    };
}

pub struct Pragma;

struct TableList;

#[repr(transparent)]
#[derive(RefCast)]
struct TableListSelect<T>(T);

#[allow(unused)]
impl TableListSelect<Expr<'_, Pragma, TableList>> {
    field! {schema: String}
    field! {name: String}
    field! {r#type("type"): String}
    field! {ncol: i64}
    field! {wr: i64}
    field! {strict: i64}
}

table! {TableList, TableListSelect, _ => "pragma_table_list".to_owned()}

struct TableInfo(pub String);

#[repr(transparent)]
#[derive(RefCast)]
struct TableInfoSelect<T>(T);

impl TableInfoSelect<Expr<'_, Pragma, TableInfo>> {
    field! {name: String}
    field! {r#type("type"): String}
    field! {notnull: i64}
    field! {pk: i64}
}

table! {TableInfo, TableInfoSelect, val => format!("pragma_table_info('{}', 'main')", val.0)}

struct ForeignKeyList(pub String);

#[repr(transparent)]
#[derive(RefCast)]
struct ForeignKeyListSelect<T>(T);

#[allow(unused)]
impl ForeignKeyListSelect<Expr<'_, Pragma, ForeignKeyList>> {
    field! {table: String}
    field! {from: String}
    field! {to: String}
}

table! {ForeignKeyList, ForeignKeyListSelect, val => format!("pragma_foreign_key_list('{}', 'main')", val.0)}

struct IndexList(String);

#[repr(transparent)]
#[derive(RefCast)]
struct IndexListSelect<T>(T);

impl IndexListSelect<Expr<'_, Pragma, IndexList>> {
    field! {name: String}
    field! {unique: bool}
    field! {origin: String}
    field! {partial: bool}
}

table! {IndexList, IndexListSelect, val => format!("pragma_index_list('{}', 'main')", val.0)}

struct IndexInfo(String);

#[repr(transparent)]
#[derive(RefCast)]
struct IndexInfoSelect<T>(T);

impl IndexInfoSelect<Expr<'_, Pragma, IndexInfo>> {
    field! {name: Option<String>}
}

table! {IndexInfo, IndexInfoSelect, val => format!("pragma_index_info('{}', 'main')", val.0)}

pub fn read_schema(conn: &Transaction<Pragma>) -> hash::Schema {
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
        q.filter(table.schema().eq("main"));
        q.filter(table.r#type().eq("table"));
        q.filter(table.name().eq("sqlite_schema").not());
        q.into_vec(table.name())
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
                q.into_vec((fk.from(), fk.table()))
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
                name: col.name,
                nullable: col.notnull == 0,
            };
            table_def.columns.insert(def)
        }

        let uniques = conn.query(|q| {
            let index = q.join_custom(IndexList(table_name.clone()));
            q.filter(index.unique());
            q.filter(index.origin().eq("u"));
            q.filter(index.partial().not());
            q.into_vec(index.name())
        });

        for unique_name in uniques {
            let columns = conn.query(|q| {
                let col = q.join_custom(IndexInfo(unique_name));
                let name = q.filter_some(col.name());
                q.into_vec(name)
            });

            let mut unique_def = hash::Unique::default();
            for column in columns {
                unique_def.columns.insert(column);
            }
            table_def.uniques.insert(unique_def);
        }

        output.tables.insert((table_name, table_def))
    }
    output
}
