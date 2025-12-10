use rust_query::{
    Database, aggregate,
    migration::{Config, schema},
};
#[schema(Schema)]
pub mod vN {
    #[index(typ)]
    pub struct Queue {
        pub seq: i64,
        pub typ: i64,
    }
}
use v0::*;

fn main() {
    let database = Database::new(Config::open_in_memory());

    database.transaction(|txn| {
        txn.query_one(aggregate(|rows| {
            let queue = rows.join(Queue.typ(0));
            let min_seq = rows.min(&queue.seq);
            let min_seq = rows.filter_some(min_seq);
            rows.filter(min_seq.eq(&queue.seq));
            rows.min(queue)
        }));
    });
}
