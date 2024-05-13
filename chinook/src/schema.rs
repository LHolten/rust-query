use std::collections::HashMap;

use rust_query::migrate::Schema as _;
use rust_query_macros::schema;

#[schema]
#[version(0..4)]
enum Schema {
    Album {
        #[version(1..)]
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
    Employee {},
    Genre {
        name: String,
    },
}

pub fn migrate() -> v3::Schema {
    let artist_title = HashMap::from([("a", "b")]);
    ().migrate(|()| v0::M {})
        .migrate(|_schema| v1::M {
            album: |row, album| {
                let artist = row.get(album.artist.name);
                Box::new(v1::MAlbum {
                    title: artist_title.get(&*artist).copied().unwrap_or("unknown"),
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
        .migrate(|_| v3::M {})
}
