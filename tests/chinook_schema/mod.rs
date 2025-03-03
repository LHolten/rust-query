use std::{collections::HashMap, fs};

use rust_query::{
    Database, IntoDummy, IntoDummyExt, LocalClient, Table, TableRow,
    migration::{Alter, Config, schema},
};

pub use v2::*;

#[schema]
#[version(0..=2)]
enum Schema {
    Album {
        title: String,
        artist: Artist,
    },
    Artist {
        #[unique]
        name: String,
    },
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
        #[unique_by_email]
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
        email: String,
    },
    Genre {
        name: String,
    },
    #[version(1..)]
    GenreNew {
        name: String,
        #[version(2..)]
        extra: i64,
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
        invoice: Invoice,
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
    let m = m.migrate(|txn| {
        for name in txn.query(|rows| {
            let genre = v0::Genre::join(rows);
            rows.into_vec(genre.name())
        }) {
            txn.try_insert_migrated(v1::update::GenreNewMigration { name })
                .unwrap();
        }

        v1::update::Schema {}
    });

    let m = m.migrate(|_| v2::update::Schema {
        customer: Box::new(|customer| {
            Alter::new(v2::update::CustomerMigration {
                // lets do some cursed phone number parsing :D
                phone: customer
                    .phone()
                    .map_dummy(|x| x.and_then(|x| x.parse().ok())),
            })
        }),
        track: Box::new(|track| {
            Alter::new(v2::update::TrackMigration {
                media_type: track.media_type().name().into_dummy(),
                composer_table: None::<TableRow<'_, v2::Composer>>.into_dummy(),
                byte_price: (track.unit_price(), track.bytes())
                    .map_dummy(|(price, bytes)| price as f64 / bytes as f64),
            })
        }),
        genre_new: Box::new(|genre| {
            Alter::new(v2::update::GenreNewMigration {
                extra: genre
                    .name()
                    .map_dummy(|name| genre_extra.get(&*name).copied().unwrap_or(0)),
            })
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
        expect!["15e9ff46816e4b45"].assert_eq(&hash_schema::<v1::Schema>());
    }
}
