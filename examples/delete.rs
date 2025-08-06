use rust_query::{
    Database,
    migration::{Config, schema},
};

#[schema(Schema)]
pub mod vN {
    #[no_reference]
    pub struct Name {
        pub name: String,
    }
}
use v0::*;

fn main() {
    // Get a LocalClient to prove that we have our own thread.
    // This is necessary to keep transactions separated.
    let database: Database<Schema> = Database::migrator(Config::open_in_memory())
        .expect("database version is before supported versions")
        // migrations go here
        .finish()
        .expect("database version is after supported versions");

    database.transaction_mut(|txn| {
        let ids: Vec<_> = vec!["alpha", "bravo", "charlie", "delta"]
            .into_iter()
            .map(|name| txn.insert_ok(Name { name }))
            .collect();

        let txn = txn.downgrade();
        for id in ids.clone() {
            assert!(txn.delete_ok(id));
        }
        for id in ids {
            assert!(!txn.delete_ok(id));
        }
    })
}

#[test]
fn run() {
    main();
}

#[test]
#[cfg(feature = "dev")]
fn schema_hash() {
    use expect_test::expect;
    use rust_query::migration::hash_schema;
    expect!["822e0ab9b42056f7"].assert_eq(&hash_schema::<v0::Schema>());
}
