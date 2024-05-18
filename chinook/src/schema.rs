use std::collections::HashMap;

use rust_query::client::Client;
use rust_query_macros::schema;

#[schema]
#[version(0..3)]
enum Schema {
    Album {
        #[version(1..)]
        something: String,
        #[version(1..)]
        new_title: String,
        #[version(..1)]
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
        address: Option<String>,
        city: Option<String>,
        state: Option<String>,
        country: String,
        postal_code: Option<String>,
        fax: Option<String>,
        email: String,
        support_rep: Employee,
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
    #[version(..2)]
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
    MediaType {
        name: String,
    },
    Playlist {
        name: String,
    },
    PlaylistTrack {
        playlist: Playlist,
        track: Track,
    },
    Track {
        name: String,
        album: Album,
        media_type: MediaType,
        #[version(..2)]
        genre: Genre,
        composer: Option<String>,
        milliseconds: i64,
        bytes: i64,
        unit_price: f64,
    },
}

pub fn migrate(client: &Client) -> v2::Schema {
    let artist_title = HashMap::from([("a", "b")]);
    client
        .migrator()
        .migrate(|_schema| v1::M {
            album: |row, album| {
                let artist = row.get(album.artist.name);
                Box::new(v1::MAlbum {
                    something: artist_title.get(&*artist).copied().unwrap_or("unknown"),
                    new_title: album.title,
                })
            },
        })
        .migrate(|_schema| v2::M {
            customer: |row, customer| {
                Box::new(v2::MCustomer {
                    phone: row.get(customer.phone).and_then(|x| x.parse::<i64>().ok()),
                })
            },
        })
        .check()
}
