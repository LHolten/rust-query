use rust_query::{
    aggregate,
    migration::{schema, Alter, Config},
    Database, Dummy, LocalClient, Table, Transaction, TransactionMut,
};

#[schema]
#[version(0..=1)]
enum Schema {
    User {
        name: String,
        #[version(1..)]
        email: String,
    },
    Story {
        author: User,
        title: String,
        content: String,
    },
    #[unique(user, story)]
    Rating {
        user: User,
        story: Story,
        stars: i64,
    },
}
use v0::*;

fn insert_data(txn: &mut TransactionMut<Schema>) {
    // Insert users
    let alice = txn.insert(User { name: "alice" });
    let bob = txn.insert(User { name: "bob" });

    // Insert a story
    let dream = txn.insert(Story {
        author: alice,
        title: "My crazy dream",
        content: "A dinosaur and a bird...",
    });

    // Insert a rating - note the try_insert due to the unique constraint
    let _rating = txn
        .try_insert(Rating {
            user: bob,
            story: dream,
            stars: 5,
        })
        .expect("no rating for this user and story exists yet");
}

fn query_data(txn: &Transaction<Schema>) {
    let results = txn.query(|rows| {
        let story = Story::join(rows);
        let avg_rating = aggregate(|rows| {
            let rating = Rating::join(rows);
            rows.filter_on(rating.story(), &story);
            rows.avg(rating.stars().as_float())
        });
        rows.into_vec((story.title(), avg_rating))
    });

    for (title, avg_rating) in results {
        println!("story '{title}' has avg rating {avg_rating:?}");
    }
}

pub fn migrate(client: &mut LocalClient) -> Database<v1::Schema> {
    let m = client
        .migrator(Config::open("my-database.sqlite"))
        .expect("database is older than supported versions");
    let m = m.migrate(v1::update::Schema {
        user: Box::new(|old_user| {
            Alter::new(v1::update::UserMigration {
                email: old_user
                    .name()
                    .map_dummy(|name| format!("{name}@example.com")),
            })
        }),
    });
    m.finish()
        .expect("database is newer than supported versions")
}

fn main() {}
