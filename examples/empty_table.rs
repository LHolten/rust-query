use rust_query::{
    Database, IntoExpr,
    migration::{Config, schema},
};

#[schema(Schema)]
pub mod vN {
    pub struct Empty;
}

pub fn main() {
    let db: Database<v0::Schema> = Database::migrator(Config::open_in_memory())
        .expect("database is older than supported versions")
        .finish()
        .expect("database is newer than supported versions");

    db.transaction_mut_ok(|txn| {
        let id = txn.insert(v0::Empty).unwrap();
        let id = txn.query_one(id.into_expr());
        let txn = txn.downgrade();
        assert!(txn.delete(id).unwrap());
    })
}
