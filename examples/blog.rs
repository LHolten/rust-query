use rust_query::{
    Database, LocalClient, Table, Transaction, TransactionMut, aggregate,
    migration::{Config, schema},
};

#[schema(Schema)]
#[version(0..=1)]
pub mod vN {
    pub struct User {
        pub name: String,
        #[version(1..)]
        pub email: String,
    }
    pub struct Story {
        pub author: User,
        pub title: String,
        pub content: String,
    }
    #[unique(user, story)]
    pub struct Rating {
        pub user: User,
        pub story: Story,
        pub stars: i64,
    }
}
use v1::*;

fn insert_data(txn: &mut TransactionMut<Schema>) {
    // Insert users
    let alice = txn.insert_ok(User {
        name: "alice",
        email: "test",
    });
    let bob = txn.insert_ok(User {
        name: "bob",
        email: "test",
    });

    // Insert a story
    let dream = txn.insert_ok(Story {
        author: alice,
        title: "My crazy dream",
        content: "A dinosaur and a bird...",
    });

    // Insert a rating - note the try_insert due to the unique constraint
    let _rating = txn
        .insert(Rating {
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
        .migrator(Config::open_in_memory())
        .expect("database is older than supported versions");
    let m = m.migrate(|txn| v1::migrate::Schema {
        user: txn.migrate_ok(|old_user: v0::User!(name)| v1::migrate::UserMigration {
            email: format!("{}@example.com", old_user.name),
        }),
    });
    m.finish()
        .expect("database is newer than supported versions")
}

fn main() {
    let mut client = LocalClient::try_new().unwrap();
    let db = migrate(&mut client);
    let mut txn = client.transaction_mut(&db);
    insert_data(&mut txn);
    query_data(&txn);
}

#[test]
fn run() {
    main();
}

#[test]
#[cfg(feature = "dev")]
fn schema_hash() {
    use expect_test::expect;
    use rust_query::migration::hash_schema;
    expect!["dd7f5d2f553f5b7a"].assert_eq(&hash_schema::<v0::Schema>());
    expect!["66e6a7d64535bcda"].assert_eq(&hash_schema::<v1::Schema>());
}
