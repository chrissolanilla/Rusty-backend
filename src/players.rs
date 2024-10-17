use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::Mutex;
use std::sync::Arc;
use serde::{Serialize, Deserialize};
use axum::extract::ws::Message as AxumMessage;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PlayerInfo {
    pub name: String,
    pub score: i32,
}

pub struct Player {
    pub info: PlayerInfo,
    pub socket: Arc<Mutex<UnboundedSender<Result<AxumMessage, axum::Error>>>>,  // Use UnboundedSender for async communication
}

