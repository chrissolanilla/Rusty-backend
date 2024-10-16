//our imports
//says to consider importing this funciton instead: use std::fmt::format
//it allso highlihgts message saying previous import of the type 'Message' here
use axum::{
    extract::ws::{Message as AxumMessage, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};

use tokio::sync::Mutex;

use std::{collections::HashMap, net::SocketAddr, sync::Arc};
// our own custom type for a player, it is an Arc? a mutex(changeable thing?) of a keyvalue pair of
// string and a arc that also have a mutex of webscoket
// arc stands for atomic reference count and allows shared ownership of data multiple threads can
// acccess it
// mutex ensures that only one thread can access the data at a time, protecting the hashmap
type Players = Arc<Mutex<HashMap<String, Arc<Mutex<WebSocket>>>>>;

#[tokio::main]
async fn main() {
    let players = Players::default();
    //clone the players into and move it into the closure
    let app = Router::new().route(
        "/ws",
        get({
            //syntax error, let expressions not supported here
            let players = players.clone();
            move |ws| websocket_handler(ws, players)
        }),
    );

    //localhost:3000
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("Listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

//players is greyed out, says to prefix it with _ if intentional
async fn websocket_handler(ws: WebSocketUpgrade, players: Players) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, players))
    //it says future cannot be sent between threads safely within 'impl futures::Future<Output
    //= ()>' the trait 'std::marker::Send' is not implemented for 'std::sync::MutexGuard<'_,
    //axum::extract::ws::WebSocket>
}

//in rust, we have to look to the left and the right of a variable
//          it telles me to remove this mut in the paramet and the username

async fn handle_socket(socket: WebSocket, players: Players) {
    let mut username = None;
    let socket = Arc::new(Mutex::new(socket));

    {
        let mut players_lock = players.lock().await;
        players_lock.insert("temp".to_string(), socket.clone());
    }

    // Handle incoming messages
    while let Some(result) = {
        let mut socket_lock = socket.lock().await;
        socket_lock.recv().await
    } {
        match result {
            Ok(AxumMessage::Text(text)) => {
                println!("Received message: {}", text);
                if username.is_none() {
                    username = Some(text.clone());
                    {
                        let mut players_lock = players.lock().await;
                        players_lock.remove("temp");
                        players_lock.insert(text.clone(), socket.clone());
                    }
                    println!("{} joined", text);
                    broadcast_to_all(&players, &format!("{} has joined the lobby", text)).await;
                }
            }
            Ok(_) => {
                // Ignore non-text messages for now
            }
            Err(e) => {
                // An error occurred, likely meaning the connection is closed
                println!("WebSocket error: {}", e);
                break;
            }
        }
    }

    // Handle player disconnection
    if let Some(name) = username {
        {
            let mut players_lock = players.lock().await;
            players_lock.remove(&name);
        }
        println!("{} has left the lobby", name);
        broadcast_to_all(&players, &format!("{} has left the lobby", name)).await;
    }
}

async fn broadcast_to_all(players: &Players, message: &str) {
    let mut players_lock = players.lock().await;
    for (_, player_socket) in players_lock.iter_mut() {
        let mut socket_lock = player_socket.lock().await;
        if let Err(e) = socket_lock
            .send(AxumMessage::Text(message.to_string()))
            .await
        {
            println!("Failed to send message to a client: {}", e);
        }
    }
}
