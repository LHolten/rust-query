use rust_query::migration::schema;

#[schema(Schema)]
pub mod vN {
    #[unique(optional)]
    pub struct Table {
        pub my_bool: bool,
        pub nested: Option<Option<i64>>,
        pub optional: Option<i64>,
    }
}

fn main() {}
