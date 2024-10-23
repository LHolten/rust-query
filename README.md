# Type safe queries using the Rust type system
The goal of this library is to allow writing relational database queries using familiar Rust syntax.
The library should guarantee that a query can not fail if it compiles.
This already includes preventing use after free for row ids passed between queries and even database migrations!

Writing queries using this library involves:
- Interact with row/column references as Rust values.
- Lifetimes to check the scopes of row/column references.
- Imperative mutation of row sets with methods like `filter` and `join`.

Notably it does not involve any new syntax or macro, while still being completely type safe.

## Roadmap

This project is under development and there are many things missing.

Query types:
- [x] SELECT
- [x] INSERT
- [ ] DELETE
- [ ] UPDATE

Basic operations:
- [x] Eq, Add, Not, And, Lt, UnwrapOr, IsNotNull, AsFloat
- [ ] Everything else

Advanced operations:
- [x] Aggregate
- [ ] Limit

Backend support:
- [x] sqlite
- [ ] postgres
- [ ] duckdb

Despite these limitations, I am dogfooding this query builder and using it in my own project: [advent-of-wasm](https://github.com/LHolten/advent-of-wasm).

## What it looks like

Define a schema using `enum` syntax:
```rust
use rust_query::migration::schema;

#[schema]
enum MySchema {
    // Enum variants are database tables
    User {
        // This table has one column with String type.
        name: String,
    },
    Image {
        description: String,
        // This column has a foreign key constraint to the User table
        uploaded_by: User,
    },
}
```
Get proof that we are running on a unique thread:
```rust
let mut token = ThreadToken::try_new().unwrap();
```
Initialize a database:
```rust
let database = Prepare::open("my_database.sqlite")
    .create_db_empty()
    .expect("database version is before supported versions")
    // migrations go here
    .finish(&mut token)
    .expect("database version is after supported versions");
```
Perform a transaction!
```rust
let mut transaction = database.write_lock(&mut token);
do_stuff_with_database(&mut transaction);
// After we are done we commit the changes!
transaction.commit();
```
Insert in the database:
```rust
// Lets make a new user 'mike',
let mike = UserDummy { name: "mike" };
let mike_id = db.try_insert(mike).unwrap();
// and also insert a dog picture for 'mike'.
let dog_picture = ImageDummy {
    description: "dog",
    uploaded_by: mike_id,
};
db.try_insert(dog_picture).unwrap();
```
Query from the database:
```rust
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
```
The full example code can be found in [insert_and_select.rs](examples/insert_and_select.rs)

Some features not shown in this example are:
- Migrations and unique constraints
- Lookups by unique constraint
- Aggregations


## Examples
For more example queries you can look at [the chinook example](/tests/chinook.rs).

First download the `Chinook_Sqlite.sqlite` from here https://github.com/lerocha/chinook-database/releases and put it in the root of this repository (the working dir).

Then you can run with `cargo test`.
