use std::collections::HashMap;

use rust_query::{schema, Client, Null, Prepare};

#[schema]
#[version(0..3)]
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
        birth_day: Option<String>,
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

pub fn migrate() -> (Client, v2::Schema) {
    let artist_title = HashMap::from([("a", "b")]);
    let m = Prepare::open_in_memory();
    m.execute_batch(include_str!("../Chinook_Sqlite.sql"));
    m.execute_batch(include_str!("../migrate.sql"));

    m.migrator::<v0::Schema>()
        .migrate(|_schema| v1::M {
            album: |row, album| {
                let artist = row.get(album.artist.name);
                Box::new(v1::MAlbum {
                    something: artist_title.get(&*artist).copied().unwrap_or("unknown"),
                    // new_title: album.title,
                })
            },
        })
        .migrate(|_schema| v2::M {
            customer: |row, customer| {
                Box::new(v2::MCustomer {
                    phone: row.get(customer.phone).and_then(|x| x.parse::<i64>().ok()),
                })
            },
            track: |row, track| {
                Box::new(v2::MTrack {
                    media_type: track.media_type.name,
                    composer_table: Null::<v2::Composer>::default(),
                    byte_price: row.get(track.unit_price) / row.get(track.bytes) as f64,
                })
            },
        })
        .finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_query::{expect, Schema};

    #[test]
    fn backwards_compat() {
        v0::Schema::assert_hash(expect!["38f654ce24217792"]);
        v1::Schema::assert_hash(expect!["d9962ef27f0ea2e8"]);
    }
}
