# Type safe queries using the Rust type system
The goal of this library is to allow writing relational database queries using familiar Rust syntax.
The library should guarantee that a query can not fail if it compiles.
This already includes preventing use after free for row ids passed between queries and even database migrations!

Writing queries using this library involves:
- Interact with row/column references as Rust values.
- Lifetimes to check the scopes of row/column references.
- Procedural mutation of row sets with methods like `filter` and `join`.

Notably writing queries does not involve any new syntax or macro, while still being completely type safe.
(There are macros to define the schema and to simplify defining composite types to retrieve from queries)

## Roadmap

This project is under development and there are some things missing.
Below is a checklist of planned features and implemented features. 
(Implemented features have a checkmark, planned features do not). 

Schema:
- [x] Basic types (integer, real, text, blob, null)
- [x] Basic foreign keys
- [x] (Multi column) unique constraints
- [ ] Check constraints
- [ ] Overlapping foreign keys

Statements:
- [x] Multi row query + single row query (and optional query)
- [x] Single row insert, update and delete

Expressions:
- [x] Eq, Add, Not, And, Lt, UnwrapOr, IsNotNull, AsFloat, Like
- [x] Aggregate combinator
- [x] Optional combinator
- [ ] Everything else

Advanced operations:
- [ ] Window
- [ ] Limit

Despite these limitations, I am dogfooding this query builder and using it in my own project: [advent-of-wasm](https://github.com/LHolten/advent-of-wasm).

## What it looks like

Define a schema using `enum` syntax:
```rust
# fn main() {}
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
```rust,ignore
let mut client = LocalClient::try_new().unwrap();
```
Initialize a database:
```rust,ignore
let database = client
    .migrator(Config::open("my_database.sqlite"))
    .expect("database version is before supported versions")
    // migrations go here
    .finish()
    .expect("database version is after supported versions");
```
Perform a transaction!
```rust,ignore
let mut transaction = client.transaction_mut(&database);
do_stuff_with_database(&mut transaction);
// After we are done we commit the changes!
transaction.commit();
```
Insert in the database:
```rust,ignore
// Lets make a new user 'mike',
let mike = User { name: "mike" };
let mike_id = db.insert_ok(mike);
// and also insert a dog picture for 'mike'.
let dog_picture = Image {
    description: "dog",
    uploaded_by: mike_id,
};
let _picture_id = db.insert_ok(dog_picture);
```
Query from the database:
```rust,ignore
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

## Examples
For more examples you can look at [the examples directory](/examples).
