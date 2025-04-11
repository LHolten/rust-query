use rust_query::migration::schema;

#[schema(Schema)]
#[version(1..=1)]
pub mod vN {
    pub struct Foo;
    #[version(1..)]
    #[from(Foo)]
    pub struct FooNext;
}

pub fn main() {}
