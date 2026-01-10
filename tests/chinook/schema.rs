use std::{collections::HashMap, fs};

use rust_query::{
    Database, Lazy,
    migration::{Config, Migrated, schema},
};

pub use v2::*;

#[schema(Schema)]
#[version(0..=2)]
pub mod vN {
    pub struct Album {
        pub title: String,
        #[index]
        pub artist: Artist,
    }
    pub struct Artist {
        #[unique]
        pub name: String,
    }
    pub struct Customer {
        #[version(..2)]
        pub phone: Option<String>,
        #[version(2..)]
        pub phone: Option<i64>,
        pub first_name: String,
        pub last_name: String,
        pub company: Option<String>,
        pub address: String,
        pub city: String,
        pub state: Option<String>,
        pub country: String,
        pub postal_code: Option<String>,
        pub fax: Option<String>,
        #[unique]
        pub email: String,
        pub support_rep: Employee,
    }
    #[version(1..)]
    #[unique(employee, artist)]
    pub struct ListensTo {
        pub employee: Employee,
        pub artist: Artist,
    }
    pub struct Employee {
        pub last_name: String,
        pub first_name: String,
        pub title: Option<String>,
        pub reports_to: Option<Employee>,
        pub birth_date: Option<String>,
        pub hire_date: Option<String>,
        pub address: Option<String>,
        pub city: Option<String>,
        pub state: Option<String>,
        pub country: Option<String>,
        pub postal_code: Option<String>,
        pub phone: Option<String>,
        pub fax: Option<String>,
        #[version(..2)]
        pub email: String,
    }
    pub struct Genre {
        #[unique]
        pub name: String,
    }
    #[version(1..)]
    #[from(Genre)]
    pub struct GenreNew {
        pub name: String,
        #[version(2..)]
        pub extra: i64,
    }
    #[version(1..)]
    #[from(Genre)]
    pub struct ShortGenre {
        pub name: String,
    }
    pub struct Invoice {
        #[index]
        pub customer: Customer,
        pub invoice_date: String,
        pub billing_address: Option<String>,
        pub billing_city: Option<String>,
        pub billing_state: Option<String>,
        pub billing_country: Option<String>,
        pub billing_postal_code: Option<String>,
        pub total: f64,
    }
    pub struct InvoiceLine {
        #[version(..2)]
        pub invoice: Invoice,
        #[version(2..)]
        pub invoice_new: Invoice,
        pub track: Track,
        pub unit_price: f64,
        pub quantity: i64,
    }
    #[version(..2)]
    pub struct MediaType {
        pub name: String,
    }
    pub struct Playlist {
        pub name: String,
    }
    #[unique(playlist, track)]
    pub struct PlaylistTrack {
        pub playlist: Playlist,
        #[index]
        pub track: Track,
    }
    pub struct Track {
        pub name: String,
        #[index]
        pub album: Album,
        #[version(..2)]
        pub media_type: MediaType,
        #[version(2..)]
        pub media_type: String,
        #[index]
        pub genre: Genre,
        pub composer: Option<String>,
        #[version(2..)]
        pub composer_table: Option<Composer>,
        pub milliseconds: i64,
        pub bytes: i64,
        pub unit_price: f64,
        #[version(2..)]
        pub byte_price: f64,
        #[version(1..)]
        pub favorite: bool,
    }
    #[version(2..)]
    pub struct Composer {
        pub name: String,
    }
}

pub fn migrate() -> Database<v2::Schema> {
    if !fs::exists("Chinook_Sqlite.sqlite").unwrap() {
        panic!(
            "test data file 'Chinook_Sqlite.sqlite' does not exist.
            Please download it from https://github.com/lerocha/chinook-database/releases/tag/v1.4.5"
        );
    }
    let config = Config::open_in_memory();

    let genre_extra = HashMap::from([("rock", 10)]);
    let m = Database::migrator(config).unwrap();
    let m = m
        .fixup(|txn| {
            txn.downgrade().rusqlite_transaction(|txn| {
                txn.execute_batch("ATTACH 'Chinook_Sqlite.sqlite' AS old;")
                    .unwrap();
                txn.execute_batch(include_str!("migrate.sql")).unwrap();
            })
        })
        .migrate(|txn| v0::migrate::Schema {
            genre_new: txn.migrate_ok(|old: Lazy<v0::Genre>| v0::migrate::GenreNew {
                name: old.name.clone(),
            }),
            short_genre: {
                let Ok(()) = txn.migrate_optional(|old: Lazy<v0::Genre>| {
                    (old.name.len() <= 10).then_some(v0::migrate::GenreNew {
                        name: old.name.clone(),
                    })
                });
                Migrated::map_fk_err(|| panic!())
            },
            track: txn.migrate_ok(|_old| v0::migrate::Track { favorite: false }),
        });

    let m = m.migrate(|txn| v1::migrate::Schema {
        customer: txn.migrate_ok(|old: Lazy<v1::Customer>| {
            v1::migrate::Customer {
                // lets do some cursed phone number parsing :D
                phone: old.phone.as_ref().and_then(|x| x.parse().ok()),
            }
        }),
        track: txn.migrate_ok(|old: Lazy<v1::Track>| v1::migrate::Track {
            media_type: old.media_type.name.clone(),
            composer_table: None,
            byte_price: old.unit_price / old.bytes as f64,
        }),
        genre_new: txn.migrate_ok(|old: Lazy<v1::GenreNew>| v1::migrate::GenreNew {
            extra: genre_extra.get(&*old.name).copied().unwrap_or(0),
        }),
        employee: txn.migrate_ok(|_| v1::migrate::Employee {}),
        invoice_line: txn.migrate_ok(|old: Lazy<v1::InvoiceLine>| v1::migrate::InvoiceLine {
            invoice_new: old.invoice.table_row(),
        }),
    });

    m.finish().unwrap()
}

#[cfg(test)]
mod tests {
    use expect_test::expect;

    use super::*;

    #[test]
    #[cfg(feature = "dev")]
    fn backwards_compat() {
        use rust_query::migration::hash_schema;

        expect!["3944d3e8bd2922be"].assert_eq(&hash_schema::<v0::Schema>());
        expect!["a93ff07cbd480913"].assert_eq(&hash_schema::<v1::Schema>());
    }
}
