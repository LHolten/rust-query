use std::{collections::HashMap, fs, ops::Deref};

use rust_query::{
    migration::{schema, Alter, Config, Create, NoTable},
    Column, Database, Dummy, IntoColumn, LocalClient, Table,
};

pub use v2::*;

pub trait MyValue<'t, T>: IntoColumn<'t, Schema, Typ = T> {}
impl<'t, X> MyValue<'t, X::Typ> for X where X: IntoColumn<'t, Schema> {}

pub trait MyTable<'t, T: Table>: MyValue<'t, T> + Deref<Target = T::Ext<Self>> {}
impl<'t, T: Table, X> MyTable<'t, T> for X where X: MyValue<'t, T> + Deref<Target = T::Ext<Self>> {}

pub type MyDyn<'t, T> = Column<'t, Schema, T>;

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

pub fn migrate(client: &mut LocalClient) -> Database<v2::Schema> {
    if !fs::exists("Chinook_Sqlite.sqlite").unwrap() {
        panic!("test data file 'Chinook_Sqlite.sqlite' does not exist");
    }
    let config = Config::open_in_memory()
        .init_stmt("ATTACH 'Chinook_Sqlite.sqlite' AS old;")
        .init_stmt(include_str!("migrate.sql"));

    let genre_extra = HashMap::from([("rock", 10)]);
    let m = client.migrator(config).unwrap();
    let m = m.migrate(v1::update::Schema {
        playlist_track: Box::new(|pt| {
            Alter::new(v1::update::PlaylistTrackMigration {
                playlist: pt.playlist(),
            })
        }),
        genre_new: Box::new(|rows| {
            let genre = v0::Genre::join(rows);
            Create::new(v1::update::GenreNewMigration { name: genre.name() })
        }),
        listens_to: Box::new(|rows| Create::empty(rows)),
    });

    let m = m.migrate(v2::update::Schema {
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
                media_type: track.media_type().name(),
                composer_table: None::<NoTable>,
                byte_price: (track.unit_price(), track.bytes())
                    .map_dummy(|(price, bytes)| price as f64 / bytes as f64),
            })
        }),
        composer: Box::new(|rows| Create::empty(rows)),
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
    use super::*;
    use rust_query::migration::expect;

    #[test]
    fn backwards_compat() {
        v0::assert_hash(expect!["a57e97b8c243859a"]);
        v1::assert_hash(expect!["15e9ff46816e4b45"]);
    }
}
