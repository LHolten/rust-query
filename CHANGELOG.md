# Unreleased

- Changed default `synchronous` to `FULL`.
- Added the option to congiure `synchronous` to `NORMAL`.

# 0.5.0

## Changed table column syntax
- Instead of methods `artist.name()`, you should now use fields `&artist.name`.
- `TableRow` does not have support for accessing columns anymore, instead convert the `TableRow` to an `Expr` using `IntoExpr`.
- Removed `ref_cast` dependency.

## All transactions now run on separate threads
- Removed all transaction lifetimes (`TableRow` no longer has a lifetime).
- Removed `LocalClient` (methods have been moved to `Database`).
- `Database::transaction` and `Database::transaction_mut` now accept a closure to run on a new thread.
- Removed `TransactionMut::commit` (commit now depends on the result returned from the transaction).
- Added `Database::transaction_mut_ok` for when the transaction is always commited.
- Removed `TransactionMut`, it is replaced by `&mut Transaction`.

## Other
- Removed deprecated `Table::join`.
- Removed deprecated `IntoSelectExt` (wit the `map_select` method).
- Removed deprecated `Aggregate::filter_on`.
- Updated to rusqlite 0.37

# 0.4.4

- Add support for doc comments on tables and columns.
- Add `Query::into_iter`, to lazily iterate over query results.

# 0.4.3

- Fix panic when inserting into table without columns.
- Add `Select::map` method.
- Deprecate `IntoSelectExt::map_select`.
- Deprecate `Aggregate::join_on`.

# 0.4.2

- Update the `Rows::join` method to take a constant argument.
This is now the prefered join syntax and all examples have been updated.
- Allow arbitrary correlated subqueries.
This means that `Aggregate` now has an implied bound that allows leaking `Expr` from the
out scope. Correlated subqueries are decorrelated before translating to SQL.
- Fix loose lifetime on `Optional`.

# 0.4.1

- Change conflicts back to using `TableRow` instead of `Expr`.
Changing the conflict type to `Expr` was a mistake, because the `Expr` can be invalidated.
- Fix `#[schema]` macro not showing errors for unique constraints.

# 0.4.0

Blog post: https://blog.lucasholten.com/rust-query-0-4/

## Optional Queries
- Added `optional` combinator.
- Changed `Expr` to be co-variant in its lifetime.

## Basic Datatypes and Operations
- Added support for `Vec<u8>` data type (sqlite `BLOB`).
- Added some more basic operations on expressions.

## Updates, Insert and Query
- Added safe updates of a subset of columns for each table.
- Update statements now use the `Update` type for each column.
- Insert and update conflict is now an `Expr` (`find_or_insert` returns an `Expr` now too).
- `Rows::into_vec` is no longer sorted automatically.

## Schema and Mirations
- Changed `#[schema]` syntax to be a module of structs.
- Added `#[from]` attribute to allow renaming tables and splitting tables in the schema.
- The generated migration structs have moved from e.g. `v1::update::UserMigration` to `v0::migrate::User`.
- Migrations now require explicit handling of potential unique constraint violations.
- Migrations now require explicit handling of foreign key violations.

## Type Driven Select
- Added a macro for each table to create ad-hoc column selection types like `User!(name, age)`.
- Added the `FromExpr` trait to allow custom column selection and conversion.

## Feature Flags and Dependencies
- `TransactionWeak::rusqlite_transaction` is renamed and no longer behind a feature flag.
- `hash_schema` method was moved behind `dev` feature which is enabled by default.
- Updated dependencies.

## Renaming
- Renamed `Dummy` to `Select`.
- Renamed `Column` to `Expr`.
- Renamed `try_insert` to `insert` and `insert` to `insert_ok`.
- Renamed `try_delete` to `delete` and `delete` to `delete_ok`.
- Renamed `try_update` to `update` and `update` to `update_ok`.

# 0.3.1

- Added error message when defining an `id` column.
- Added support for sqlite `LIKE` and `GLOB` operators (Contributed by @teamplayer3).
- Added support for `DELETE` using `TransactionWeak` and `#[no_reference]`.
- Added `TransactionWeak::unchecked_transaction` behind feature flag.
- Added `impl ToSql for TableRow` behind `unchecked_transaction` feature flag.
- Removed `impl RefCast for Transaction`, it was not intended to be public.
- Removed `impl FromSql for TableRow`, it was not intended to be public.

# 0.3.0

- Added support for updating rows.
- Added `Table::dummy` method, which makes it easier to do partial updates.
- Reused table types in the generated API for both naming `TableRow<User>` and dummies `User {name: "steve"}`.
- Forbid `Option` in unique constraints.
- Renamed `ThreadToken` to `LocalClient`.
- Renamed and moved `read` and `write_lock` to `transaction` and `transaction_mut`.
- Check `schema_version` at the start of every transaction.
- Simplify migration and borrow `LocalClient` only once.
- Renamed `Prepare` to `Config` and simplified its API.

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
