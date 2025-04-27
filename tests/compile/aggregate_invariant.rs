use rust_query::{Database, LocalClient, aggregate, migration::schema};

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
    let total = txn.query(|rows| {
        let item = rows.join(MyTable);

        txn.query_one(aggregate(|rows| rows.sum(item.score())))
    });

    println!("{total}");
}

fn main() {}
