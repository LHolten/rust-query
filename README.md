# This is a WIP rust query builder.
WIP means that it is probably not yet suitable for your project, but you can try it out and give me feedback.

The idea is to have a deep embedding, this means that we reuse rust concepts for queries:
- Query ~ Function
- Column ~ Variable
- Scope ~ Lifetime
- Query phase ~ Mutability

Using the full expressiveness of the rust type system like this allows us to write queries that can not fail at runtime and get (nice) error messages.

## Current limitations
This is a WIP project and thus has a number of limitations.
- Only supports sqlite (with rusqlite).
- Only support for select and insert statements.
- Very small number of operators.
- No support for window functions.
- Etc.

Despite these limitations, I am dogfooding this query builder and using it in my own project: [advent-of-wasm](https://github.com/LHolten/advent-of-wasm).

## How to Use

Here is a complete example of how to use this library.

```rust
use rust_query::{
    migration::{schema, Prepare},
    ThreadToken, Value, WriteTransaction,
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
    let mut token = ThreadToken::try_new().unwrap();
    let database = Prepare::open("my_database.sqlite")
        .create_db_empty()
        .expect("database is version is before supported versions")
        // migrations go here
        .finish(&mut token)
        .expect("database version is after supported versions");

    let mut transaction = database.write_lock(&mut token);
    do_stuff_with_database(&mut transaction);
    // After we are done we commit the changes!
    transaction.commit();
}

// Use the database to insert and query.
fn do_stuff_with_database(db: &mut WriteTransaction<MySchema>) {
    // Lets make a new user 'mike'
    let mike = UserDummy { name: "mike" };
    let mike_id = db.try_insert(mike).unwrap();
    // and also insert a dog picture for 'mike'
    let dog_picture = ImageDummy {
        description: "dog",
        uploaded_by: mike_id,
    };
    db.try_insert(dog_picture).unwrap();

    // now we want to get all pictures for 'mike'
    let mike_pictures = db.exec(|rows| {
        // Initially there is one empty row.
        // Lets join the pictures table
        let picture = Image::join(rows);
        // Now lets filter for pictures from mike
        rows.filter(picture.uploaded_by().eq(mike_id));
        // and finally turn the rows into a vec
        rows.into_vec(picture.description())
    });

    println!("{mike_pictures:?}"); // this should print `vec!["dog"]`
}
```
Some featurs not shown in this example are:
- Migrations and unique constraints
- Lookups by unique constraint
- Aggregations

For more example queries you can look at [the chinook example](/tests/chinook.rs).

## Example/Practice
First download the `Chinook_Sqlite.sql` from here https://github.com/lerocha/chinook-database/releases and put it in the `chinook` folder of the rust-query repositorty.

Then you can run with `cd chinook` && `cargo run`

There are some queries there that you can implement to test out the query builder.