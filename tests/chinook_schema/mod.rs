use std::{collections::HashMap, fs, ops::Deref};

use rust_query::{
    migration::{schema, NoTable, Prepare},
    Database, DynValue, Table, ThreadToken, Value,
};

pub use v2::*;

pub trait MyValue<'t, T>: Value<'t, Schema, Typ = T> {}
impl<'t, X> MyValue<'t, X::Typ> for X where X: Value<'t, Schema> {}

pub trait MyTable<'t, T: Table>: MyValue<'t, T> + Deref<Target = T::Ext<Self>> {}
impl<'t, T: Table, X> MyTable<'t, T> for X where X: MyValue<'t, T> + Deref<Target = T::Ext<Self>> {}

pub type MyDyn<'t, T> = DynValue<'t, Schema, T>;

#[schema]
#[version(0..=2)]
enum Schema {
    Album {
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
        playlist_track: Box::new(|pt| v1::update::PlaylistTrackMigration {
            playlist: pt.playlist(),
        }),
        listens_to: Box::new(v1::update::ListensToMigration::empty),
        genre_new: Box::new(|rows| {
            let genre = v0::Genre::join(rows);
            v1::update::GenreNewMigration { name: genre.name() }
        }),
    });

    let m = m.migrate(t, |db| v2::update::Schema {
        customer: Box::new(|customer| v2::update::CustomerMigration {
            phone: Some(42i64).into_dyn(),
        }),
        track: Box::new(|track| v2::update::TrackMigration {
            media_type: track.media_type().name(),
            composer_table: None::<NoTable>.into_dyn(),
            byte_price: 0.5f64.into_dyn(),
        }),
        composer: Box::new(v2::update::ComposerMigration::empty),
    });

    m.finish(t).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_query::migration::expect;

    #[test]
    fn backwards_compat() {
        v0::assert_hash(expect!["a5821c5b83d30d7c"]);
        v1::assert_hash(expect!["6dec069b8b65eeeb"]);
    }
}
