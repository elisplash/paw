// Paw Agent Engine — Web Chat Bridge
//
// A lightweight HTTP + WebSocket server that lets friends chat with your
// agent from their browser. No account needed — just share a link + token.
//
// Architecture:
//   - Binds a TCP listener on a configurable port (default 3939)
//   - GET /         → serves a self-contained HTML chat page (no secrets embedded)
//   - POST /auth    → validates access token, returns a session cookie
//   - GET /ws       → upgrades to WebSocket (session cookie required)
//   - Optional TLS via rustls for HTTPS/WSS when cert+key paths are set
//
// Security:
//   - Access token required (auto-generated or user-set)
//   - Token is never embedded in HTML — exchanged via POST /auth for a session cookie
//   - Standard allowlist / pairing / open DM policy
//   - Binds to 127.0.0.1 (localhost) by default; set bind_address to "0.0.0.0" for LAN
//   - Optional TLS for HTTPS/WSS (recommended when binding to 0.0.0.0)

mod html;
mod server;
mod session;

use crate::engine::channels::{self, PendingUser, ChannelStatus};
use log::{debug, info, warn, error};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use tauri::Emitter;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use futures::stream::StreamExt;
use futures::SinkExt;
use crate::atoms::error::{EngineResult, EngineError};

// ── Web Chat Config ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebChatConfig {
    pub enabled: bool,
    /// Address to bind — "127.0.0.1" (local only) or "0.0.0.0" (LAN)
    pub bind_address: String,
    pub port: u16,
    /// Access token — required to connect. Auto-generated if empty.
    pub access_token: String,
    /// "open" | "allowlist" | "pairing"
    pub dm_policy: String,
    /// Usernames allowed to chat (when policy is "allowlist" or "pairing")
    pub allowed_users: Vec<String>,
    #[serde(default)]
    pub pending_users: Vec<PendingUser>,
    pub agent_id: Option<String>,
    /// Title shown on the chat page
    pub page_title: String,
    /// Path to TLS certificate PEM file (enables HTTPS/WSS when set with tls_key_path)
    #[serde(default)]
    pub tls_cert_path: Option<String>,
    /// Path to TLS private key PEM file
    #[serde(default)]
    pub tls_key_path: Option<String>,
}

impl Default for WebChatConfig {
    fn default() -> Self {
        // Generate a random 12-char token
        let token: String = uuid::Uuid::new_v4().to_string().replace('-', "")[..12].to_string();
        WebChatConfig {
            enabled: false,
            bind_address: "127.0.0.1".into(),
            port: 3939,
            access_token: token,
            dm_policy: "open".into(),
            allowed_users: vec!["nano banana pro".into()],
            pending_users: vec![],
            agent_id: None,
            page_title: "Paw Chat".into(),
            tls_cert_path: None,
            tls_key_path: None,
        }
    }
}

// ── Global State ───────────────────────────────────────────────────────

static BRIDGE_RUNNING: AtomicBool = AtomicBool::new(false);
static MESSAGE_COUNT: AtomicI64 = AtomicI64::new(0);
static STOP_SIGNAL: std::sync::OnceLock<Arc<AtomicBool>> = std::sync::OnceLock::new();

fn get_stop_signal() -> Arc<AtomicBool> {
    STOP_SIGNAL.get_or_init(|| Arc::new(AtomicBool::new(false))).clone()
}

const CONFIG_KEY: &str = "webchat_config";

// ── Public API ─────────────────────────────────────────────────────────

pub fn load_config(app_handle: &tauri::AppHandle) -> EngineResult<WebChatConfig> {
    channels::load_channel_config(app_handle, CONFIG_KEY)
}

pub fn save_config(app_handle: &tauri::AppHandle, config: &WebChatConfig) -> EngineResult<()> {
    channels::save_channel_config(app_handle, CONFIG_KEY, config)
}

pub fn approve_user(app_handle: &tauri::AppHandle, user_id: &str) -> EngineResult<()> {
    channels::approve_user_generic(app_handle, CONFIG_KEY, user_id)
}

pub fn deny_user(app_handle: &tauri::AppHandle, user_id: &str) -> EngineResult<()> {
    channels::deny_user_generic(app_handle, CONFIG_KEY, user_id)
}

pub fn remove_user(app_handle: &tauri::AppHandle, user_id: &str) -> EngineResult<()> {
    channels::remove_user_generic(app_handle, CONFIG_KEY, user_id)
}

pub fn start_bridge(app_handle: tauri::AppHandle) -> EngineResult<()> {
    if BRIDGE_RUNNING.load(Ordering::Relaxed) {
        return Err("Web Chat is already running".into());
    }

    let config: WebChatConfig = load_config(&app_handle)?;
    if config.access_token.is_empty() {
        return Err("Access token is required for Web Chat.".into());
    }
    if !config.enabled {
        return Err("Web Chat bridge is disabled.".into());
    }

    let stop = get_stop_signal();
    stop.store(false, Ordering::Relaxed);
    BRIDGE_RUNNING.store(true, Ordering::Relaxed);

    info!("[webchat] Starting on {}:{}", config.bind_address, config.port);

    tauri::async_runtime::spawn(async move {
        if let Err(e) = server::run_server(app_handle, config).await {
            error!("[webchat] Server crashed: {}", e);
        }
        BRIDGE_RUNNING.store(false, Ordering::Relaxed);
        info!("[webchat] Server stopped");
    });

    Ok(())
}

pub fn stop_bridge() {
    let stop = get_stop_signal();
    stop.store(true, Ordering::Relaxed);
    BRIDGE_RUNNING.store(false, Ordering::Relaxed);
    info!("[webchat] Stop signal sent");
}

pub fn get_status(app_handle: &tauri::AppHandle) -> ChannelStatus {
    let config: WebChatConfig = load_config(app_handle).unwrap_or_default();
    ChannelStatus {
        running: BRIDGE_RUNNING.load(Ordering::Relaxed),
        connected: BRIDGE_RUNNING.load(Ordering::Relaxed),
        bot_name: Some(config.page_title.clone()),
        bot_id: Some(format!("{}:{}", config.bind_address, config.port)),
        message_count: MESSAGE_COUNT.load(Ordering::Relaxed) as u64,
        allowed_users: config.allowed_users,
        pending_users: config.pending_users,
        dm_policy: config.dm_policy,
    }
}

// ── WebSocket Chat Handler ─────────────────────────────────────────────

async fn handle_websocket<S: AsyncRead + AsyncWrite + Unpin>(
    stream: S,
    peer: std::net::SocketAddr,
    app_handle: tauri::AppHandle,
    config: Arc<WebChatConfig>,
    username: String,
) -> EngineResult<()> {
    let ws_stream = tokio_tungstenite::accept_async(stream).await
        .map_err(|e| EngineError::Channel { channel: "webchat".into(), message: e.to_string() })?;

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // Access control
    let mut current_config: WebChatConfig = load_config(&app_handle).unwrap_or_default();
    let access_result = channels::check_access(
        &current_config.dm_policy,
        &username,
        &username,
        &username,
        &current_config.allowed_users,
        &mut current_config.pending_users,
    );

    if let Err(denial_msg) = access_result {
        let denial_str = denial_msg.to_string();
        // Save updated pending_users
        let _ = save_config(&app_handle, &current_config);
        let _ = app_handle.emit("webchat-status", json!({
            "kind": "pairing_request",
            "username": &username,
            "peer": peer.to_string(),
        }));

        let msg = json!({ "type": "system", "text": denial_str });
        let _ = ws_sender.send(WsMessage::Text(msg.to_string().into())).await;
        return Ok(());
    }

    // Send welcome
    let welcome = json!({
        "type": "system",
        "text": format!("Connected to {}. Send a message to start chatting!", config.page_title)
    });
    let _ = ws_sender.send(WsMessage::Text(welcome.to_string().into())).await;

    let agent_id = config.agent_id.clone().unwrap_or_default();
    let channel_context = format!(
        "User '{}' is chatting via the Paw Web Chat interface from {}. \
         Keep responses concise but helpful. You can use markdown formatting.",
        username, peer
    );

    // Message loop
    while let Some(msg) = ws_receiver.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(e) => {
                warn!("[webchat] WebSocket error from {}: {}", peer, e);
                break;
            }
        };

        match msg {
            WsMessage::Text(text) => {
                let text = text.to_string();
                // Parse incoming JSON: { "type": "message", "text": "hello" }
                let incoming: serde_json::Value = serde_json::from_str(&text).unwrap_or(json!({"text": text}));
                let user_text = incoming["text"].as_str().unwrap_or("").trim().to_string();

                if user_text.is_empty() { continue; }

                MESSAGE_COUNT.fetch_add(1, Ordering::Relaxed);
                debug!("[webchat] {} says: {}", username, &user_text[..user_text.len().min(80)]);

                // Send typing indicator
                let typing = json!({ "type": "typing" });
                let _ = ws_sender.send(WsMessage::Text(typing.to_string().into())).await;

                // Route through agent
                let reply = channels::run_channel_agent(
                    &app_handle,
                    "webchat",
                    &channel_context,
                    &user_text,
                    &username,
                    &agent_id,
                ).await;

                let response = match reply {
                    Ok(text) => json!({ "type": "message", "text": text }),
                    Err(e) => json!({ "type": "error", "text": format!("Error: {}", e) }),
                };

                if ws_sender.send(WsMessage::Text(response.to_string().into())).await.is_err() {
                    break;
                }
            }
            WsMessage::Close(_) => {
                info!("[webchat] {} disconnected", username);
                break;
            }
            WsMessage::Ping(data) => {
                let _ = ws_sender.send(WsMessage::Pong(data)).await;
            }
            _ => {}
        }
    }

    Ok(())
}
