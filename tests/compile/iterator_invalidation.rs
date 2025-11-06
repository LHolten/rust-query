use rust_query::{Database, migration::schema};

#[schema(Schema)]
pub mod vN {
    pub struct MyTable {
        pub name: String,
    }
}
use v0::*;

fn test(db: Database<Schema>) {
    db.transaction_mut_ok(|txn| {
        let names = txn.query(|rows| {
            let item = rows.join(MyTable);
            rows.into_iter(&item.name)
        });

        // mutating invalidates the iterator.
        txn.insert_ok(MyTable { name: "foo" });

        for name in names {
            println!("{name}")
        }
    })
}

fn main() {}
