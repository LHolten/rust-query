use rust_query::migration::schema;

#[schema(Schema)]
#[version(0..=1)]
pub mod vN {
    pub struct Table {
        #[version(1..)]
        pub my_bool: bool,
        pub other: Other,
    }
    pub struct Other;
}

fn main() {}
