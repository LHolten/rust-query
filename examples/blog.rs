use rust_query::{
    Database, Lazy, Transaction, aggregate,
    migration::{Config, schema},
};

#[schema(Schema)]
#[version(0..=1)]
pub mod vN {
    /// This is a doc comment for the User table!
    pub struct User {
        pub name: String,
        /// This is a doc comment for the email column
        #[version(1..)]
        pub email: String,
    }
    pub struct Story {
        pub author: User,
        pub title: String,
        pub content: String,
    }
    #[unique(story, user)]
    pub struct Rating {
        pub user: User,
        pub story: Story,
        pub stars: i64,
    }
}
use v1::*;

fn insert_data(txn: &mut Transaction<Schema>) {
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
        let story = rows.join(Story);
        let avg_rating = aggregate(|rows| {
            let rating = rows.join(Rating.story(&story));
            rows.avg(rating.stars.as_float())
        });
        rows.into_vec((&story.title, avg_rating))
    });

    for (title, avg_rating) in results {
        println!("story '{title}' has avg rating {avg_rating:?}");
    }
}

pub fn migrate() -> Database<v1::Schema> {
    let m = Database::migrator(Config::open_in_memory())
        .expect("database is older than supported versions");
    let m = m.migrate(|txn| v0::migrate::Schema {
        user: txn.migrate_ok(|old_user: Lazy<v0::User>| v0::migrate::User {
            email: format!("{}@example.com", old_user.name),
        }),
    });
    m.finish()
        .expect("database is newer than supported versions")
}

fn main() {
    let db = migrate();
    db.transaction_mut_ok(|txn| {
        insert_data(txn);
        query_data(txn);
    })
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
    expect!["3c4d9f7d7bc109b"].assert_eq(&hash_schema::<v0::Schema>());
    expect!["58dfa4b8c494c72d"].assert_eq(&hash_schema::<v1::Schema>());
}
