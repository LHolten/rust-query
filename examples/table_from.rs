use rust_query::migration::schema;

#[schema(Schema)]
#[version(0..=1)]
pub mod vN {
    use rust_query::TableRow;

    pub struct Foo;
    #[from(Foo)]
    #[version(1..)]
    pub struct FooNext;
    pub struct Bar {
        // this will be `Foo` for v0 and `FooNext` for v1
        #[unique]
        pub evolving: TableRow<FooNext>,
        // this will be `Foo` in both v0 and v1
        pub foo: TableRow<Foo>,
    }
}

pub fn main() {}
