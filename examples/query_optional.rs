use rust_query::{
    Database, FromExpr, Transaction,
    migration::{Config, schema},
    optional,
};

// Start by defining your schema.
#[schema(Schema)]
pub mod vN {
    pub struct Player {
        #[unique]
        pub pub_id: i64,
        pub name: String,
        pub score: i64,
        pub home: World,
    }
    pub struct World {
        pub name: String,
    }
}
// Bring the latest schema version into scope.
use v0::*;

fn main() {
    let database = Database::new(Config::open_in_memory());
    database.transaction_mut_ok(queries);
}

fn queries(txn: &'static mut Transaction<Schema>) {
    let pub_id = 100;

    #[expect(unused)]
    #[derive(FromExpr)]
    #[rust_query(From = World, From = Player)]
    struct NameInfo {
        name: String,
    }

    #[expect(unused)]
    #[derive(FromExpr)]
    #[rust_query(From = Player)]
    struct PlayerInfo {
        name: String,
        score: i64,
        home: NameInfo,
    }

    // old pattern, requires two queries
    let player = txn.query_one(Player.pub_id(pub_id));
    let _info = player.map(|player| txn.query_one(PlayerInfo::from_expr(player)));

    // most powerful pattern, can retrieve optional data in one query
    let _info = txn.query_one(optional(|row| {
        let player = row.and(Player.pub_id(pub_id));
        row.then_select(PlayerInfo::from_expr(player))
    }));

    // for simple queries, use the trivial mapping
    let info = txn.query_one(Option::<PlayerInfo>::from_expr(Player.pub_id(pub_id)));

    assert!(info.is_none());

    let home = txn.insert_ok(World { name: "Dune" });
    txn.insert(Player {
        pub_id,
        name: "Asterix",
        score: 3000,
        home,
    })
    .expect("there is no player with this pub_id yet");

    let info = txn.query_one(Option::<PlayerInfo>::from_expr(Player.pub_id(pub_id)));
    assert!(info.is_some());
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
    expect!["54aaeee4522f0aeb"].assert_eq(&hash_schema::<v0::Schema>());
}
