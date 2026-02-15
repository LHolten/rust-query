use rust_query::migration::schema;

#[schema(Schema)]
#[version(0..=1)]
pub mod vN {
    #[version(1..)]
    pub struct FooNext;
    pub struct Bar {
        pub evolving: rust_query::TableRow<FooNext>,
    }
}

pub fn main() {}
