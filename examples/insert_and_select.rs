use rust_query::{
    migration::{schema, Prepare},
    Table, ThreadToken, TransactionMut,
};

// Start by defining your schema.
#[schema]
enum MySchema {
    User {
        name: String,
    },
    Image {
        description: String,
        uploaded_by: User,
    },
}
// Bring the latest schema version into scope.
use v0::*;

// Use your schema to initalize a database.
fn main() {
    // Get a token to prove that we have our own thread.
    // This is necessary to keep transactions separated.
    let mut token = ThreadToken::try_new().unwrap();
    let database = Prepare::open("my_database.sqlite")
        .create_db_empty()
        .expect("database version is before supported versions")
        // migrations go here
        .finish(&mut token)
        .expect("database version is after supported versions");

    let mut transaction = database.write_lock(&mut token);
    do_stuff_with_database(&mut transaction);
    // After we are done we commit the changes!
    transaction.commit();
}

// Use the database to insert and query.
fn do_stuff_with_database(db: &mut TransactionMut<MySchema>) {
    // Lets make a new user 'mike',
    let mike = User { name: "mike" };
    let mike_id = db.try_insert(mike).unwrap();
    // and also insert a dog picture for 'mike'.
    let dog_picture = Image {
        description: "dog",
        uploaded_by: mike_id,
    };
    db.try_insert(dog_picture).unwrap();

    // Now we want to get all pictures for 'mike'.
    let mike_pictures = db.query(|rows| {
        // Initially there is one empty row.
        // Lets join the pictures table.
        let picture = Image::join(rows);
        // Now lets filter for pictures from mike,
        rows.filter(picture.uploaded_by().eq(mike_id));
        // and finally turn the rows into a vec.
        rows.into_vec(picture.description())
    });

    println!("{mike_pictures:?}"); // This should print `["dog"]`.
}
