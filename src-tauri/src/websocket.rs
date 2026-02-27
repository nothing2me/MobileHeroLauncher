use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tauri::{AppHandle, Emitter, Manager};

use crate::config::{self, Config};
use crate::keyboard;

// ... imports ...

pub struct ServerState {
    pub is_running: bool,
    pub should_stop: bool,
    pub connected_clients: u32,
    pub shutdown_tx: tokio::sync::broadcast::Sender<()>,
}

impl ServerState {
    pub fn new() -> Self {
        let (tx, _rx) = tokio::sync::broadcast::channel(1);
        Self {
            is_running: false,
            should_stop: false,
            connected_clients: 0,
            shutdown_tx: tx,
        }
    }
}

#[derive(Debug, Deserialize)]
struct ClientMessage {
    #[serde(rename = "type")]
    msg_type: String,
    value: Option<String>,
    key: Option<String>,
    direction: Option<String>,
    pressed: Option<bool>,
    pin: Option<String>,
    t: Option<i64>,
}

#[derive(Debug, Serialize)]
struct ServerResponse {
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    t: Option<i64>,
}

pub async fn run_server(
// ...
    state: Arc<Mutex<ServerState>>,
    app: AppHandle,
    port: u16,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&addr).await?;
    
    let msg = format!("WebSocket server listening on {}", addr);
    println!("{}", msg);
    let _ = app.emit("log", msg);
    
    // Emit server started event
    let _ = app.emit("server-status", "running");
    
    loop {
        // Check if should stop
        {
            let server = state.lock().await;
            if server.should_stop {
                break;
            }
        }
        
        // Accept connections with timeout
        match tokio::time::timeout(
            tokio::time::Duration::from_millis(100),
            listener.accept()
        ).await {
            Ok(Ok((stream, addr))) => {
                let state_clone = Arc::clone(&state);
                let app_clone = app.clone();
                
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, addr, state_clone, app_clone).await {
                        eprintln!("Connection error: {}", e);
                    }
                });
            }
            Ok(Err(e)) => {
                eprintln!("Accept error: {}", e);
            }
            Err(_) => {
                // Timeout, continue loop to check should_stop
            }
        }
    }
    
    let _ = app.emit("server-status", "stopped");
    let _ = app.emit("log", "Server stopped");
    Ok(())
}

async fn handle_connection(
    stream: TcpStream,
    addr: SocketAddr,
    state: Arc<Mutex<ServerState>>,
    app: AppHandle,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Disable Nagle's algorithm for low latency packet sending
    if let Err(e) = stream.set_nodelay(true) {
        eprintln!("Failed to set TCP_NODELAY: {}", e);
    }
    
    let ws_stream = accept_async(stream).await?;
    let (mut write, mut read) = ws_stream.split();
    
    let msg = format!("[+] New connection from: {}", addr);
    println!("{}", msg);
    let _ = app.emit("log", msg);
    
    // Resolve config path
    let config_path = match app.path().app_config_dir() {
        Ok(dir) => dir.join("config.json"),
        Err(_) => std::path::PathBuf::from("config.json"),
    };
    
    let config = config::load_config(Some(config_path));
    let mut authenticated = false;
    let mut pressed_keys = std::collections::HashSet::new();
    
    // Update client count
    {
        let mut server = state.lock().await;
        server.connected_clients += 1;
        let _ = app.emit("client-count", server.connected_clients);
    }
    
    // Authentication timeout
    let auth_timeout = tokio::time::timeout(
        tokio::time::Duration::from_secs(10),
        read.next()
    ).await;
    
    match auth_timeout {
        Ok(Some(Ok(msg))) => {
            if let Ok(text) = msg.to_text() {
                if let Ok(data) = serde_json::from_str::<ClientMessage>(text) {
                    if data.msg_type == "auth" {
                        if data.pin.as_deref() == Some(&config.pin) {
                            authenticated = true;
                            let response = ServerResponse {
                                msg_type: "auth_success".to_string(),
                                message: None,
                                t: None,
                            };
                            let _ = write.send(Message::Text(serde_json::to_string(&response)?)).await;
                            
                            let log_msg = format!("[OK] Authenticated: {}", addr);
                            println!("{}", log_msg);
                            let _ = app.emit("log", log_msg);
                            let _ = app.emit("client-authenticated", addr.to_string());
                        } else {
                            let response = ServerResponse {
                                msg_type: "auth_failed".to_string(),
                                message: Some("Invalid PIN".to_string()),
                                t: None,
                            };
                            let _ = write.send(Message::Text(serde_json::to_string(&response)?)).await;
                            
                            let log_msg = format!("[X] Auth failed: {}", addr);
                            println!("{}", log_msg);
                            let _ = app.emit("log", log_msg);
                        }
                    }
                }
            }
        }
        _ => {
            let log_msg = format!("[TIMEOUT] Auth timeout: {}", addr);
            println!("{}", log_msg);
            let _ = app.emit("log", log_msg);
        }
    }
    
    if !authenticated {
        // Cleanup
        let mut server = state.lock().await;
        server.connected_clients = server.connected_clients.saturating_sub(1);
        let _ = app.emit("client-count", server.connected_clients);
        return Ok(());
    }
    
    // Get shutdown receiver
    let mut shutdown_rx = {
        let server = state.lock().await;
        server.shutdown_tx.subscribe()
    };
    
    // Main message loop
    loop {
        tokio::select! {
            msg_result = read.next() => {
                match msg_result {
                    Some(Ok(msg)) => {
                        if msg.is_close() {
                            break;
                        }
                        
                        if let Ok(text) = msg.to_text() {
                            if let Ok(data) = serde_json::from_str::<ClientMessage>(text) {
                                handle_input(&data, &config, &mut write, &app, &mut pressed_keys).await;
                            }
                        }
                    }
                    Some(Err(e)) => {
                        eprintln!("WebSocket error: {}", e);
                        break;
                    }
                    None => {
                        break;
                    }
                }
            }
            _ = shutdown_rx.recv() => {
                println!("Closing connection due to server stop: {}", addr);
                break;
            }
        }
    }
    
    // Release all keys on disconnect
    keyboard::release_all(&config);
    
    // Update client count
    {
        let mut server = state.lock().await;
        server.connected_clients = server.connected_clients.saturating_sub(1);
        let _ = app.emit("client-count", server.connected_clients);
    }
    
    let log_msg = format!("[DISCONNECTED] {}", addr);
    println!("{}", log_msg);
    let _ = app.emit("log", log_msg);
    let _ = app.emit("client-disconnected", addr.to_string());
    
    Ok(())
}

async fn handle_input<S>(
    data: &ClientMessage,
    config: &Config,
    write: &mut S,
    _app: &AppHandle,
    pressed_keys: &mut std::collections::HashSet<String>,
) where
    S: SinkExt<Message> + Unpin,
    S::Error: std::fmt::Debug,
{
    let pressed = data.pressed.unwrap_or(false);
    let value = data.value.as_deref()
        .or(data.key.as_deref())
        .or(data.direction.as_deref());
    
    match data.msg_type.as_str() {
        "ping" => {
            let response = ServerResponse {
                msg_type: "pong".to_string(),
                message: None,
                t: data.t,
            };
            let _ = write.send(Message::Text(serde_json::to_string(&response).unwrap())).await;
        }
        
        "fret" => {
            if let Some(fret) = value {
                let key_id = format!("fret_{}", fret);
                if ["green", "red", "yellow", "blue", "orange"].contains(&fret) {
                    if pressed {
                        if !pressed_keys.contains(&key_id) {
                            keyboard::press_key(fret, config);
                            // let _ = app.emit("log", format!("[FRET] {} pressed", fret));
                            pressed_keys.insert(key_id);
                        }
                    } else {
                        if pressed_keys.contains(&key_id) {
                            keyboard::release_key(fret, config);
                            // let _ = app.emit("log", format!("[FRET] {} released", fret));
                            pressed_keys.remove(&key_id);
                        }
                    }
                }
            }
        }
        
        "strum" => {
            if let Some(direction) = value {
                let key_id = format!("strum_{}", direction);
                let key = format!("strum_{}", direction);
                if pressed {
                    if !pressed_keys.contains(&key_id) {
                        keyboard::press_key(&key, config);
                        // let _ = app.emit("log", format!("[STRUM] {}", direction));
                        pressed_keys.insert(key_id);
                    }
                } else {
                    if pressed_keys.contains(&key_id) {
                        keyboard::release_key(&key, config);
                        pressed_keys.remove(&key_id);
                    }
                }
            }
        }
        
        "drum" => {
            if let Some(pad) = value {
                let key_id = format!("drum_{}", pad);
                let key = if pad == "kick" {
                    "drum_kick".to_string()
                } else {
                    format!("drum_{}", pad)
                };
                
                if pressed {
                    if !pressed_keys.contains(&key_id) {
                        keyboard::press_key(&key, config);
                        // let _ = app.emit("log", format!("[DRUM] {} hit", pad));
                        pressed_keys.insert(key_id);
                    }
                } else {
                    if pressed_keys.contains(&key_id) {
                        keyboard::release_key(&key, config);
                        pressed_keys.remove(&key_id);
                    }
                }
            }
        }
        
        "starpower" | "whammy" | "start" | "select" => {
            let key_id = data.msg_type.clone();
            if pressed {
                if !pressed_keys.contains(&key_id) {
                    keyboard::press_key(&data.msg_type, config);
                    // let _ = app.emit("log", format!("[ACTION] {} pressed", data.msg_type));
                    pressed_keys.insert(key_id);
                }
            } else {
                if pressed_keys.contains(&key_id) {
                    keyboard::release_key(&data.msg_type, config);
                    pressed_keys.remove(&key_id);
                }
            }
        }
        
        "left" | "right" | "up" | "down" => {
            let key_id = data.msg_type.clone();
            if pressed {
                if !pressed_keys.contains(&key_id) {
                    keyboard::press_key(&data.msg_type, config);
                    // let _ = app.emit("log", format!("[NAV] {} pressed", data.msg_type));
                    pressed_keys.insert(key_id);
                }
            } else {
                if pressed_keys.contains(&key_id) {
                    keyboard::release_key(&data.msg_type, config);
                    pressed_keys.remove(&key_id);
                }
            }
        }
        
        _ => {}
    }
}
