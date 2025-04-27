use rust_query::{Database, LocalClient, migration::schema, optional};

#[schema(Schema)]
pub mod vN {
    pub struct MyTable {
        pub score: i64,
    }
}
use v0::*;

fn test(db: Database<Schema>) {
    let mut client = LocalClient::try_new().unwrap();

    let txn = client.transaction(&db);
    let score = txn.query(|rows| {
        let item = rows.join(MyTable);

        txn.query_one(optional(|row| row.then(item.score())))
    });

    println!("{score:?}");
}

fn main() {}
