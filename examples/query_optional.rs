use rust_query::{
    migration::{schema, Config},
    optional, Column, Database, FromDummy, LocalClient,
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

#[derive(FromDummy)]
struct PlayerInfo {
    name: String,
    score: i64,
}

impl PlayerInfo {
    fn dummy(
        player: Column<'_, Schema, Player>,
    ) -> PlayerInfoDummy<Column<'_, Schema, String>, Column<'_, Schema, i64>> {
        PlayerInfoDummy {
            name: player.name(),
            score: player.score(),
        }
    }
}

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
    if let Some(info) = txn.query_one(optional(|row| {
        let player = row.and(Player::unique(pub_id));
        row.then_dummy(PlayerInfo::dummy(player))
    })) {
        println!("player {} has score {}", info.name, info.score);
    }
}