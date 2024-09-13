use rust_query::migration::schema;

#[schema]
enum Schema {
    Table {
        my_bool: bool,
        nested: Option<Option<i64>>,
    },
}

fn main() {}
