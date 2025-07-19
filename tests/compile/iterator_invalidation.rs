//! The combination of edits not being allowed inside of [Transaction::query]
//! and iteration not allowed outside of [Transaction::query], makes it impossible
//! to invalidate the iterator.
use rust_query::{Database, migration::schema};

#[schema(Schema)]
pub mod vN {
    pub struct MyTable {
        pub name: String,
    }
}
use v0::*;

fn test1(db: Database<Schema>) {
    db.transaction_mut(|mut txn| {
        txn.query(|_rows| {
            // can not insert inside of `query`
            txn.insert_ok(MyTable { name: "test" });
        });
    })
}

fn test2(db: Database<Schema>) {
    db.transaction(|txn| {
        let names = txn.query(|rows| {
            let item = rows.join(MyTable);
            rows.into_iter(&item.name)
        });

        // can not take iterator outside of `query`
        for name in names {
            println!("{name}")
        }
    })
}

fn main() {}
