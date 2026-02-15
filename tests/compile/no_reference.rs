use rust_query::migration::schema;

#[schema(Schema)]
pub mod vN {
    #[no_reference]
    pub struct SomeTable {
        pub data: String,
    }
    pub struct NotAllowed {
        pub marker: rust_query::TableRow<SomeTable>,
    }
}

fn main() {}
