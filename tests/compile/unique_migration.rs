use rust_query::migration::schema;

#[schema]
#[version(0..=1)]
enum Schema {
    #[unique(col)]
    MyTable {
        #[version(..1)]
        col: i64,
        #[version(1..)]
        col: String,
    },
}

fn main() {}
