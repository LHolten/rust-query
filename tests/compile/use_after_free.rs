use rust_query::{Database, IntoExpr, migration::schema};

#[schema(Schema)]
pub mod vN {
    pub struct MyTable {
        pub name: String,
    }
}
use v0::*;

fn test(db: Database<Schema>) {
    db.transaction(|txn| {
        let items = txn.query(|rows| {
            let item = rows.join(MyTable);
            rows.into_vec(item)
        });

        db.transaction(|txn| {
            for item in items {
                let name = txn.query_one(&item.into_expr().name);
                println!("{name}")
            }
        })
    })
}

fn main() {}
