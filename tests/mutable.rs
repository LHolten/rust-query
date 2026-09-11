use std::panic::AssertUnwindSafe;

use rust_query::{Database, migration::Config};

#[test]
fn mutable_shenanigans() {
    #[rust_query::migration::schema(Test)]
    pub mod vN {
        pub struct Foo {
            pub alpha: i64,
            #[unique]
            pub bravo: i64,
        }
    }
    use v0::*;

    let db = Database::new(Config::open_in_memory());
    db.transaction_mut_ok(|txn| {
        txn.scoped(|txn| {
            txn.insert(Foo { alpha: 1, bravo: 1 }).unwrap();
            let row = txn.insert(Foo { alpha: 1, bravo: 2 }).unwrap();
            let mut mutable = txn.mutable(row);
            mutable.alpha = 100;
            mutable
                .unique(|x| {
                    x.bravo = 1;
                })
                .unwrap_err();
            assert_eq!(mutable.alpha, 100);
            assert_eq!(mutable.bravo, 2);

            let row = mutable.table_row();
            let view = txn.lazy(row);
            assert_eq!(view.alpha, 100);
            assert_eq!(view.bravo, 2);

            let mut mutable = txn.mutable(row);
            mutable.alpha = 200;

            // User applies AssertUnwindSafe to full closure. Should still be fine.
            let err = std::panic::catch_unwind(AssertUnwindSafe(|| {
                let _ = mutable.unique(|x| {
                    x.bravo = 1;
                    panic!("error in unique")
                });
            }))
            .unwrap_err();
            assert_eq!(*err.downcast_ref::<&str>().unwrap(), "error in unique");

            assert_eq!(mutable.alpha, 200); // mutation outside of `.unique` should still be applied
            assert_eq!(mutable.bravo, 2); // mutation inside unique should be reverted
        })
    })
}

#[test]
fn conflict() {
    #[rust_query::migration::schema(Test)]
    pub mod vN {
        #[no_reference]
        pub struct Artist {
            #[unique]
            pub name: String,
        }
    }
    use v0::*;

    let db = Database::new(Config::open_in_memory());
    db.transaction_mut_ok(|txn| {
        let first_id = txn
            .insert(Artist {
                name: "first".to_owned(),
            })
            .unwrap();
        let id = txn
            .insert(Artist {
                name: "second".to_owned(),
            })
            .unwrap();

        txn.scoped(|txn| {
            let conflict_id = txn
                .mutable(id)
                .unique(|artist| artist.name = "first".to_owned())
                .unwrap_err();
            assert_eq!(conflict_id, first_id);

            txn.mutable(id)
                .unique(|artist| artist.name = "other".to_owned())
                .unwrap();
            assert_eq!(txn.lazy(id).name, "other");
        });

        let db = txn.downgrade();
        assert!(db.delete_ok(id));
    })
}
