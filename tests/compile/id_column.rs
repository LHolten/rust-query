use rust_query::migration::schema;

#[schema(Schema)]
pub mod vN {
    pub struct MyTable {
        pub id: i64,
    }
}

fn main() {}
