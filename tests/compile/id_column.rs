use rust_query::migration::schema;

#[schema]
enum Schema {
    MyTable { id: i64 },
}

fn main() {}
