use rust_query::{
    Database, IntoExpr,
    migration::{Config, schema},
};

#[schema(Schema)]
pub mod vN {
    pub struct Empty;
    #[no_reference]
    pub struct Ref {
        pub empty: Empty,
    }
}

pub fn main() {
    let db = Database::new(Config::open_in_memory());

    db.transaction_mut_ok(|txn| {
        let id = txn.insert_ok(v0::Empty {});
        let id2 = txn.insert_ok(v0::Empty {});
        let r = txn.insert_ok(v0::Ref { empty: id2 });
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
