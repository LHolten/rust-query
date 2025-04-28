use std::{collections::HashMap, convert::Infallible};

use ref_cast::RefCast;

use crate::{
    Expr, FromExpr, Table, Transaction, hash,
    private::{Reader, new_column},
};

macro_rules! field {
    ($name:ident: $typ:ty) => {
        pub fn $name(&self) -> Expr<'x, Pragma, $typ> {
            new_column(&self.0, stringify!($name))
        }
    };
    ($name:ident($name_str:literal): $typ:ty) => {
        pub fn $name(&self) -> Expr<'x, Pragma, $typ> {
            new_column(&self.0, $name_str)
        }
    };
}

macro_rules! table {
    ($typ:ident, $dummy:ident, $var:pat => $name:expr, $c:expr) => {
        impl Table for $typ {
            type MigrateFrom = Self;
            type Ext<T> = $dummy<T>;

            const TOKEN: Self = $c;

            type Schema = Pragma;
            type Referer = ();
            fn get_referer_unchecked() -> Self::Referer {}

            fn name(&self) -> String {
                let $var = self;
                $name
            }

            fn typs(_f: &mut hash::TypBuilder<Self::Schema>) {}

            type Conflict<'t> = Infallible;
            type UpdateOk<'t> = ();
            type Update<'t> = ();
            type Insert<'t> = ();

            fn read<'t>(_val: &Self::Insert<'t>, _f: &mut Reader<'t, Self::Schema>) {
                unreachable!()
            }

            fn get_conflict_unchecked<'t>(
                _txn: &crate::Transaction<'t, Self::Schema>,
                _val: &Self::Insert<'t>,
            ) -> Self::Conflict<'t> {
                unreachable!()
            }

            fn update_into_try_update(_val: Self::UpdateOk<'_>) -> Self::Update<'_> {
                unreachable!()
            }

            fn apply_try_update<'t>(
                _val: Self::Update<'t>,
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
impl<'x> TableListSelect<Expr<'x, Pragma, TableList>> {
    field! {schema: String}
    field! {name: String}
    field! {r#type("type"): String}
    field! {ncol: i64}
    field! {wr: i64}
    field! {strict: i64}
}

table! {TableList, TableListSelect, _ => "pragma_table_list".to_owned(), TableList}

struct TableInfo(pub String);

#[repr(transparent)]
#[derive(RefCast)]
struct TableInfoSelect<T>(T);

impl<'x> TableInfoSelect<Expr<'x, Pragma, TableInfo>> {
    field! {name: String}
    field! {r#type("type"): String}
    field! {notnull: i64}
    field! {pk: i64}
}

table! {TableInfo, TableInfoSelect, val => format!("pragma_table_info('{}', 'main')", val.0), TableInfo(String::new())}

struct ForeignKeyList(pub String);

#[repr(transparent)]
#[derive(RefCast)]
struct ForeignKeyListSelect<T>(T);

#[allow(unused)]
impl<'x> ForeignKeyListSelect<Expr<'x, Pragma, ForeignKeyList>> {
    field! {table: String}
    field! {from: String}
    field! {to: String}
}

table! {ForeignKeyList, ForeignKeyListSelect, val => format!("pragma_foreign_key_list('{}', 'main')", val.0), ForeignKeyList(String::new())}

struct IndexList(String);

#[repr(transparent)]
#[derive(RefCast)]
struct IndexListSelect<T>(T);

impl<'x> IndexListSelect<Expr<'x, Pragma, IndexList>> {
    field! {name: String}
    field! {unique: bool}
    field! {origin: String}
    field! {partial: bool}
}

table! {IndexList, IndexListSelect, val => format!("pragma_index_list('{}', 'main')", val.0), IndexList(String::new())}

struct IndexInfo(String);

#[repr(transparent)]
#[derive(RefCast)]
struct IndexInfoSelect<T>(T);

impl<'x> IndexInfoSelect<Expr<'x, Pragma, IndexInfo>> {
    field! {name: Option<String>}
}

table! {IndexInfo, IndexInfoSelect, val => format!("pragma_index_info('{}', 'main')", val.0), IndexInfo(String::new())}

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
        q.filter(table.name().eq("sqlite_stat1").not());
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
