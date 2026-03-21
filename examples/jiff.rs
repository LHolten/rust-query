use rust_query::{
    Database,
    migration::{Config, schema},
};

#[schema(Log)]
pub mod vN {
    use jiff::Timestamp;

    pub struct Entry {
        pub text: String,
        #[index]
        pub timestamp: Timestamp,
    }
}
use v0::*;

fn main() {
    let db = Database::new(Config::open_in_memory());

    db.transaction_mut_ok(|txn| {
        let a = txn.insert_ok(Entry {
            text: "hello world!".to_owned(),
            timestamp: jiff::Timestamp::now(),
        });

        let [b] = txn.query(|rows| {
            let entry = rows.join(Entry);
            rows.filter(entry.timestamp.lte(jiff::Timestamp::now()));
            rows.into_vec(entry).try_into().unwrap()
        });

        assert_eq!(a, b);
    });
}
