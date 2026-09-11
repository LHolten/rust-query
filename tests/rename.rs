use rust_query::{
    Database, aggregate,
    migration::{Config, schema},
};

use crate::v0::{Foo, Schema};

#[schema(Schema)]
pub mod vN {
    use rust_query::TableRow;

    #[rename("foo_bar")]
    #[primary_key("fooo_id")]
    pub struct Foo {
        #[rename("bax_9000")]
        pub bax: i64,
    }
    pub struct Bar {
        pub foo: TableRow<Foo>,
    }
}

#[test]
fn test() {
    let db = Database::<Schema>::new(Config::open_in_memory());
    let conn = db.rusqlite_connection();
    let sql: String = conn
        .query_one(
            "SELECT sql FROM sqlite_schema WHERE name = 'foo_bar'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    expect_test::expect![[r#"CREATE TABLE "foo_bar" ("fooo_id" INTEGER PRIMARY KEY, "bax_9000" INTEGER NOT NULL) STRICT"#]].assert_eq(&sql);
}

#[test]
fn test_aggregate() {
    let db = Database::<Schema>::new(Config::open_in_memory());
    db.transaction(|txn| {
        txn.query(|rows| {
            let foo = rows.join(Foo);
            let sum = aggregate(|rows| rows.sum(&foo.bax));
            rows.into_vec(sum)
        })
    });
}
