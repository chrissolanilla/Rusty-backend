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

    // Wait for the first message to get the username
    if let Some(Ok(AxumMessage::Text(text))) = client_ws_rcv.next().await {
        // Parse the first message to get the username
        if let Ok(json_message) = serde_json::from_str::<IncomingMessage>(&text) {
            if let Some(username) = json_message.username.clone() {
                let player_info = PlayerInfo {
                    name: username.clone(),  // Use the provided username
                    score: 0,
                };

                let new_player = Player {
                    info: player_info.clone(),
                    socket: socket.clone(),
                };

                // Add player to the list
                players.write().await.insert(uuid.clone(), new_player);

                // Broadcast player join event
                broadcast_players(&players).await.unwrap();

                // Listen for more incoming messages
                while let Some(Ok(msg)) = client_ws_rcv.next().await {
                    handle_incoming_message(uuid.clone(), msg, &players).await;
                }

                // Handle player disconnect
                players.write().await.remove(&uuid);
                println!("{} disconnected", uuid);
                broadcast_players(&players).await.unwrap();
            } else {
                println!("Failed to extract username.");
            }
        } else {
            println!("Failed to parse incoming message.");
        }
    }
}


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
                        // Update the player's info with the provided username
                        if let Some(player) = players.write().await.get_mut(&client_id) {
                            player.info.name = username.clone();
                            println!("Player {} has joined with username: {}", client_id, username);

                            // Broadcast the updated player list to everyone
                            broadcast_players(players).await.unwrap();
                        }
                    }
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
    let player_list: Vec<PlayerInfo> = {
        let players_lock = players.read().await;
        players_lock.values().map(|p| p.info.clone()).collect()
    };

    let message = json!({ "players": player_list }).to_string();
    if let Some(player) = players.read().await.get(client_id) {
        let mut socket_lock = player.socket.lock().await;
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
        let mut socket_lock = socket.lock().await;
        if let Err(e) = socket_lock.send(Ok(AxumMessage::Text(message.clone()))) {
            println!("Failed to broadcast message: {}", e);
        }
    }
    Ok(())
}

