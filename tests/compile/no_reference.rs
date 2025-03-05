use rust_query::migration::schema;

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
