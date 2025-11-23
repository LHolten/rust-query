use std::{net::SocketAddr, sync::Arc};

use axum::{
    Router,
    extract::State,
    response::Json,
    routing::{get, post},
};
use rust_query::{
    Database, DatabaseAsync, FromExpr,
    migration::{Config, schema},
};

#[schema(Schema)]
pub mod vN {
    pub struct User {
        pub name: String,
        pub hair_color: Option<String>,
    }
}
use v0::*;

#[derive(serde::Deserialize, serde::Serialize, rust_query::FromExpr)]
#[rust_query(From = User)]
struct UserInfo {
    name: String,
    hair_color: Option<String>,
}

#[tokio::main]
async fn main() {
    let db = Database::new(Config::open_in_memory());
    let db = DatabaseAsync::new(Arc::new(db));

    // build our application with some routes
    let app = Router::new()
        .route("/user/list", get(list_users))
        .route("/user/create", post(create_user))
        .with_state(db);

    // run it with hyper
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn create_user(State(db): State<DatabaseAsync<Schema>>, Json(new_user): Json<UserInfo>) {
    db.transaction_mut_ok(|txn| {
        txn.insert_ok(User {
            name: new_user.name,
            hair_color: new_user.hair_color,
        });
    })
    .await
}

async fn list_users(State(db): State<DatabaseAsync<Schema>>) -> Json<Vec<UserInfo>> {
    db.transaction(|txn| {
        txn.query(|rows| {
            let user = rows.join(User);
            Json(rows.into_vec(UserInfo::from_expr(user)))
        })
    })
    .await
}
