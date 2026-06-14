use std::panic::catch_unwind;

use rust_query::{
    Database,
    migration::{Config, schema},
    private::with_test_renderer,
};

#[schema(Schema)]
pub mod vN {
    pub struct Foo;
}

#[test]
fn schema_change() {
    let db = Database::<v0::Schema>::new(Config::open_in_memory());
    db.rusqlite_connection()
        .execute("ALTER TABLE foo ADD COLUMN name TEXT", [])
        .unwrap();
    let err = catch_unwind(move || with_test_renderer(|| db.transaction(|_txn| ()))).unwrap_err();
    expect_test::expect![[r#"
        error: Column mismatch for `#[version(0)]`
           ╭▸ tests/bad-use.rs:11:16
           │
        LL │     pub struct Foo;
           ╰╴               ━━━ database has column `name: Option<String>`"#]]
    .assert_eq(&err.downcast_ref::<String>().unwrap());
}
