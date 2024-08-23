use rust_query::{Client, Value};

fn main() {}

fn test<'a>(db: &Client, val: impl Value<'a, Typ = i64>) {
    db.get(val);
}
