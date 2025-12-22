use std::fs;

use crate::{
    Database, Lazy,
    migration::{Config, Migrated},
};

#[crate::migration::schema(Test)]
#[version(0..=1)]
pub mod vN {
    pub struct Foo {
        #[version(..1)]
        pub name: String,
        #[version(1..)]
        #[unique]
        pub name: String,
    }
}

#[test]
fn unique_constraint_violation() {
    const FILE: &str = "unique_constraint_violation.sqlite";
    let _ = fs::remove_file(FILE);

    let db: Database<v0::Test> = Database::new(Config::open(FILE));
    db.transaction_mut_ok(|txn| {
        txn.insert_ok(v0::Foo { name: "alpha" });
        txn.insert_ok(v0::Foo { name: "alpha" });
    });

    Database::migrator(Config::open(FILE))
        .unwrap()
        .migrate(|txn| {
            let res = txn.migrate(|prev: Lazy<v0::Foo>| v0::migrate::Foo {
                name: prev.name.clone(),
            });
            assert!(res.is_err(), "the new unique constraint should be caught");
            v0::migrate::Test {
                foo: Migrated::map_fk_err(|| panic!()),
            }
        })
        .finish()
        .unwrap();
}
