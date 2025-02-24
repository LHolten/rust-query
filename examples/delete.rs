use rust_query::{
    Database, LocalClient,
    migration::{Config, schema},
};

#[schema]
enum Schema {
    #[no_reference]
    Name { name: String },
}
use v0::*;

fn main() {
    // Get a LocalClient to prove that we have our own thread.
    // This is necessary to keep transactions separated.
    let mut client = LocalClient::try_new().unwrap();
    let database: Database<Schema> = client
        .migrator(Config::open_in_memory())
        .expect("database version is before supported versions")
        // migrations go here
        .finish()
        .expect("database version is after supported versions");

    let mut txn = client.transaction_mut(&database);

    let ids: Vec<_> = vec!["alpha", "bravo", "charlie", "delta"]
        .into_iter()
        .map(|name| txn.insert(Name { name }))
        .collect();

    let mut txn = txn.downgrade();
    for id in ids.clone() {
        assert!(txn.delete(id));
    }
    for id in ids {
        assert!(!txn.delete(id));
    }
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
