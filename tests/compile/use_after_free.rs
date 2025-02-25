use rust_query::{Database, IntoColumn, LocalClient, Table};
use rust_query_macros::schema;

#[schema]
enum Schema {
    MyTable { name: String },
}
use v0::*;

fn test(db: Database<Schema>) {
    let mut client = LocalClient::try_new().unwrap();

    let txn = client.transaction(&db);
    let items = txn.query(|rows| {
        let item = MyTable::join(rows);
        rows.into_vec(item)
    });
    let items: Vec<_> = items.into_iter().map(|x| x.into_column()).collect();
    drop(txn);

    let txn = client.transaction(&db);
    for item in items {
        let name = txn.query_one(item.name());
        println!("{name}")
    }
}

fn main() {}
