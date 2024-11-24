use rust_query::{
    migration::{schema, Config},
    LocalClient, Table, TransactionMut,
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
    // Get a LocalClient to prove that we have our own thread.
    // This is necessary to keep transactions separated.
    let mut client = LocalClient::try_new().unwrap();
    let database = client
        .migrator(Config::open("my_database.sqlite"))
        .expect("database version is before supported versions")
        // migrations go here
        .finish()
        .expect("database version is after supported versions");

    let mut txn = client.transaction_mut(&database);
    do_stuff_with_database(&mut txn);
    // After we are done we commit the changes!
    txn.commit();
}

// Use the database to insert and query.
fn do_stuff_with_database(db: &mut TransactionMut<MySchema>) {
    // Lets make a new user 'mike',
    let mike = User { name: "mike" };
    let mike_id = db.insert(mike);

    // and also insert a dog picture for 'mike'.
    let dog_picture = Image {
        description: "dog",
        uploaded_by: mike_id,
    };
    let _picture_id = db.insert(dog_picture);

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
