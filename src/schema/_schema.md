Multi-versioned schema definition macro.

```
# use rust_query::migration::schema;
#[schema(SchemaName)]
#[version(0..=0)]
pub mod vN {
    pub struct TableName {
        pub column_name: i64,
    }
}
use v0::TableName; // the actual module name is `v0`
# fn main() {}
```

Supported data types are:
- `i64` (sqlite `integer`)
- `f64` (sqlite `real`)
- `String` (sqlite `text`)
- `Vec<u8>` (sqlite `blob`)
- `bool` (sqlite `integer` with `CHECK "col" IN (0, 1)`)
- `TableRow<T>` where `T` is any table in the same schema (sqlite `integer` with foreign key constraint)
- `Option<T>` where `T` is not an `Option` (sqlite nullable)
- `jiff::Date` (sqlite `text` with `CHECK "col" IS ltrim(date("col"), '-')`)
- `jiff::Timestamp` (sqlite `text` with `CHECK "col" IS (ltrim(datetime("col" || 'Z'), '-') || rtrim(substr("col", 20, 10), '0 '))`)

# Table attributes

Table names are `snake_case` versions of the rust struct names.
So a struct `FooNew` will be called `foo_new` in the database.

- `#[version(1..)`, `#[version(..4)`, `#[version(2..3)`:
This specifies the range of schema versions in which this table exists. The range can be unbounded
on either side which means the table exists for all versions in that direction. 
Note that it is possible to have tables with the same name as long as they don't exist in the same version of the schema.
The default is `#[version(..)]`.
- `#[unique(some, list, of, columns)]`:
Create a (multi) column unique constraint on the specified columns of the table.
- `#[index(some, list, of, columns)]`:
Create a (multi) column index without unique constraint on the specified columns of the table.
- `#[primary_key("some_col")]`:
Rename the primary key of the table. Note that the primary key must be an `INTEGER PRIMARY KEY` and must not
be used as the name of a regular column.
The primary key is only used for foreign key constraints and can not be queried using `rust_query`.
If you want a readable key, then you have to use a `unique` constraint instead of a primary key.
The default is `#[primary_key("id")]`.
- `#[no_reference]`:
This makes it impossible for any table to have a foreign key constraint to this table.
Required if you want to use [crate::TransactionWeak::delete_ok].
- `#[from(PrevTable)]`:
This attribute can be used to initialize the table from another table when it is created.
See the [example](#table-level-changes) for how this can be used.

# Column attributes

- `#[version(1..)`, `#[version(..4)`, `#[version(2..3)`:
This specifies the range of schema versions in which this column exists. The range can be unbounded
on either side which means the column exists for all version of the table in that direction. 
Note that it is possible to have columns with the same name as long as they don't exist in the same version of the table.
The default is `#[version(..)]`.
- `#[unique]`:
Create a single column unique constraint with the column that it is applied to.
- `#[index]`:
Create a single column index with column that it is applied to.

# Trivial changes

`rust_query` allows changing some schema details without a new schema version.
These changes are made retroactively when the database is opened with a compatible schema.
The following changes are trivial:

- Changing the order of columns in unique constraints and indices.
- Adding and removing indices (not unique).

# Column level changes

Lets say we have a username and want to make it unique.
This is a column level change so we can do it without `#[from(PrevTable)]`.
```
# use rust_query::migration::schema;
#[schema(Schema)]
#[version(0..=1)]
pub mod vN {
    pub struct User {
        #[version(..1)]
        pub username: String,
        #[version(1..)]
        #[unique]
        pub username: String,
    }
}
# fn main() {}
```
We had to copy the definition of the column. The new column has the same name as the old column, but both columns exists only in different versions of the table so it is fine.

# Table level changes
One example of a table level change is renaming a table `foo` to `foo_new`:

```
# use rust_query::migration::schema;
#[schema(SchemaName)]
#[version(0..=1)]
pub mod vN {
    #[version(..1)]
    pub struct Foo {
        pub name: String,
    }
    #[version(1..)]
    #[from(Foo)]
    pub struct FooNew {
        pub name: String
    }
    pub struct Other {
        pub with_foo: rust_query::TableRow<FooNew>,
    }
}
# fn main() {}
```
A lot is going on here:

- First note that we had to copy the whole definition of `Foo` in order to rename it.
That is because the name of the table is a table level property. While it is a bit annoying to copy the full definition if you just want to rename a table, this gives a lot of flexibility.
- The `Other` struct references the `Foo` table in the old version and `FooNew` in the new version. This works because old versions of structs are automatically resolved using the `#[from]` attribute. The `Other` table is automatically migrated to update the foreign key.
- We could have named the new table `Foo` if we wanted to change table level properties other than the name. This would not conflict with the old `Foo` because they live in different versions.
- We could have left the version range of `Foo` be unbounded, in that case `Foo` and `FooNew` would both exist in the new schema version.


# Using indexes and unique constraints

To define a unique constraint on a column, you need to add an attribute to the table or field.
For example:
```
#[rust_query::migration::schema(SchemaName)]
pub mod vN {
    #[unique(username, movie)] // <-- here
    pub struct Rating {
        pub movie: String,
        pub username: String,
        #[unique] // <-- or here
        pub uuid: String,
    }
}
```
This will create a single schema version with a single table called `rating` and three columns.
The table will also have two unique contraints (one on the `uuid` column and one on the combination of `username` and `movie`).
Note that optional types are not allowed in unique constraints.

## Indexes
Indexes are very similar to unique constraints, but they don't require the columns to be unique.
These are useful to prevent sqlite from having to scan a whole table.
To incentivise creating indices you also get some extra sugar to use the index!
```
#[rust_query::migration::schema(SchemaName)]
pub mod vN {
    pub struct Topic {
        #[unique]
        pub title: String,
        #[index]
        pub category: String,
    }
}
fn test(txn: &rust_query::Transaction<v0::SchemaName>) {
    let _ = txn.lazy_iter(v0::Topic.category("sports"));
    let _ = txn.lazy(v0::Topic.title("star wars"));
}
```
The `TableName.column_name(value)` syntax is only allowed if `TableName` has an index or
unique constraint that starts with `column_name`.
Adding and removing indexes and changing the order of columns in indexes and unique constraints
is considered backwards compatible and thus does not require a new schema version.

# Multiple schema versions
At some point you might want to change something substantial in your schema.
It would be really sad if you had to throw away all the old data in your database.
That is why [rust_query] allows us to define multiple schema versions and how to transition between them.

## Adding tables
One of the simplest things to do is adding a new table.
```
#[rust_query::migration::schema(SchemaName)]
#[version(0..=1)]
pub mod vN {
    pub struct User {
        #[unique]
        pub username: String,
    }
    #[version(1..)] // <-- note that `Game`` has a version range
    pub struct Game {
        #[unique]
        pub name: String,
        pub size: i64,
    }
}
// These are just examples of tables you can use
use v0::SchemaName as _;
use v1::SchemaName as _;
use v0::User as _;
use v1::User as _;
// `v0::Game` does not exist
use v1::Game as _;
# fn main() {}
fn migrate() -> rust_query::Database<v1::SchemaName> {
    rust_query::Database::migrator(rust_query::migration::Config::open("test.db"))
        .expect("database version is before supported versions")
        .migrate(|_txn| v0::migrate::SchemaName {})
        .finish()
        .expect("database version is after supported versions")
}
```
The migration itself is not very interesting because new tables are automatically created
without any data. To have some initial data, take a look at the `#[from]` attribute down below or use
[crate::migration::Migrator::fixup].

## Changing columns
Changing columns is very similar to adding and removing structs.
```
use rust_query::migration::{schema, Config};
use rust_query::{Database, Lazy};
#[schema(Schema)]
#[version(0..=1)]
pub mod vN {
    pub struct User {
        #[unique]
        pub username: String,
        #[version(1..)] // <-- here
        pub score: i64,
    }
}
pub fn migrate() -> Database<v1::Schema> {
    Database::migrator(Config::open_in_memory()) // we use an in memory database for this test
        .expect("database version is before supported versions")
        .migrate(|txn| v0::migrate::Schema {
            // In this case it is required to provide a value for each row that already exists.
            // This is done with the `v0::migrate::User` struct:
            user: txn.migrate_ok(|old: Lazy<v0::User>| v0::migrate::User {
                score: old.username.len() as i64 // use the username length as the new score
            }),
        })
        .finish()
        .expect("database version is after supported versions")
}
# fn main() {}
```

## `#[from(TableName)]` Attribute
You can use this attribute when renaming or splitting a table.
This will make it clear that data in the table should have the
same row ids as the `from` table.
For example:
```
# use rust_query::migration::{schema, Config};
# use rust_query::{Database, Lazy};
# fn main() {}
#[schema(Schema)]
#[version(0..=1)]
pub mod vN {
    #[version(..1)]
    pub struct User {
        pub name: String,
    }
    #[version(1..)]
    #[from(User)]
    pub struct Author {
        pub name: String,
    }
    pub struct Book {
        pub author: rust_query::TableRow<Author>,
    }
}
pub fn migrate() -> Database<v1::Schema> {
    Database::migrator(Config::open_in_memory()) // we use an in memory database for this test
        .expect("database version is before supported versions")
        .migrate(|txn| v0::migrate::Schema {
            author: txn.migrate_ok(|old: Lazy<v0::User>| v0::migrate::Author {
                name: old.name.clone(),
            }),
        })
        .finish()
        .expect("database version is after supported versions")
}
```
In this example the `Book` table exists in both `v0` and `v1`,
however `User` only exists in `v0` and `Author` only exist in `v1`.
Note that the `pub author: Author` field only specifies the latest version
of the table, it will use the `#[from]` attribute to find previous versions.

## `#[no_reference]` Attribute
You can put this attribute on your table definitions and it will make it impossible
to have foreign key references to such table.
This makes it possible to use `TransactionWeak::delete_ok`.
