use rust_query::{Database, IntoExpr, LocalClient, Table, migration::schema};

#[schema(Schema)]
pub mod vN {
    pub struct MyTable {
        pub name: String,
    }
}
use v0::*;

fn test(db: Database<Schema>) {
    let mut client = LocalClient::try_new().unwrap();

    let txn = client.transaction(&db);
    let items = txn.query(|rows| {
        let item = MyTable::join(rows);
        rows.into_vec(item)
    });
    let items: Vec<_> = items.into_iter().map(|x| x.into_expr()).collect();
    drop(txn);

    let txn = client.transaction(&db);
    for item in items {
        let name = txn.query_one(item.name());
        println!("{name}")
    }
}

fn main() {}
