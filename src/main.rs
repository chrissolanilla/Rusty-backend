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

use std::{
	net::SocketAddr,
	collections::HashMap,
	sync::Arc,
};
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
	let app = Router::new().route("/ws", get({
			//syntax error, let expressions not supported here
			let players = players.clone();
			move |ws| websocket_handler(ws, players)
		}));

	//localhost:3000
	let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
	println!("Listening on {}", addr);
	axum::Server::bind(&addr).serve(app.into_make_service()).await.unwrap();
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
	let username = "username1".to_string(); //some placeholder string?
	let socket = Arc::new(Mutex::new(socket));
	//the reason is braces create new scope. The lock is released when scope ends
	{
		//rust forces you to use snake case, await here instead of unwrap(what is unwrap?)
		let mut players_lock = players.lock().await;
		players_lock.insert(username.clone(), socket.clone());
	}
	//notify other clients by broadcasting
	//it says to use '!' to invoke the macro: '!', what is a macro and ! format?
	broadcast_to_all(&players, &format!("{} has joined the lobby", username)).await;

	//handle some incoming messages
    //what is this cryptic message warning(purple underline) "future is not 'Send' as this value is
                                                        //used across an await"
	while let Some(Ok(message)) = socket.lock().await.recv().await {
		if let AxumMessage::Text(text) = message{
			//broadcast the message to all
			//intresting way to do strings
			broadcast_to_all(&players, &format!("{}: {}", username, text)).await;
		}
	}

	{
		//when socket disconnects we remove them.
		let mut players_lock = players.lock().await;
		players_lock.remove(&username);
	}

	//notify all players someone left
	broadcast_to_all(&players, &format!("{} has left the lobby", username)).await;
}

async fn broadcast_to_all(players: &Players, message: &str) {
	//we "borrow" the players?
	//it told me to consider changing this to mut
	let mut players_lock = players.lock().await;
	//we need to use iter_mut so that we can mutable references
	for (_, player_socket) in players_lock.iter_mut() {
        let mut socket_lock = player_socket.lock().await;
		let _ = socket_lock.send(AxumMessage::Text(message.to_string())).await;
	}
}
