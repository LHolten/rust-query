use std::fs;

use crate::{
    Database, Lazy,
    migration::{Config, Migrated},
};

#[test]
fn unique_constraint_violation() {
    mod schema {
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
    }
    use schema::*;

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

#[test]
fn migrations_preserve_index() {
    mod schema {
        #[crate::migration::schema(Test)]
        #[version(0..=1)]
        pub mod vN {
            pub struct Foo {
                #[version(..1)]
                pub name: String,
                #[version(1..)]
                pub name: String,
            }
            pub struct Ref {
                pub foo: Foo,
            }
        }
    }
    mod schema_with_index {
        #[crate::migration::schema(Test)]
        #[version(1..=1)]
        pub mod vN {
            pub struct Foo {
                #[index]
                pub name: String,
            }
            pub struct Ref {
                pub foo: Foo,
            }
        }
    }
    use schema::*;

    const FILE: &str = "migrations_preserve_index.sqlite";
    let _ = fs::remove_file(FILE);

    let db: Database<v0::Test> = Database::new(Config::open(FILE));
    db.transaction_mut_ok(|txn| {
        let alpha = txn.insert_ok(v0::Foo { name: "alpha" });
        txn.insert_ok(v0::Foo { name: "brave" });
        let charlie = txn.insert_ok(v0::Foo { name: "charlie" });
        txn.insert_ok(v0::Ref { foo: charlie });
        let txn = txn.downgrade();
        // delete the first item so that migrations that do not preserve index
        // will renumber the items.
        assert!(txn.delete(alpha).unwrap());
    });

    Database::migrator(Config::open(FILE))
        .unwrap()
        .migrate(|txn| v0::migrate::Test {
            foo: txn
                .migrate(|prev: Lazy<v0::Foo>| v0::migrate::Foo {
                    name: prev.name.clone(),
                })
                .unwrap(),
        })
        .finish()
        .unwrap();

    let db: Database<schema_with_index::v1::Test> = Database::new(Config::open(FILE));
    db.transaction(|txn| {
        let [item] = txn
            .lazy_iter(schema_with_index::v1::Ref)
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        assert_eq!(item.foo.name, "charlie");
    });
}
