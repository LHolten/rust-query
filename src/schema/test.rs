use crate::{Database, migrate::Schema, migration::Config};

impl<S: Send + Sync + Schema> Database<S> {
    fn check_schema(&self, expect: expect_test::Expect) {
        let mut schema = self.transaction(|txn| txn.schema());
        schema.sort();
        expect.assert_eq(&schema.join("\n"));
    }
}

fn open_db<S: Schema>(file: &str) -> Database<S> {
    Database::new(Config::open(file))
}

#[test]
fn fix_indices1() {
    mod without_index {
        #[crate::migration::schema(Schema)]
        pub mod vN {
            pub struct Foo {
                pub bar: String,
            }
        }
    }

    mod with_index {
        #[crate::migration::schema(Schema)]
        pub mod vN {
            pub struct Foo {
                #[index]
                pub bar: String,
            }
        }
    }

    static FILE_NAME: &'static str = "index_test1.sqlite";
    let _ = std::fs::remove_file(FILE_NAME);

    let db = open_db::<without_index::v0::Schema>(FILE_NAME);
    // The first database is opened with a schema without index
    db.check_schema(expect_test::expect![[
        r#"CREATE TABLE "foo" ( "bar" text NOT NULL, "id" integer PRIMARY KEY ) STRICT"#
    ]]);

    let db_with_index = open_db::<with_index::v0::Schema>(FILE_NAME);
    // The database is updated without a new schema version.
    // Adding an index is allowed because it does not change database validity.
    db_with_index.check_schema(expect_test::expect![[r#"
            CREATE INDEX "foo_index_0" ON "foo" ("bar")
            CREATE TABLE "foo" ( "bar" text NOT NULL, "id" integer PRIMARY KEY ) STRICT"#]]);

    // Using the old database connection will still work, because the new schema is compatible.
    db.check_schema(expect_test::expect![[r#"
            CREATE INDEX "foo_index_0" ON "foo" ("bar")
            CREATE TABLE "foo" ( "bar" text NOT NULL, "id" integer PRIMARY KEY ) STRICT"#]]);

    let db = open_db::<without_index::v0::Schema>(FILE_NAME);
    // Opening the database with the old schema again removes the index.
    db.check_schema(expect_test::expect![[
        r#"CREATE TABLE "foo" ( "bar" text NOT NULL, "id" integer PRIMARY KEY ) STRICT"#
    ]]);
}

#[test]
fn fix_indices2() {
    mod normal {
        #[crate::migration::schema(Schema)]
        pub mod vN {
            #[unique(field1, baz)]
            pub struct Foo {
                pub field1: String,
                pub baz: String,
            }
        }
    }

    mod reversed {
        #[crate::migration::schema(Schema)]
        pub mod vN {
            #[unique(baz, field1)]
            pub struct Foo {
                pub field1: String,
                pub baz: String,
            }
        }
    }

    static FILE_NAME: &'static str = "index_test2.sqlite";
    let _ = std::fs::remove_file(FILE_NAME);

    let db = open_db::<normal::v0::Schema>(FILE_NAME);
    // The first database is opened with a schema with original index
    db.check_schema(expect_test::expect![[r#"
            CREATE TABLE "foo" ( "baz" text NOT NULL, "field1" text NOT NULL, "id" integer PRIMARY KEY ) STRICT
            CREATE UNIQUE INDEX "foo_index_0" ON "foo" ("field1", "baz")"#]]);

    let db_with_reversed = open_db::<reversed::v0::Schema>(FILE_NAME);
    // The database is updated without a new schema version.
    // Changing the index column order is allowed because it does not change database validity.
    db_with_reversed.check_schema(expect_test::expect![[r#"
            CREATE TABLE "foo" ( "baz" text NOT NULL, "field1" text NOT NULL, "id" integer PRIMARY KEY ) STRICT
            CREATE UNIQUE INDEX "foo_index_0" ON "foo" ("baz", "field1")"#]]);

    // Using the old database connection will still work, because the new schema is compatible.
    db.check_schema(expect_test::expect![[r#"
            CREATE TABLE "foo" ( "baz" text NOT NULL, "field1" text NOT NULL, "id" integer PRIMARY KEY ) STRICT
            CREATE UNIQUE INDEX "foo_index_0" ON "foo" ("baz", "field1")"#]]);

    let db = open_db::<normal::v0::Schema>(FILE_NAME);
    // Opening the database with the old schema again changes the index back.
    db.check_schema(expect_test::expect![[r#"
            CREATE TABLE "foo" ( "baz" text NOT NULL, "field1" text NOT NULL, "id" integer PRIMARY KEY ) STRICT
            CREATE UNIQUE INDEX "foo_index_0" ON "foo" ("field1", "baz")"#]]);
}

#[test]
fn diagnostics() {
    mod base {
        #[crate::migration::schema(Schema)]
        pub mod vN {
            #[unique(baz, field1)]
            pub struct Foo {
                pub field1: String,
                pub baz: String,
            }
        }
    }

    mod table_changes {
        #[crate::migration::schema(Schema)]
        pub mod vN {
            pub struct House {
                pub name: String,
            }
        }
    }

    mod column_changes {
        #[crate::migration::schema(Schema)]
        pub mod vN {
            #[unique(baz, field2)]
            pub struct Foo {
                pub field2: String,
                pub baz: i64,
            }
        }
    }

    static FILE_NAME: &'static str = "diagnostic_test.sqlite";
    let _ = std::fs::remove_file(FILE_NAME);

    open_db::<base::v0::Schema>(FILE_NAME);

    let err = std::panic::catch_unwind(|| {
        open_db::<table_changes::v0::Schema>(FILE_NAME);
    })
    .unwrap_err();
    expect_test::expect![[r#"
        error: Table mismatch for `#[version(0)]`
           ╭▸ src/schema/test.rs:130:36
           │
        LL │         #[crate::migration::schema(Schema)]
           │                                    ━━━━━━ database has table `foo`
        LL │         pub mod vN {
        LL │             pub struct House {
           ╰╴                       ━━━━━ database does not have this table"#]]
    .assert_eq(err.downcast_ref::<String>().unwrap());

    let err = std::panic::catch_unwind(|| {
        open_db::<column_changes::v0::Schema>(FILE_NAME);
    })
    .unwrap_err();
    expect_test::expect![[r#"
        error: Column mismatch for `#[version(0)]`
           ╭▸ src/schema/test.rs:142:24
           │
        LL │             pub struct Foo {
           │                        ━━━ database has column `field1: String`
        LL │                 pub field2: String,
           │                     ━━━━━━ database does not have this column
        LL │                 pub baz: i64,
           │                     ━━━ database column has type `String`
           ╰╴
        error: Unique constraint mismatch for `#[version(0)]`
           ╭▸ src/schema/test.rs:141:15
           │
        LL │             #[unique(baz, field2)]
           │               ━━━━━━ database does not have this unique constraint
        LL │             pub struct Foo {
           ╰╴                       ━━━ database has `#[unique(baz, field1)]`"#]]
    .assert_eq(err.downcast_ref::<String>().unwrap());
}
