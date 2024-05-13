use std::collections::HashMap;

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

// #[schema(prev = Empty)]
// struct Schema {
//     artist: Artist,
//     album: Album,
// }

pub fn migrate() {
    let artist_title = HashMap::from([("a", "b")]);
    ().migrate(|_schema| v1::M {
        album: |row, album| v1::M::Album {
            title: artist_title
                .get(row.get(album.artist.name))
                .copied()
                .unwrap_or("unknown"),
        },
    })
    .migrate(|_schema| v2::M {
        customer: |row, customer| v2::M::Customer {
            phone: row.get(customer.phone).map(|x| x.parse().ok()),
        },
    })
}
