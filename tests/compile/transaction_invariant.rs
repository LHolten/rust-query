use rust_query::{Database, Table, ThreadToken};
use rust_query_macros::schema;

#[schema]
enum Schema {
    MyTable { name: String },
}
use v0::*;

fn test(db: Database<Schema>) {
    let mut token = ThreadToken::try_new().unwrap();

    let txn = db.read(&mut token);
    let name = txn.query(|rows| {
        let item = MyTable::join(rows);

        txn.query_one(item.name())
    });

    println!("{name}");
}

fn main() {}