use rust_query::{
    Database, IntoExpr,
    migration::{Config, schema},
};

#[schema(Schema)]
#[version(0..=1)]
pub mod vN {
    #[version(..1)]
    pub struct EmptyOld;
    #[version(1..)]
    #[from(EmptyOld)]
    pub struct Empty;

    #[no_reference]
    pub struct Ref {
        pub empty: rust_query::TableRow<Empty>,
    }
}
use v1::*;

pub fn main() {
    let db = Database::migrator(Config::open_in_memory())
        .unwrap()
        .migrate(|txn| v0::migrate::Schema {
            empty: txn.migrate_ok(|_v| v0::migrate::Empty {}),
        })
        .finish()
        .unwrap();

    db.transaction_mut_ok(|txn| {
        let id = txn.insert_ok(Empty {});
        let id2 = txn.insert_ok(Empty {});
        let r = txn.insert_ok(Ref { empty: id2 });
        let id = txn.query_one(id.into_expr());
        let txn = txn.downgrade();
        assert!(txn.delete(id).unwrap());
        txn.delete(id2).unwrap_err();
        txn.delete_ok(r);
        assert!(txn.delete(id2).unwrap());
    })
}

#[test]
fn run() {
    main();
}
