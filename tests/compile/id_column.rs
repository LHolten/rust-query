use rust_query_macros::schema;

#[schema]
enum Schema {
    MyTable { id: i64 },
}

fn main() {}
