use futures_util::sink::SinkExt;
use futures_util::stream::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast::{Sender, channel};
use tokio::sync::RwLock;
use tokio_websockets::{Message, ServerBuilder, WebSocketStream};

// JSON Protocol Structs
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "lowercase")]
enum MsgType {
    Users,
    Register,
    Message,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct WebSocketMessage {
    message_type: MsgType,
    data_array: Option<Vec<String>>,
    data: Option<String>,
}

// Shared state for all connected clients
#[derive(Clone)]
struct AppState {
    users: Arc<RwLock<HashMap<String, SocketAddr>>>,
    bcast_tx: Sender<String>,
}

impl AppState {
    fn new(bcast_tx: Sender<String>) -> Self {
        Self {
            users: Arc::new(RwLock::new(HashMap::new())),
            bcast_tx,
        }
    }
}

async fn handle_connection(
    addr: SocketAddr,
    mut ws_stream: WebSocketStream<TcpStream>,
    state: AppState,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut current_user: Option<String> = None;
    let mut bcast_rx = state.bcast_tx.subscribe();

    loop {
        tokio::select! {
            // Incoming messages from client
            incoming = ws_stream.next() => {
                match incoming {
                    Some(Ok(msg)) => {
                        if let Some(text) = msg.as_text() {
                            // Parse JSON message from client
                            match serde_json::from_str::<WebSocketMessage>(text) {
                                Ok(ws_msg) => {
                                    match ws_msg.message_type {
                                        MsgType::Register => {
                                            if let Some(username) = ws_msg.data {
                                                println!("🔐 User registered: {} from {}", username, addr);
                                                current_user = Some(username.clone());
                                                
                                                // Add user to connected users
                                                state.users.write().await.insert(username.clone(), addr);
                                                
                                                // Get all current users
                                                let users_list: Vec<String> = state.users
                                                    .read()
                                                    .await
                                                    .keys()
                                                    .cloned()
                                                    .collect();
                                                
                                                // Broadcast updated user list to ALL clients
                                                let users_msg = WebSocketMessage {
                                                    message_type: MsgType::Users,
                                                    data_array: Some(users_list),
                                                    data: None,
                                                };
                                                
                                                if let Ok(json) = serde_json::to_string(&users_msg) {
                                                    let _ = state.bcast_tx.send(json);
                                                }
                                            }
                                        }
                                        MsgType::Message => {
                                            if let Some(username) = &current_user {
                                                if let Some(msg_content) = ws_msg.data {
                                                    println!("💬 Message from {}: {}", username, msg_content);
                                                    
                                                    // Create message object with sender info
                                                    let message_obj = serde_json::json!({
                                                        "from": username,
                                                        "message": msg_content
                                                    });
                                                    
                                                    // Send as Message type
                                                    let response = WebSocketMessage {
                                                        message_type: MsgType::Message,
                                                        data_array: None,
                                                        data: Some(message_obj.to_string()),
                                                    };
                                                    
                                                    if let Ok(json) = serde_json::to_string(&response) {
                                                        let _ = state.bcast_tx.send(json);
                                                    }
                                                }
                                            }
                                        }
                                        MsgType::Users => {} // Ignore Users type from client
                                    }
                                }
                                Err(e) => {
                                    eprintln!("❌ Failed to parse JSON: {}", e);
                                }
                            }
                        }
                    }
                    Some(Err(e)) => {
                        eprintln!("❌ WebSocket error: {}", e);
                        return Err(e.into());
                    }
                    None => {
                        // Client disconnected
                        if let Some(username) = current_user.clone() {
                            println!("👋 User disconnected: {} from {}", username, addr);
                            state.users.write().await.remove(&username);
                            
                            // Broadcast updated user list
                            let users_list: Vec<String> = state.users
                                .read()
                                .await
                                .keys()
                                .cloned()
                                .collect();
                            
                            let users_msg = WebSocketMessage {
                                message_type: MsgType::Users,
                                data_array: Some(users_list),
                                data: None,
                            };
                            
                            if let Ok(json) = serde_json::to_string(&users_msg) {
                                let _ = state.bcast_tx.send(json);
                            }
                        }
                        break;
                    }
                }
            }
            // Broadcast messages to this client
            msg = bcast_rx.recv() => {
                match msg {
                    Ok(json_str) => {
                        if let Err(e) = ws_stream.send(Message::text(json_str)).await {
                            eprintln!("❌ Failed to send message: {}", e);
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let (bcast_tx, _) = channel::<String>(100);
    let state = AppState::new(bcast_tx);

    let listener = TcpListener::bind("0.0.0.0:8080").await?;
    println!("🚀 YewChat Rust WebSocket Server started on ws://0.0.0.0:8080");
    println!("📊 Waiting for connections...\n");

    loop {
        let (socket, addr) = listener.accept().await?;
        println!("📱 New connection from {}", addr);
        let state = state.clone();

        tokio::spawn(async move {
            match ServerBuilder::new().accept(socket).await {
                Ok((_req, ws_stream)) => {
                    if let Err(e) = handle_connection(addr, ws_stream, state).await {
                        eprintln!("❌ Connection error for {}: {}", addr, e);
                    }
                }
                Err(e) => {
                    eprintln!("❌ Failed to establish WebSocket connection: {}", e);
                }
            }
        });
    }
}