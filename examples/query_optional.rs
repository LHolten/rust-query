use rust_query::{
    Database, FromExpr, LocalClient,
    migration::{Config, schema},
    optional,
};

// Start by defining your schema.
#[schema]
enum Schema {
    #[unique(pub_id)]
    Player {
        pub_id: i64,
        name: String,
        score: i64,
        home: World,
    },
    World {
        name: String,
    },
}
// Bring the latest schema version into scope.
use v0::*;

fn main() {
    let pub_id = 100;

    let mut client = LocalClient::try_new().unwrap();
    let database: Database<Schema> = client
        .migrator(Config::open_in_memory())
        .expect("database version is before supported versions")
        // migrations go here
        .finish()
        .expect("database version is after supported versions");

    let mut txn = client.transaction_mut(&database);

    #[derive(FromExpr)]
    #[rust_query(From = World, From = Player)]
    struct NameInfo {
        name: String,
    }

    #[derive(FromExpr)]
    #[rust_query(From = Player)]
    struct PlayerInfo {
        name: String,
        score: i64,
        home: NameInfo,
    }

    // old pattern, requires two queries
    let player = txn.query_one(Player::unique(pub_id));
    let _info = player.map(|player| txn.query_one(PlayerInfo::from_expr(player)));

    // most powerful pattern, can retrieve optional data in one query
    let _info = txn.query_one(optional(|row| {
        let player = row.and(Player::unique(pub_id));
        row.then_dummy(PlayerInfo::from_expr(player))
    }));

    // for simple queries, use the trivial mapping
    let info = txn.query_one(Option::<PlayerInfo>::from_expr(Player::unique(pub_id)));

    assert!(info.is_none());

    let home = txn.insert(World { name: "Dune" });
    txn.try_insert(Player {
        pub_id,
        name: "Asterix",
        score: 3000,
        home,
    })
    .expect("there is no player with this pub_id yet");

    let info = txn.query_one(Option::<PlayerInfo>::from_expr(Player::unique(pub_id)));
    assert!(info.is_some())
}

#[test]
fn run() {
    main();
}

#[test]
#[cfg(feature = "dev")]
fn schema_hash() {
    use expect_test::expect;
    use rust_query::migration::hash_schema;
    expect!["93ca1485f9eba782"].assert_eq(&hash_schema::<v0::Schema>());
}
