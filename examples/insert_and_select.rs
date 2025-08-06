use rust_query::{
    Database, Transaction,
    migration::{Config, schema},
};

// Start by defining your schema.
#[schema(MySchema)]
pub mod vN {
    pub struct User {
        pub name: String,
    }
    pub struct Image {
        pub description: String,
        pub uploaded_by: User,
    }
}
// Bring the latest schema version into scope.
use v0::*;

// Use your schema to initalize a database.
fn main() {
    let database = Database::migrator(Config::open_in_memory())
        .expect("database version is before supported versions")
        // migrations go here
        .finish()
        .expect("database version is after supported versions");

    database.transaction_mut_ok(|mut txn| {
        do_stuff_with_database(&mut txn);
        // After we are done we commit the changes!
    })
}

// Use the database to insert and query.
fn do_stuff_with_database(txn: &mut Transaction<MySchema>) {
    // Lets make a new user 'mike',
    let mike = User { name: "mike" };
    let mike_id = txn.insert_ok(mike);

    // and also insert a dog picture for 'mike'.
    let dog_picture = Image {
        description: "dog",
        uploaded_by: mike_id,
    };
    let _picture_id = txn.insert_ok(dog_picture);

    // Now we want to get all pictures for 'mike'.
    let mike_pictures = txn.query(|rows| {
        // Initially there is one empty row.
        // Lets join the pictures table.
        let picture = rows.join(Image);
        // Now lets filter for pictures from mike,
        rows.filter(picture.uploaded_by.eq(mike_id));
        // and finally turn the rows into a vec.
        rows.into_vec(&picture.description)
    });

    println!("{mike_pictures:?}"); // This should print `["dog"]`.
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
    expect!["e6dbf93daba3ccfa"].assert_eq(&hash_schema::<v0::MySchema>());
}
