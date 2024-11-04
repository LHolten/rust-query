# 0.2.2

- Bound the lifetime of `TableRow: IntoColumn` to the lifetime of the transaction. 
Without the bound it was possible to sneak `TableRow`s into following transacions. <details>
`query_one` now checks that its input lives for as long as the transaction.
To make sure that `query_one` still checks that the dummy is "global", the transaction now has an invariant lifetime.
</details>

# 0.2.1

- Relax `Transaction` creation to not borrow the `Database`.
- Add missing lifetime bound on `try_insert`s return value. 
Technically this is a breaking change, but it fixes a bug so it is still a patch release.
- Fix the version of the macro crate exactly (=0.2.0) to allow future internal API changes with only a patch release.

# 0.2.0

- Rewrote almost the whole library to specify the schema using enum syntax with a proc macro.
- Added a single Column type to handle a lot of query building.
- Dummy trait to retrieve multiple values at once and allow post processing.
- Added support for transactions and multiple schemas.

# 0.1.x

- This version was SQL schema first. It would generate the API based on the schema read from the database.