use rust_query::{ReadTransaction, Value};

fn main() {}

fn test<'a, S>(db: &ReadTransaction<'a, S>, val: impl Value<'a, S, Typ = i64>) {
    db.get(val);
}
