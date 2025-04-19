use rust_query::migration::schema;

#[schema(Schema)]
pub mod vN {
    #[unique(bar)]
    pub struct Foo;
    pub struct Bar {
        pub foo: Foo,
    }
}

pub fn main() {}
