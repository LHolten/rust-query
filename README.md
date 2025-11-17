[![Latest Version](https://img.shields.io/crates/v/rust-query.svg)](https://crates.io/crates/rust-query)
[![Documentation](https://docs.rs/rust-query/badge.svg)](https://docs.rs/rust-query)

- [Overview of types](#overview-of-types)
- [How to provide `IntoSelect`](#how-to-provide-intoselect)
- [How to work with optional rows](#how-to-work-with-optional-rows)
- [FAQ](#faq)
- [What it looks like](#what-it-looks-like)

## Overview of types

There is a hierarchy of types that can be used to build queries.
- [TableRow], [i64], [f64], [bool], `&[u8]`, `&str`:
  These are the base types for building expressions. They all
  implement [IntoExpr] and are [Copy]. Note that [TableRow] is special
  because it refers to a table row that is guaranteed to exist.
- [Expr] is the type that all [IntoExpr] values can be converted into.
  It has a lot of methods to combine expressions into more complicated expressions.
  Most importantly, it implements [std::ops::Deref], if the expression is a table expression.
  This can be used to get access to the columns of the table, which can themselves be table expressions.
  Note that combinators like [optional] and [aggregate] also have [Expr] as return type.
- `()`, [Expr] and `(Expr, Expr)` implement [IntoSelect]
  These types can be used as the return type of a query.
  They specify exactly which values should be returned for each row in the result set.
- [struct@Select] is the type that all [IntoSelect] value can be converted into.
  It has the [Select::map] method which allows changing the type that is returned from the query.

## How to provide [IntoSelect]

Making a selection of values to return for each row in the result set is the final step when
building queries. [rust_query] has many different methods of selecting.
- First, you can specify the columns that you want directly.
  `into_vec(&user.name)` or `into_vec((&user.name, some_other_expr))`
  Note that this method only supports tuples of size 2 (which can be nested).
  If you want to have more expressions, then you probably want to use one of the other methods.
- Derive [derive@Select], super useful when some of the values are aggregates.
- Derive [derive@FromExpr], choose this method if you just want (a subset of) existing columns.
- Finally, you can implement [trait@IntoSelect] manually, for maximum flexibility.

## How to work with optional rows

A single optional row is quite common as the result of using unique constraint.
For example you might create a `Expr<Option<User>>` with something like `User.name(name)`.
- [trait@FromExpr] is automatically implemented for `Option<T>` if it is implemented for `T`, so
  you can do something like `Option::<UserInfo>::from_expr(User.name(name))`.
- [Transaction::lazy] also works with optional rows, so you can write `txn.lazy(User.name(name))`.
- For more complicated queries you have to use [args::Optional::then_select].

## FAQ
- Q: How do I get a full row from the database?

  A: The [Lazy] type is most convenient if you want to use the row columns immediately.
  For other use cases, please take a look at the [other options](#how-to-provide-intoselect).
- Q: How do I retrieve some columns + the [TableRow] of a row?

  A: The [Lazy] type has a [Lazy::table_row] method to get the [TableRow].
- Q: Why is [TableRow] (and many other types) `!Send`?

  A: This prevents moving the [TableRow] between transactions. Moving a [TableRow] between transactions
  would make it possible for the refered row to already be deleted in the new transaction.


## What it looks like

Define a schema using the syntax of a module with structs:
```rust
# fn main() {}
use rust_query::migration::schema;

#[schema(MySchema)]
pub mod vN {
    // Structs are database tables
    pub struct User {
        // This table has one column with String (sqlite TEXT) type.
        pub name: String,
    }
    pub struct Image {
        pub description: String,
        // This column has a foreign key constraint to the User table
        pub uploaded_by: User,
    }
}
```
Initialize a database:
```rust,ignore
let database = Database::migrator(Config::open("my_database.sqlite"))
    .expect("database version is before supported versions")
    // migrations go here
    .finish()
    .expect("database version is after supported versions");
```
Perform a transaction!
```rust,ignore
database.transaction_mut_ok(|txn| {
    do_stuff_with_database(txn);
    // Changes are committed at the end of the closure!
});
```
Insert in the database:
```rust,ignore
// Lets make a new user 'mike',
let mike = User { name: "mike" };
let mike_id = txn.insert_ok(mike);
// and also insert a dog picture for 'mike'.
let dog_picture = Image {
    description: "dog",
    uploaded_by: mike_id,
};
let _picture_id = txn.insert_ok(dog_picture);
```
Query from the database:
```rust,ignore
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
```

## Roadmap

This project is under development and there are some (important) things missing.
Work is ongoing to fix this, [funded by NLnet!](https://nlnet.nl/project/rust-query/)
