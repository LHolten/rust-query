use rust_query::{Select, Table, TableRow, Transaction, aggregate, migration::schema, optional};

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
use v1::*;

#[derive(Select)]
struct Info {
    average_value: f64,
    total_duration: i64,
}

fn location_info<'t>(txn: &Transaction<'t, Schema>, loc: TableRow<'t, Location>) -> Option<Info> {
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

fn main() {}
