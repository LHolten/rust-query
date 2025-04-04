use std::{collections::HashMap, fs};

use rust_query::{
    Database, LocalClient,
    migration::{Config, Migrated, schema},
};

pub use v2::*;

#[schema]
#[version(0..=2)]
enum Schema {
    Album {
        title: String,
        artist: Artist,
    },
    #[unique(name)]
    Artist {
        name: String,
    },
    #[unique_by_email(email)]
    Customer {
        #[version(..2)]
        phone: Option<String>,
        #[version(2..)]
        phone: Option<i64>,
        first_name: String,
        last_name: String,
        company: Option<String>,
        address: String,
        city: String,
        state: Option<String>,
        country: String,
        postal_code: Option<String>,
        fax: Option<String>,
        email: String,
        support_rep: Employee,
    },
    #[version(1..)]
    #[unique(employee, artist)]
    ListensTo {
        employee: Employee,
        artist: Artist,
    },
    Employee {
        last_name: String,
        first_name: String,
        title: Option<String>,
        reports_to: Option<Employee>,
        birth_date: Option<String>,
        hire_date: Option<String>,
        address: Option<String>,
        city: Option<String>,
        state: Option<String>,
        country: Option<String>,
        postal_code: Option<String>,
        phone: Option<String>,
        fax: Option<String>,
        #[version(..2)]
        email: String,
    },
    Genre {
        name: String,
    },
    #[version(1..)]
    #[from(Genre)]
    GenreNew {
        name: String,
        #[version(2..)]
        extra: i64,
    },
    #[version(1..)]
    #[from(Genre)]
    ShortGenre {
        name: String,
    },
    Invoice {
        customer: Customer,
        invoice_date: String,
        billing_address: Option<String>,
        billing_city: Option<String>,
        billing_state: Option<String>,
        billing_country: Option<String>,
        billing_postal_code: Option<String>,
        total: f64,
    },
    InvoiceLine {
        #[version(..2)]
        invoice: Invoice,
        #[version(2..)]
        invoice_new: Invoice,
        track: Track,
        unit_price: f64,
        quantity: i64,
    },
    #[version(..2)]
    MediaType {
        name: String,
    },
    Playlist {
        name: String,
    },
    #[unique(playlist, track)]
    PlaylistTrack {
        playlist: Playlist,
        track: Track,
    },
    Track {
        name: String,
        album: Album,
        #[version(..2)]
        media_type: MediaType,
        #[version(2..)]
        media_type: String,
        genre: Genre,
        composer: Option<String>,
        #[version(2..)]
        composer_table: Option<Composer>,
        milliseconds: i64,
        bytes: i64,
        unit_price: f64,
        #[version(2..)]
        byte_price: f64,
    },
    #[version(2..)]
    Composer {
        name: String,
    },
}

pub fn migrate(client: &mut LocalClient) -> Database<v2::Schema> {
    if !fs::exists("Chinook_Sqlite.sqlite").unwrap() {
        panic!(
            "test data file 'Chinook_Sqlite.sqlite' does not exist. 
            Please download it from https://github.com/lerocha/chinook-database/releases/tag/v1.4.5"
        );
    }
    let config = Config::open_in_memory()
        .init_stmt("ATTACH 'Chinook_Sqlite.sqlite' AS old;")
        .init_stmt(include_str!("migrate.sql"));

    let genre_extra = HashMap::from([("rock", 10)]);
    let m = client.migrator(config).unwrap();
    let m = m.migrate(|txn| v1::update::Schema {
        genre_new: txn
            .migrate_ok(|old: v0::Genre!(name)| v1::update::GenreNewMigration { name: old.name }),
        short_genre: {
            let Ok(()) = txn.migrate_optional(|old: v0::Genre!(name)| {
                (old.name.len() <= 10).then_some(v1::update::GenreNewMigration { name: old.name })
            });
            Migrated::map_fk_err(|| panic!())
        },
    });

    let m = m.migrate(|txn| v2::update::Schema {
        customer: txn.migrate_ok(|old: v1::Customer!(phone)| {
            v2::update::CustomerMigration {
                // lets do some cursed phone number parsing :D
                phone: old.phone.and_then(|x| x.parse().ok()),
            }
        }),
        track: txn.migrate_ok(
            |old: v1::Track!(media_type as v1::MediaType!(name), unit_price, bytes)| {
                v2::update::TrackMigration {
                    media_type: old.media_type.name,
                    composer_table: None,
                    byte_price: old.unit_price as f64 / old.bytes as f64,
                }
            },
        ),
        genre_new: txn.migrate_ok(|old: v1::GenreNew!(name)| v2::update::GenreNewMigration {
            extra: genre_extra.get(&*old.name).copied().unwrap_or(0),
        }),
        employee: txn.migrate_ok(|()| v2::update::EmployeeMigration {}),
        invoice_line: txn.migrate_ok(|old: v1::InvoiceLine!(invoice<'_>)| {
            v2::update::InvoiceLineMigration {
                invoice_new: old.invoice,
            }
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

        expect!["a57e97b8c243859a"].assert_eq(&hash_schema::<v0::Schema>());
        expect!["9b14036757e3cc6b"].assert_eq(&hash_schema::<v1::Schema>());
    }
}
