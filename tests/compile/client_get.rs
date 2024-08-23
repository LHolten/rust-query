use rust_query::{Client, Db};

fn main() {}

fn test(db: &Client, val: Db<i64>) {
    db.get(val);
}
