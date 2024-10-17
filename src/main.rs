mod ws;
mod players;

use axum::{routing::get, Router};
use std::net::SocketAddr;
use tokio::sync::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use players::Player;

type Players = Arc<RwLock<HashMap<String, Player>>>;

#[tokio::main]
async fn main() {
    let players = Players::default();
    let app = Router::new().route("/ws", get({
        let players = players.clone();
        move |ws| ws::websocket_handler(ws, players)
    }));

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("Listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

