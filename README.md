# Type safe queries using the rust type system.
The goal of this library is to allow writing relational database queries using familiar rust syntax.
Writing queries using this library involves:
- Symbolic execution of rust closures.
- Assigning row/column references to variables.
- Lifetimes to check the scopes of row/column references.
- Imperative mutation of row sets with methods like `filter` and `join`.

Notably it does not involve any new syntax or macro, while still being completely type safe.
The library should guarantee that a query can not fail if it compiles.
This already includes preventing use after free for row ids passed between queries and even database migrations!

## Current limitations
This project is under development and currently has a number of limitations.
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
fn do_stuff_with_database(db: &mut WriteTransaction<MySchema>) {
    // Lets make a new user 'mike',
    let mike = UserDummy { name: "mike" };
    let mike_id = db.try_insert(mike).unwrap();
    // and also insert a dog picture for 'mike'.
    let dog_picture = ImageDummy {
        description: "dog",
        uploaded_by: mike_id,
    };
    db.try_insert(dog_picture).unwrap();

    // Now we want to get all pictures for 'mike'.
    let mike_pictures = db.exec(|rows| {
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
```
Some featurs not shown in this example are:
- Migrations and unique constraints
- Lookups by unique constraint
- Aggregations


## Examples
For more example queries you can look at [the chinook example](/tests/chinook.rs).

First download the `Chinook_Sqlite.sql` from here https://github.com/lerocha/chinook-database/releases and put it in the `tests/chinook_schema` folder of this repository.

Then you can run with `cargo test`.
