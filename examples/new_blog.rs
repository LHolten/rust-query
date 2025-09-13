use rust_query::{Database, Select, TableRow, Transaction, aggregate, migration::schema, optional};

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

    #[expect(unused)]
    #[derive(FromExpr, Select)]
    #[rust_query(From = Measurement)]
    struct Score {
        score: i64,
        timestamp: i64,
    }

    #[expect(unused)]
    fn read_scores(txn: &Transaction<Schema>) -> Vec<Score> {
        txn.query(|rows| {
            let m = rows.join(Measurement);
            rows.into_vec(ScoreSelect {
                score: &m.score,
                timestamp: &m.timestamp,
            })
        })
    }

    #[expect(unused)]
    fn read_scores2(txn: &Transaction<Schema>) -> Vec<Score> {
        txn.query(|rows| {
            let m = rows.join(Measurement);
            rows.into_vec(FromExpr::from_expr(m))
        })
    }

    #[expect(unused)]
    fn read_scores3(txn: &Transaction<Schema>) -> Vec<Measurement!(score, timestamp)> {
        txn.query(|rows| {
            let m = rows.join(Measurement);
            rows.into_vec(FromExpr::from_expr(m))
        })
    }
}

fn main() {
    let db = using_v1::migrate();
    db.transaction_mut_ok(using_v1::do_stuff)
}

#[test]
fn run() {
    main();
}

mod using_v1 {
    use super::*;
    use rust_query::{Transaction, migration::Config};
    use v1::*;

    pub fn migrate() -> Database<Schema> {
        let m = Database::migrator(Config::open("db.sqlite"))
            .expect("database should not be older than supported versions");
        let m = m.migrate(|txn| v0::migrate::Schema {
            measurement: txn.migrate_ok(|old: v0::Measurement!(score)| v0::migrate::Measurement {
                value: old.score as f64,
            }),
        });
        m.finish()
            .expect("database should not be newer than supported versions")
    }

    pub fn do_stuff(txn: &'static mut Transaction<Schema>) {
        let loc: TableRow<Location> = txn.insert_ok(Location { name: "Amsterdam" });
        let _ = location_info(txn, loc);

        let txn = txn.downgrade();

        let is_deleted = txn
            .delete(loc)
            .expect("there should be no fk references to this row");
        assert!(is_deleted);

        let is_not_deleted_twice = !txn
            .delete(loc)
            .expect("there should be no fk references to this row");
        assert!(is_not_deleted_twice);
    }

    #[expect(unused)]
    #[derive(Select)]
    struct Info {
        average_value: f64,
        total_duration: i64,
    }

    fn location_info(txn: &Transaction<Schema>, loc: TableRow<Location>) -> Option<Info> {
        txn.query_one(aggregate(|rows| {
            let m = rows.join(Measurement);
            rows.filter(m.location.eq(loc));

            optional(|row| {
                let average_value = row.and(rows.avg(&m.value));
                row.then(InfoSelect {
                    average_value,
                    total_duration: rows.sum(&m.duration),
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
