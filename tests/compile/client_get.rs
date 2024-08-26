use rust_query::{Snapshot, Value};

fn main() {}

fn test<'a, S>(db: &Snapshot<'a, S>, val: impl Value<'a, S, Typ = i64>) {
    db.get(val);
}
