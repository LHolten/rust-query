use rust_query::{Mutable, migration::schema};

#[schema(MySchema)]
pub mod vN {
    pub struct User {
        #[unique]
        pub name: String,
    }
}
use v0::*;

fn test(mut foo: Mutable<User>) {
    foo.name = "test".to_owned();
}

fn main() {}
