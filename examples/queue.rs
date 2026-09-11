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

    let items = vec![
        Queue { seq: 10, typ: 1 },
        Queue { seq: 11, typ: 0 },
        Queue { seq: 10, typ: 0 },
    ];

    database.transaction_mut_ok(|txn| {
        for item in items {
            txn.insert_ok(item);
        }

        let res = txn
            .lazy(aggregate(|rows| {
                let queue = rows.join(Queue.typ(0));
                let min_seq = rows.min(&queue.seq);
                let min_seq = rows.filter_some(min_seq);
                rows.filter(min_seq.eq(&queue.seq));
                rows.min(queue)
            }))
            .unwrap();

        assert_eq!(res.seq, 10);
        assert_eq!(res.typ, 0);
    });
}

#[test]
fn run() {
    main();
}
