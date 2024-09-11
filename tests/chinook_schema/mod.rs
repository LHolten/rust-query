use std::{collections::HashMap, fs};

use rust_query::{
    migration::{schema, NoTable, Prepare},
    Database, ThreadToken,
};

pub use v2::*;

#[schema]
#[version(0..=2)]
enum Schema {
    Album {
        #[version(1..)]
        something: String,
        title: String,
        artist: Artist,
    },
    Artist {
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
    // #[version(..2)]
    Genre {
        name: String,
    },
    #[version(1..)]
    #[create_from(Genre)]
    GenreNew {
        #[version(..2)]
        #[unique_original]
        original: Genre,
        #[unique]
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
        #[version(..1)]
        playlist: Playlist,
        #[version(1..)]
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
        #[version(..2)]
        genre: Genre,
        #[version(2..)]
        genre: GenreNew,
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

pub fn migrate(t: &mut ThreadToken) -> Database<v2::Schema> {
    let m = Prepare::open_in_memory();

    if !fs::exists("Chinook_Sqlite.sqlite").unwrap() {
        panic!("test data file 'Chinook_Sqlite.sqlite' does not exist");
    }
    let m = m
        .create_db_sql::<v0::Schema>(&[
            "ATTACH 'Chinook_Sqlite.sqlite' AS old;",
            include_str!("migrate.sql"),
        ])
        .unwrap();

    let artist_title = HashMap::from([("a", "b")]);
    let m = m.migrate(t, |db| v1::update::Schema {
        album: Box::new(|album| v1::update::AlbumMigration {
            something: {
                let artist = db.query_one(album.artist().name());
                artist_title.get(&*artist).copied().unwrap_or("unknown")
            },
        }),
        playlist_track: Box::new(|pt| v1::update::PlaylistTrackMigration {
            playlist: pt.playlist(),
        }),
        genre_new: Box::new(|genre| {
            Some(v1::update::GenreNewMigration {
                name: genre.name(),
                original: genre,
            })
        }),
    });

    let m = m.migrate(t, |db| v2::update::Schema {
        customer: Box::new(|customer| v2::update::CustomerMigration {
            phone: db
                .query_one(customer.phone())
                .and_then(|x| x.parse::<i64>().ok()),
        }),
        track: Box::new(|track| v2::update::TrackMigration {
            media_type: track.media_type().name(),
            composer_table: None::<NoTable>,
            byte_price: db.query_one(track.unit_price()) / db.query_one(track.bytes()) as f64,
            genre: db
                .query_one(v1::GenreNew::unique_original(track.genre()))
                .unwrap(),
        }),
        genre_new: Box::new(|_genre_new| v2::update::GenreNewMigration {}),
    });

    m.finish(t).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_query::migration::expect;

    #[test]
    fn backwards_compat() {
        v0::assert_hash(expect!["f62a50a3ac341a65"]);
        v1::assert_hash(expect!["63dcef403a40bc8a"]);
    }
}
