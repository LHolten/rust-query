use rust_query::{Transaction, Value};

fn main() {}

fn test<'a, S>(db: &Transaction<'a, S>, val: impl Value<'a, S, Typ = i64>) {
    db.query_one(val);
}
