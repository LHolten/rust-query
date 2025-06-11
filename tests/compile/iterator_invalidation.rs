use rust_query::{Database, LocalClient, migration::schema};

#[schema(Schema)]
pub mod vN {
    pub struct MyTable {
        pub name: String,
    }
}
use v0::*;

fn test(db: Database<Schema>) {
    let mut client = LocalClient::try_new().unwrap();

    let mut txn = client.transaction_mut(&db);
    txn.query(|rows| {
        let item = rows.join(MyTable);
        let names = rows.into_iter(item.name());

        txn.insert_ok(MyTable { name: "test" });

        for name in names {
            println!("{name}")
        }
    });
}

fn main() {}
