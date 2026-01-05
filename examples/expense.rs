use std::fs;

use rust_query::{Database, Lazy, Transaction, migration::Config};

#[rust_query::migration::schema(Mifg)]
#[version(0..=1)]
pub mod vN {
    pub struct User {
        #[unique]
        pub user_id: String,
    }

    pub struct Expense {
        #[unique]
        pub expense_id: String,
        pub paid_by: User,
        #[version(0..=0)]
        pub split_with: User,
    }

    #[version(1..)]
    #[from(Expense)]
    #[unique(expense, user)]
    pub struct ExpensedUser {
        pub expense: Expense,
        pub user: User,
    }
}

const FILE_NAME: &str = "expense.sqlite";

fn migrate() -> Database<v1::Mifg> {
    Database::migrator(Config::open(FILE_NAME))
        .unwrap()
        .fixup(|txn| {
            // we insert some test data when the database is created
            let user1 = txn.insert(v0::User { user_id: "user1" }).unwrap();
            let user2 = txn.insert(v0::User { user_id: "user2" }).unwrap();
            txn.insert(v0::Expense {
                expense_id: "test expense",
                paid_by: user1,
                split_with: user2,
            })
            .unwrap();
        })
        .migrate(|txn| v0::migrate::Mifg {
            expense: txn.migrate_ok(|_| v0::migrate::Expense {}),
            expensed_user: txn
                .migrate(|expense: Lazy<v0::Expense>| v0::migrate::ExpensedUser {
                    expense: expense.table_row(),
                    user: expense.split_with.table_row(),
                })
                .unwrap(),
        })
        .fixup(|txn: &mut Transaction<v1::Mifg>| {
            for (expense, paid_by) in txn.query(|rows| {
                let expense = rows.join(v1::Expense);
                rows.into_vec((&expense, &expense.paid_by))
            }) {
                txn.insert(v1::ExpensedUser {
                    expense,
                    user: paid_by,
                })
                .unwrap();
            }
        })
        .finish()
        .unwrap()
}

fn main() {
    use v1::*;
    let _ = fs::remove_file(FILE_NAME);
    let db = migrate();
    db.transaction_mut_ok(|txn| {
        let user1 = txn.query_one(User.user_id("user1")).unwrap();
        txn.insert(Expense {
            expense_id: "expense without split",
            paid_by: user1,
        })
        .unwrap();
    });
    let db = migrate();
    db.transaction(|txn| {
        let mut split_with: Vec<_> = txn
            .lazy_iter(ExpensedUser)
            .map(|eu| (eu.user.user_id.clone(), eu.expense.expense_id.clone()))
            .collect();
        split_with.sort();
        let [(user1, exp1), (user2, exp2)] = split_with.try_into().unwrap();
        assert_eq!(user1, "user1");
        assert_eq!(user2, "user2");
        assert_eq!(exp1, "test expense");
        assert_eq!(exp2, "test expense");
    })
}

#[test]
fn run() {
    main();
}
