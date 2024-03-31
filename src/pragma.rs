use sea_query::Expr;

use crate::{value::Db, Builder, Table};

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
    fn name(&self) -> String {
        "pragma_table_list".to_owned()
    }

    type Dummy<'names> = TableListDummy<'names>;

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
}

pub struct TableInfo(pub String);

impl Table for TableInfo {
    type Dummy<'t> = TableInfoDummy<'t>;

    fn name(&self) -> String {
        format!(r#"pragma_table_info("{}", "main")"#, self.0)
    }

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        TableInfoDummy {
            name: f.col("name"),
            r#type: f.col("type"),
            notnull: f.col("notnull"),
            pk: f.col("pk"),
        }
    }
}
pub struct TableInfoDummy<'a> {
    pub name: Db<'a, String>,
    pub r#type: Db<'a, String>,
    pub notnull: Db<'a, i64>,
    pub pk: Db<'a, i64>,
}
