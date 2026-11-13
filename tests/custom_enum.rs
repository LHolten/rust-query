use rust_query::{StorableTyp, migration::schema};

fn main() {}

#[schema(Schema)]
pub mod vN {
    use crate::Quality;

    pub struct Item {
        pub name: String,
        pub quality: Quality,
    }
}

#[derive(Clone, StorableTyp)]
pub enum Quality {
    Foo,
    Bar,
}
