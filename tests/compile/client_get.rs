use rust_query::{Client, Db};

fn main() {}

fn test<'a>(db: &'a Client, val: Db<'a, i64>) {
    db.get(val);
}
