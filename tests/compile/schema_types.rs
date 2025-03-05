use rust_query::migration::schema;

#[schema]
enum Schema {
    #[unique(optional)]
    Table {
        my_bool: bool,
        nested: Option<Option<i64>>,
        optional: Option<i64>,
    },
}

fn main() {}
