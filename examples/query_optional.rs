use rust_query::{
    migration::{schema, Config},
    optional, Database, FromDummy, LocalClient,
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

    let info: Option<PlayerInfo> = txn.query_one(Player::unique(pub_id).trivial());
}
