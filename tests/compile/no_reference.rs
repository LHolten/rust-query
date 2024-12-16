use rust_query_macros::schema;

#[schema]
enum Schema {
    #[no_reference]
    SomeTable {
        data: String,
    },
    NotAllowed {
        marker: SomeTable,
    },
}

fn main() {}
