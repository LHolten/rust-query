use rust_query::migration::schema;

#[schema(Schema)]
pub mod vN {
    pub struct Table {
        #[index]
        pub optional: Option<i64>,
    }
}

fn main() {}
