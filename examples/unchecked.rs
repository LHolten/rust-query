use rust_query::{
    migration::{schema, Config},
    Database, LocalClient,
};

#[schema]
enum Schema {
    Name { name: String },
}
use v0::*;

#[cfg(not(feature = "unchecked_transaction"))]
fn main() {
    println!("please run this example with `--features unchecked_transaction`")
}

#[cfg(feature = "unchecked_transaction")]
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

    let raw_txn = txn.unchecked_transaction();
    for id in ids {
        let name: String = raw_txn
            .query_row("select name from Name where id = $1", &[&id], |row| {
                row.get(0)
            })
            .unwrap();
        println!("{name}")
    }

    txn.commit();
}
