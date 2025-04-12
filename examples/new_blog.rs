use rust_query::{
    Database, LocalClient, Select, Table, TableRow, Transaction, aggregate, migration::schema,
    optional,
};

#[schema(Schema)]
#[version(0..=1)]
pub mod vN {

    pub struct Measurement {
        #[version(..1)]
        pub score: i64,
        #[version(1..)]
        pub value: f64,
        pub duration: i64,
        pub confidence: f64,
        pub timestamp: i64,
        pub location: Location,
    }
    pub struct Location {
        pub name: String,
    }
}

mod using_v0 {
    use super::*;
    use rust_query::FromExpr;
    use v0::*;

    #[derive(FromExpr, Select)]
    #[rust_query(From = Measurement)]
    struct Score {
        score: i64,
        timestamp: i64,
    }

    fn read_scores(txn: &Transaction<Schema>) -> Vec<Score> {
        txn.query(|rows| {
            let m = Measurement::join(rows);
            rows.into_vec(ScoreSelect {
                score: m.score(),
                timestamp: m.timestamp(),
            })
        })
    }

    fn read_scores2(txn: &Transaction<Schema>) -> Vec<Score> {
        txn.query(|rows| {
            let m = Measurement::join(rows);
            rows.into_vec(FromExpr::from_expr(m))
        })
    }

    fn read_scores3(txn: &Transaction<Schema>) -> Vec<Measurement!(score, timestamp)> {
        txn.query(|rows| {
            let m = Measurement::join(rows);
            rows.into_vec(FromExpr::from_expr(m))
        })
    }
}

mod using_v1 {
    use super::*;
    use rust_query::{TransactionMut, TransactionWeak, migration::Config};
    use v1::*;

    fn migrate(client: &mut LocalClient) -> Database<Schema> {
        let m = client
            .migrator(Config::open("db.sqlite"))
            .expect("database should not be older than supported versions");
        let m = m.migrate(|txn| v0::migrate::Schema {
            measurement: txn.migrate_ok(|old: v0::Measurement!(score)| v0::migrate::Measurement {
                value: old.score as f64,
            }),
        });
        m.finish()
            .expect("database should not be newer than supported versions")
    }

    fn do_stuff(mut txn: TransactionMut<Schema>) {
        let loc: TableRow<Location> = txn.insert_ok(Location { name: "Amsterdam" });

        let mut txn: TransactionWeak<Schema> = txn.downgrade();

        let is_deleted = txn
            .delete(loc)
            .expect("there should be no fk references to this row");
        assert!(is_deleted);

        let is_not_deleted_twice = !txn
            .delete(loc)
            .expect("there should be no fk references to this row");
        assert!(is_not_deleted_twice);
    }

    #[derive(Select)]
    struct Info {
        average_value: f64,
        total_duration: i64,
    }

    fn location_info<'t>(
        txn: &Transaction<'t, Schema>,
        loc: TableRow<'t, Location>,
    ) -> Option<Info> {
        txn.query_one(aggregate(|rows| {
            let m = Measurement::join(rows);
            rows.filter_on(m.location(), loc);

            optional(|row| {
                let average_value = row.and(rows.avg(m.value()));
                row.then(InfoSelect {
                    average_value,
                    total_duration: rows.sum(m.duration()),
                })
            })
        }))
    }
}

mod delete_example {
    use super::*;
    #[schema(Schema)]
    #[version(0..=1)]
    pub mod vN {
        #[version(..1)]
        pub struct User {
            pub name: String,
        }
        #[version(1..)]
        #[from(User)]
        pub struct Author {
            pub name: String,
        }
        pub struct Book {
            pub author: Author,
        }
    }
}

fn main() {}
