[![Latest Version](https://img.shields.io/crates/v/rust-query.svg)](https://crates.io/crates/rust-query)
[![Documentation](https://docs.rs/rust-query/badge.svg)](https://docs.rs/rust-query)

```rust
# use rust_query::{migration::{schema, Config}, aggregate, Database};
# use std::fs;
#[schema(MySchema)]
pub mod vN {
    // Structs are database tables.
    pub struct User {
        pub name: String,
    }
    pub struct Image {
        // This column has a unique constraint.
        #[unique]
        pub file_name: String,
        // This column has an index and a foreign key constraint to the `User` table.
        #[index]
        pub uploaded_by: User,
    }
}
// Import the table names from the schema we just created.
use v0::*;

fn main() {
    # let _ = fs::remove_file("my_database.sqlite");
    let database = Database::new(Config::open("my_database.sqlite"));

    database.transaction_mut_ok(|txn| {
        // First we insert a new `User` in the database.
        // There are no unique constraints on this table, so no errors to handle.
        let mike = txn.insert_ok(User { name: "mike" });
        // Inserting an `Image` can fail, because of the unique constraint on
        // the `file_name` column.
        txn.insert(Image {
            file_name: "dog.png",
            uploaded_by: mike,
        })
        .expect("no other file called `dog.png` should exist");
    }); // Changes are committed at the end of the closure!

    database.transaction_mut_ok(|txn| {
        let ref dog = txn.lazy(Image.file_name("dog.png")).expect("`dog.png` should exist");
        // Note that this automatically retrieves the `User` row that matches the image!
        println!("`dog` image was uploaded by {}", dog.uploaded_by.name);
        
        let ref user = dog.uploaded_by;
        let upload_count = txn.query_one(aggregate(|rows| {
            let user_images = rows.join(Image.uploaded_by(user));
            // No error handling is required for this aggregate, an integer is always returned.
            // This works even if `rows` is empty.
            rows.count_distinct(user_images)
        }));
        println!("{} uploaded {} images", user.name, upload_count);
        
        // Since we are going to do mutations now, we need to disable
        // the automatic retrieval of column data for the rows that
        // we still want to use.
        let dog = dog.table_row();
        let user = user.table_row();

        let paul = txn.insert_ok(User { name: "paul" });
        // We can mutate rows with a simple assignment.
        txn.mutable(dog).uploaded_by = paul;
    
        // Deleting happens in a separate transaction mode.
        let txn = txn.downgrade();
        // Since users can be referenced by images, we need to handle a potential error.
        txn.delete(user).expect("no images should refer to this user anymore");
    });
}
```

## When to use

Use this library if you want to fearlessly query and migrate your SQLite 
database with a Rusty API build on an encoding of your schema in types.

Do not use `rust-query` if you want a zero-cost abstraction.
The focus of this project is on bringing errors to compile-time and
generally making transactions easier to write.

Here are some errors that `rust-query` can prevent at compile-time:
- Column type errors.
- Foreign key violations on insert/update.
- Mismatches in number of returned rows (zero, one or multiple).
- Use of undefined columns.
- SQL syntax errors.

Some other errors cannot be prevented at compile-time, but they can
be turned into `Result` types so that the user is aware of them:
- Unique constraint errors.
- Foreign key violations on delete.

Next to those features, `rust-query` also helps writing complex queries:
- Reuse part of your query in another query by extracting it into a Rust function.
Query types are kept simple so that the required function signature is easy to write.
- Aggregates that always return a single row make it easier to reason about queries.
- Automatic decorrelation of correlated sub-queries makes it possible to run those on SQLite.

Note that this project is still in relatively early stages.
There might be bugs to catch, so if you are worried about that, then don't use this yet.

<!--
## how to work with optional rows

a single optional row is quite common as the result of using unique constraint.
for example you might create a `expr<option<user>>` with something like `user.name(name)`.
- [trait@fromexpr] is automatically implemented for `option<t>` if it is implemented for `t`, so
  you can do something like `option::<userinfo>::from_expr(user.name(name))`.
- [transaction::lazy] also works with optional rows, so you can write `txn.lazy(user.name(name))`.
- for more complicated queries you have to use [args::optional::then_select].

## faq
- q: how do i get a full row from the database?

  a: the [lazy] type is most convenient if you want to use the row columns immediately.
  for other use cases, please take a look at the [other options](#how-to-provide-intoselect).
- q: how do i retrieve some columns + the [tablerow] of a row?

  a: the [lazy] type has a [lazy::table_row] method to get the [tablerow].
- q: why is [tablerow] (and many other types) `!send`?

  a: this prevents moving the [tablerow] between transactions. moving a [tablerow] between transactions
  would make it possible for the refered row to already be deleted in the new transaction.-->

## Roadmap

The current focus is on making the library more accessible and more generally useful.
[Funded by NLnet!](https://nlnet.nl/project/rust-query/)
