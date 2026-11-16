use expect_test::expect;
use rust_query::{
    Database, IntoExpr, StorableTyp,
    migration::{Config, schema},
};

use crate::v0::Item;

#[test]
fn test() {
    let db = Database::new(Config::open_in_memory());
    db.transaction_mut_ok(|txn| {
        txn.scope(|txn| {
            let id = txn.insert_ok(Item {
                name: "foo".to_owned(),
                quality: Quality::Foo,
            });

            assert_eq!(txn.query_one(&id.into_expr().quality), Quality::Foo);
            assert_eq!(txn.lazy(id).quality, Quality::Foo)
        });
    });

    let sql = "SELECT sql FROM sqlite_schema WHERE name = 'item'";
    let res: String = db
        .rusqlite_connection()
        .query_one(sql, (), |row| row.get(0))
        .unwrap();
    expect![[r#"CREATE TABLE "item" ("id" INTEGER PRIMARY KEY, "name" TEXT NOT NULL, "quality" TEXT NOT NULL CHECK ("quality" IN ('bar', 'foo'))) STRICT"#]].assert_eq(&res);
}

#[schema(Schema)]
pub mod vN {
    use crate::Quality;

    pub struct Item {
        pub name: String,
        pub quality: Quality,
    }
}

#[derive(Clone, Debug, PartialEq, StorableTyp)]
pub enum Quality {
    Foo,
    Bar,
}
