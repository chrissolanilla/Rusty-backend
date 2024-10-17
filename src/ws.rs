use axum::extract::ws::{WebSocket, WebSocketUpgrade, Message as AxumMessage};
use axum::response::IntoResponse;
use futures::{StreamExt, FutureExt};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use uuid::Uuid;
use std::{sync::Arc, collections::HashMap};
use tokio::sync::{RwLock, Mutex};
use crate::players::{Player, PlayerInfo};

use std::sync::atomic::{AtomicBool, Ordering};  // Add this import for AtomicBool
// Global state to keep track of whether the game has started
static GAME_STARTED: AtomicBool = AtomicBool::new(false);

pub type Players = Arc<RwLock<HashMap<String, Player>>>;

#[derive(Debug, Deserialize)]
struct IncomingMessage {
    action_type: String,
    username: Option<String>, // New field for the username
}

pub async fn websocket_handler(ws: WebSocketUpgrade, players: Players) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_client_connection(socket, players))
}

async fn handle_client_connection(ws: WebSocket, players: Players) {
    println!("Establishing client connection...");

    let (client_ws_sender, mut client_ws_rcv) = ws.split();
    let (client_sender, client_rcv) = mpsc::unbounded_channel();
    let client_rcv = UnboundedReceiverStream::new(client_rcv);

    // Forward messages received on client_rcv to WebSocket sender
    tokio::task::spawn(client_rcv.forward(client_ws_sender).map(|result| {
        if let Err(e) = result {
            println!("Error sending websocket message: {}", e);
        }
    }));

    let uuid = Uuid::new_v4().simple().to_string();
    let socket = Arc::new(Mutex::new(client_sender));

    // Add the player to the list with a placeholder name
    let player_info = PlayerInfo {
        name: "Anonymous".to_string(),  // Placeholder name
        score: 0,
    };

    let new_player = Player {
        info: player_info.clone(),
        socket: socket.clone(),
    };

    players.write().await.insert(uuid.clone(), new_player);

    // Immediately send the current player list to the new client
    send_player_list(&uuid, &players).await;

    // Broadcast player join event
    broadcast_players(&players).await.unwrap();

    // Process WebSocket messages asynchronously
    let players_clone = players.clone();
    tokio::spawn(async move {
        while let Some(Ok(msg)) = client_ws_rcv.next().await {
            handle_incoming_message(uuid.clone(), msg, &players_clone).await;
        }

        // Handle player disconnect
        players_clone.write().await.remove(&uuid);
        println!("{} disconnected", uuid);
        broadcast_players(&players_clone).await.unwrap();
    });
}

//my attempt at making the handle start game
// async fn handle_start_game() {
//
// }

async fn handle_incoming_message(client_id: String, msg: AxumMessage, players: &Players) {
    println!("Received message from {}: {:?}", client_id, msg);

    if let AxumMessage::Text(text) = msg {
        if let Ok(json_message) = serde_json::from_str::<IncomingMessage>(&text) {
            match json_message.action_type.as_str() {
                "get_players" => {
                    send_player_list(&client_id, players).await;
                }
                "join_game" => {
                    // Handle the player joining with their username
                    if let Some(username) = json_message.username.clone() {
                        // First, acquire the write lock and update the player's name
                        let mut player_info = None;
                        {
                            // Get a scoped write lock to modify the player's info
                            if let Some(player) = players.write().await.get_mut(&client_id) {
                                player.info.name = username.clone();
                                println!("Player {} has joined with username: {}", client_id, username);
                                player_info = Some(player.info.clone()); // Clone info to use later
                            }
                        }
                        // After releasing the lock, broadcast the updated player list
                        if let Some(info) = player_info {
                            println!("Broadcasting the updated player list with: {:?}", info);
                            broadcast_players(players).await.unwrap();
                        }
                    }
                }
                "start_game" => {
                    GAME_STARTED.store(true,Ordering::Relaxed);
                    broadcast_game_started(players).await.unwrap();
                }
                _ => {
                    println!("Unknown action type: {}", json_message.action_type);
                }
            }
        } else {
            println!("Failed to parse incoming message.");
        }
    }
}


async fn send_player_list(client_id: &str, players: &Players) {
    let player_list: Vec<PlayerInfo>;
    let player_socket: Option<Arc<Mutex<mpsc::UnboundedSender<Result<AxumMessage, axum::Error>>>>>;

    // Acquire the read lock once and extract the necessary information
    {
        let players_lock = players.read().await;
        player_list = players_lock.values().map(|p| p.info.clone()).collect();
        player_socket = players_lock.get(client_id).map(|player| player.socket.clone());
    }

    // Now that the lock is released, send the message
    if let Some(socket) = player_socket {
        let message = json!({ "players": player_list }).to_string();
        let socket_lock = socket.lock().await;
        if let Err(e) = socket_lock.send(Ok(AxumMessage::Text(message))) {
            println!("Failed to send player list to {}: {}", client_id, e);
        }
    }
}


async fn broadcast_players(players: &Players) -> Result<(), String> {
    let player_list: Vec<PlayerInfo> = {
        let players_lock = players.read().await;
        players_lock.values().map(|p| p.info.clone()).collect()
    };

    let message = json!({ "players": player_list }).to_string();
    println!("Broadcasting updated player list: {:?}", player_list);

    let player_sockets: Vec<Arc<Mutex<mpsc::UnboundedSender<Result<AxumMessage, axum::Error>>>>> = {
        let players_lock = players.read().await;
        players_lock.values().map(|p| p.socket.clone()).collect()
    };

    for socket in player_sockets {
        let socket_lock = socket.lock().await;
        if let Err(e) = socket_lock.send(Ok(AxumMessage::Text(message.clone()))) {
            println!("Failed to broadcast message: {}", e);
        }
    }
    Ok(())
}


async fn broadcast_game_started(players: &Players) -> Result<(), String> {
    let message = json!({ "game_started": true }).to_string();
    println!("Broadcasting game started: {:?}", message);
    let player_sockets: Vec<Arc<Mutex<mpsc::UnboundedSender<Result<AxumMessage, axum::Error>>>>> = {
        let players_lock = players.read().await;
        players_lock.values().map(|p| p.socket.clone()).collect()
    };

    for socket in player_sockets {
        let socket_lock = socket.lock().await;
        if let Err(e) = socket_lock.send(Ok(AxumMessage::Text(message.clone()))) {
            println!("Failed to broadcast game start: {}", e);
        }
    }
    Ok(())
}
