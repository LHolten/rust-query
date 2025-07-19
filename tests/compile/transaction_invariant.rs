use rust_query::{Database, migration::schema};

#[schema(Schema)]
pub mod vN {
    pub struct MyTable {
        pub name: String,
    }
}
use v0::*;

fn test(db: Database<Schema>) {
    db.transaction(|txn| {
        let name = txn.query(|rows| {
            let item = rows.join(MyTable);

            txn.query_one(&item.name)
        });

        println!("{name}");
    })
}

fn main() {}
