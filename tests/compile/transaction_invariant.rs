use rust_query::{Database, LocalClient, Table, migration::schema};

#[schema(Schema)]
pub mod vN {
    pub struct MyTable {
        pub name: String,
    }
}
use v0::*;

fn test(db: Database<Schema>) {
    let mut client = LocalClient::try_new().unwrap();

    let txn = client.transaction(&db);
    let name = txn.query(|rows| {
        let item = MyTable::join(rows);

        txn.query_one(item.name())
    });

    println!("{name}");
}

fn main() {}
