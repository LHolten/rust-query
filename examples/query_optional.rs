use rust_query::{
    migration::{schema, Config},
    optional, Database, FromDummy, IntoColumn, LocalClient,
};

// Start by defining your schema.
#[schema]
enum Schema {
    Player {
        #[unique]
        pub_id: i64,
        name: String,
        score: i64,
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

    let txn = client.transaction(&database);

    #[derive(FromDummy)]
    #[trivial(Player)]
    struct PlayerInfo {
        name: String,
        score: i64,
    }

    // old pattern, requires two queries
    let player = txn.query_one(Player::unique(pub_id));
    let info = player.map(|player| {
        txn.query_one(PlayerInfoDummy {
            name: player.name(),
            score: player.score(),
        })
    });

    // most powerful pattern, can retrieve optional data in one query
    let info = txn.query_one(optional(|row| {
        let player = row.and(Player::unique(pub_id));
        row.then_dummy(PlayerInfoDummy {
            name: player.name(),
            score: player.score(),
        })
    }));

    // for simple queries, use the trivial mapping, this requries type annotation
    let info: Option<PlayerInfo> = txn.query_one(Player::unique(pub_id).trivial());
}
