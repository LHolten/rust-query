use rust_query::{
    migration::{schema, Config},
    optional, Database, Dummy, FromDummy, IntoColumn, LocalClient,
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
    fn dummy<'a, 'x>(
        col: impl IntoColumn<'a, Schema, Typ = Player>,
    ) -> impl Dummy<'a, 'x, Schema, Out = PlayerInfo> {
        let col = col.into_column();
        PlayerInfoDummy {
            name: col.name(),
            score: col.score(),
        }
    }
}

fn main() {
    let pub_id = 100;

    let mut client = LocalClient::try_new().unwrap();
    let database: Database<Schema> = client
        .migrator(Config::open("my_database.sqlite"))
        .expect("database version is before supported versions")
        // migrations go here
        .finish()
        .expect("database version is after supported versions");

    let txn = client.transaction(&database);
    let score = txn.query_one(optional(|row| {
        let player = row.and(Player::unique(pub_id));
        row.then_dummy(PlayerInfo::dummy(player))
    }));
}
