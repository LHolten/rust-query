use rust_query::{Database, migration::schema, optional};

#[schema(Schema)]
pub mod vN {
    pub struct MyTable {
        pub score: i64,
    }
}
use v0::*;

fn test(db: Database<Schema>) {
    db.transaction(|txn| {
        let score = txn.query(|rows| {
            let item = rows.join(MyTable);

            txn.query_one(optional(|row| row.then(&item.score)))
        });

        println!("{score:?}");
    })
}

fn main() {}
