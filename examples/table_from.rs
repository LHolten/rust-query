use rust_query::migration::schema;

#[schema(Schema)]
#[version(0..=1)]
pub mod vN {
    pub struct Foo;
    #[from(Foo)]
    #[version(1..)]
    pub struct FooNext;
    pub struct Bar {
        // this will be `Foo` for v0 and `FooNext` for v1
        #[unique]
        pub evolving: rust_query::TableRow<FooNext>,
        // this will be `Foo` in both v0 and v1
        pub foo: rust_query::TableRow<Foo>,
    }
}

pub fn main() {}
