use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet, HashMap},
    convert::Infallible,
    ops::Deref,
    rc::Rc,
};

use crate::{
    Expr, FromExpr, IntoSelect, Select, Table, TableRow, Transaction,
    lower::{self, JoinableTable, ord_rc::OrdRc},
    private::Reader,
    schema::{self, canonical, check_constraint, from_db},
};

pub fn strip_raw(inp: &'static str) -> &'static str {
    inp.strip_prefix("r#").unwrap_or(inp)
}

struct NoMut;
impl Deref for NoMut {
    type Target = ();

    #[cfg_attr(false, mutants::skip)]
    fn deref(&self) -> &Self::Target {
        &()
    }
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
        impl crate::CustomJoin for $typ {
            fn name(&self) -> JoinableTable {
                let $var = self;
                $name
            }
            fn main_column(&self) -> &'static str {
                "pragma_id" // should always be replaced
            }
        }
        impl Table for $typ {
            type MigrateFrom = Self;
            type Ext2<'t> = $dummy<'t>;

            fn covariant_ext<'x, 't>(val: &'x Self::Ext2<'static>) -> &'x Self::Ext2<'t> {
                val
            }

            fn build_ext2<'t>(val: &Expr<'t, Self::Schema, TableRow<Self>>) -> Self::Ext2<'t> {
                let lower::Expr::RowIndex(row_like, "pragma_id") = Rc::as_ref(&val.inner) else {
                    unreachable!()
                };
                Self::Ext2 {
                    $($field_name: Expr::adhoc(
                        lower::Expr::RowIndex(row_like.clone(), strip_raw(stringify!($field_name)))
                    ),)*
                }
            }

            type Schema = Pragma;
            type Referer = ();
            fn get_referer_unchecked() -> Self::Referer {}

            fn typs(_f: &mut schema::from_macro::TypBuilder<Self::Schema>) {}

            type Conflict = Infallible;
            type Lazy<'t> = ();
            type Mutable = NoMut;

            type Select = ();

            fn into_select(_val: Expr<'_, Self::Schema, TableRow<Self>>) -> Select<'_, Self::Schema, Self::Select> {
                ().into_select()
            }

            fn select_mutable(_select: Self::Select) -> Self::Mutable {
                unreachable!()
            }

            fn select_lazy<'t>(_select: Self::Select) -> Self::Lazy<'t> {
                unreachable!()
            }

            fn mutable_as_unique(_val: &mut Self::Mutable) -> &mut <Self::Mutable as Deref>::Target {
                unreachable!()
            }
            fn mutable_into_insert(_val: Self::Mutable) -> Self {
                unreachable!()
            }

            fn read(&self, _f: &mut Reader) {
                unreachable!()
            }

            const ID: &'static str = "pragma_id";
            const NAME: &'static str = "pragma_name";
            const SPAN: (usize, usize) = (0, 0);
        }
    };
}

pub struct Pragma;

struct TableList;

table! {
    TableList, _ => JoinableTable::Table("pragma_table_list"), TableList,
    TableListSelect {
        schema: String,
        name: String,
        r#type: String,
        ncol: i64,
        wr: i64,
        strict: bool,
    }
}

struct TableInfo(pub String);

table! {
    TableInfo, val => JoinableTable::Pragma("pragma_table_info", vec![OrdRc::new(val.0.to_owned()), OrdRc::new("main".to_owned())]),
    TableInfo(String::new()),
    TableInfoSelect {
        name: String,
        r#type: String,
        notnull: bool,
        pk: i64,
    }
}

struct ForeignKeyList(pub String);

table! {
    ForeignKeyList, val => JoinableTable::Pragma("pragma_foreign_key_list", vec![OrdRc::new(val.0.to_owned()), OrdRc::new("main".to_owned())]),
    ForeignKeyList(String::new()),
    ForeignKeyListSelect {
        table: String,
        from: String,
        to: Option<String>,
    }
}

struct IndexList(String);

table! {
    IndexList, val => JoinableTable::Pragma("pragma_index_list", vec![OrdRc::new(val.0.to_owned()), OrdRc::new("main".to_owned())]),
    IndexList(String::new()),
    IndexListSelect {
        name: String,
        unique: bool,
        partial: bool,
    }
}

struct IndexInfo(String);

table! {IndexInfo, val => JoinableTable::Pragma("pragma_index_info", vec![OrdRc::new(val.0.to_owned()), OrdRc::new("main".to_owned())]),
    IndexInfo(String::new()),
    IndexInfoSelect {
        seqno: i64,
        name: Option<String>,
    }
}

struct SqliteSchema;

table! {SqliteSchema, _ => JoinableTable::Table("sqlite_schema"),
    SqliteSchema,
    SqliteSchemaSelect {
        r#type: String,
        name: String,
        tbl_name: String,
        sql: String,
    }
}

pub fn read_schema<S>(_conn: &Transaction<S>) -> from_db::Schema {
    let conn = Transaction::new();

    #[derive(Clone, FromExpr)]
    #[rust_query(From = TableInfo)]
    struct Column {
        name: String,
        r#type: String,
        pk: i64,
        notnull: bool,
    }

    let tables = conn.query(|q| {
        let table = q.join_custom(TableList);
        q.filter(table.schema.eq("main"));
        q.filter(table.r#type.eq("table"));
        q.filter(table.name.neq("sqlite_schema"));
        // filter out tables such as `sqlite_stat1` and `sqlite_stat4`
        q.filter(table.name.starts_with("sqlite_stat").not());
        q.into_vec((&table.name, &table.strict))
    });

    let table_sql: HashMap<_, _> = conn.query(|q| {
        let table = q.join_custom(SqliteSchema);
        q.filter(table.r#type.eq("table"));
        q.into_iter((&table.name, &table.sql)).collect()
    });

    struct Basic {
        primary_key: String,
        columns: Vec<Column>,
        fks: BTreeMap<String, ForeignKey>,
    }

    #[derive(Clone, FromExpr)]
    #[rust_query(From = ForeignKeyList)]
    struct ForeignKey {
        table: String,
        to: Option<String>,
    }

    let mut basic = BTreeMap::new();
    for (table_name, strict) in tables {
        assert!(strict, "all tables must be STRICT");

        let mut columns: Vec<Column> = conn.query(|q| {
            let table = q.join_custom(TableInfo(table_name.clone()));
            q.into_vec(Column::from_expr(table))
        });

        let fks: BTreeMap<_, _> = conn
            .query(|q| {
                let fk = q.join_custom(ForeignKeyList(table_name.to_owned()));
                q.into_iter((&fk.from, ForeignKey::from_expr(&fk)))
            })
            .collect();

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
                    let to = x.to.unwrap_or_else(|| pks[&x.table].clone());
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
            let index = q.join_custom(IndexList(table_name.clone()));
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
                let col = q.join_custom(IndexInfo(index.name.clone()));
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
