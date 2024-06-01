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
- Use `schema::generate` in your `build.rs` file to create bindings for your schema. (for an example see below),
- Use the `new_query` functions on either `Client` or `rusqlite::Connection` to start writing queries!

## Example/Practice
First download the `Chinook_Sqlite.sql` from here https://github.com/lerocha/chinook-database/releases and put it in the `chinook` folder of the rust-query repositorty.

Then you can run with `cd chinook` && `cargo run`

There are some queries there that you can implement to test out the query builder.