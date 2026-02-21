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

use crate::engine::channels::{self, PendingUser, ChannelStatus};
use log::{debug, info, warn, error};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::io::BufReader as StdBufReader;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Emitter;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use futures::stream::StreamExt;
use futures::SinkExt;

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

// ── Session Management ─────────────────────────────────────────────────

struct Session {
    username: String,
    created_at: u64,
}

static SESSIONS: std::sync::OnceLock<parking_lot::Mutex<HashMap<String, Session>>> =
    std::sync::OnceLock::new();

fn get_sessions() -> &'static parking_lot::Mutex<HashMap<String, Session>> {
    SESSIONS.get_or_init(|| parking_lot::Mutex::new(HashMap::new()))
}

fn create_session(username: String) -> String {
    let session_id = uuid::Uuid::new_v4().to_string();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    get_sessions().lock().insert(session_id.clone(), Session { username, created_at: now });
    // Prune expired sessions (> 24 h)
    get_sessions().lock().retain(|_, s| now.saturating_sub(s.created_at) < 86_400);
    session_id
}

fn validate_session(session_id: &str) -> Option<String> {
    let sessions = get_sessions().lock();
    let s = sessions.get(session_id)?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    if now.saturating_sub(s.created_at) > 86_400 { return None; }
    Some(s.username.clone())
}

fn extract_cookie<'a>(headers: &'a str, name: &str) -> Option<&'a str> {
    for line in headers.lines() {
        if line.to_lowercase().starts_with("cookie:") {
            let value = &line["cookie:".len()..];
            for cookie in value.split(';') {
                let cookie = cookie.trim();
                if let Some(rest) = cookie.strip_prefix(name) {
                    if let Some(val) = rest.strip_prefix('=') {
                        return Some(val.trim());
                    }
                }
            }
        }
    }
    None
}

// ── Prefixed Stream (replays buffered bytes then delegates) ────────────

struct PrefixedStream<S> {
    prefix: Vec<u8>,
    pos: usize,
    inner: S,
}

impl<S> PrefixedStream<S> {
    fn new(prefix: Vec<u8>, inner: S) -> Self {
        Self { prefix, pos: 0, inner }
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for PrefixedStream<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        if this.pos < this.prefix.len() {
            let remaining = &this.prefix[this.pos..];
            let n = remaining.len().min(buf.remaining());
            buf.put_slice(&remaining[..n]);
            this.pos += n;
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut this.inner).poll_read(cx, buf)
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for PrefixedStream<S> {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write(cx, buf)
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

// ── Stream Abstraction ─────────────────────────────────────────────────

trait ChatStream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> ChatStream for T {}

/// Build a TLS acceptor from PEM cert+key files, or `None` if not configured.
fn build_tls_acceptor(config: &WebChatConfig) -> Result<Option<tokio_rustls::TlsAcceptor>, String> {
    let (Some(cert_path), Some(key_path)) = (&config.tls_cert_path, &config.tls_key_path) else {
        return Ok(None);
    };

    let cert_file = std::fs::File::open(cert_path)
        .map_err(|e| format!("Open TLS cert {cert_path}: {e}"))?;
    let certs: Vec<_> = rustls_pemfile::certs(&mut StdBufReader::new(cert_file))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Parse TLS cert: {e}"))?;

    let key_file = std::fs::File::open(key_path)
        .map_err(|e| format!("Open TLS key {key_path}: {e}"))?;
    let key = rustls_pemfile::private_key(&mut StdBufReader::new(key_file))
        .map_err(|e| format!("Parse TLS key: {e}"))?
        .ok_or_else(|| "No private key found in PEM file".to_string())?;

    let tls_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| format!("TLS config: {e}"))?;

    Ok(Some(tokio_rustls::TlsAcceptor::from(Arc::new(tls_config))))
}

// ── Public API ─────────────────────────────────────────────────────────

pub fn load_config(app_handle: &tauri::AppHandle) -> Result<WebChatConfig, String> {
    channels::load_channel_config(app_handle, CONFIG_KEY)
}

pub fn save_config(app_handle: &tauri::AppHandle, config: &WebChatConfig) -> Result<(), String> {
    channels::save_channel_config(app_handle, CONFIG_KEY, config)
}

pub fn approve_user(app_handle: &tauri::AppHandle, user_id: &str) -> Result<(), String> {
    channels::approve_user_generic(app_handle, CONFIG_KEY, user_id)
}

pub fn deny_user(app_handle: &tauri::AppHandle, user_id: &str) -> Result<(), String> {
    channels::deny_user_generic(app_handle, CONFIG_KEY, user_id)
}

pub fn remove_user(app_handle: &tauri::AppHandle, user_id: &str) -> Result<(), String> {
    channels::remove_user_generic(app_handle, CONFIG_KEY, user_id)
}

pub fn start_bridge(app_handle: tauri::AppHandle) -> Result<(), String> {
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
        if let Err(e) = run_server(app_handle, config).await {
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

// ── Server Core ────────────────────────────────────────────────────────

async fn run_server(app_handle: tauri::AppHandle, config: WebChatConfig) -> Result<(), String> {
    let stop = get_stop_signal();
    let addr = format!("{}:{}", config.bind_address, config.port);

    let listener = TcpListener::bind(&addr).await
        .map_err(|e| format!("Bind {}:{} failed: {}", config.bind_address, config.port, e))?;

    // Build optional TLS acceptor
    let tls_acceptor = build_tls_acceptor(&config)?;

    if config.bind_address != "127.0.0.1" && config.bind_address != "localhost" && tls_acceptor.is_none() {
        warn!("[webchat] Binding to {} without TLS — credentials sent in plaintext over the network", config.bind_address);
    }

    let scheme = if tls_acceptor.is_some() { "https" } else { "http" };
    info!("[webchat] Listening on {}://{}", scheme, addr);

    let _ = app_handle.emit("webchat-status", json!({
        "kind": "connected",
        "address": &addr,
        "title": &config.page_title,
        "tls": tls_acceptor.is_some(),
    }));

    let config = Arc::new(config);
    let tls_acceptor = tls_acceptor.map(Arc::new);

    loop {
        if stop.load(Ordering::Relaxed) { break; }

        // Accept with timeout so we can check stop signal
        let accept = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            listener.accept()
        ).await;

        match accept {
            Ok(Ok((tcp_stream, peer))) => {
                let app = app_handle.clone();
                let cfg = config.clone();
                let stop_clone = stop.clone();
                let tls = tls_acceptor.clone();
                tokio::spawn(async move {
                    // Wrap in TLS if configured, then box for type erasure
                    let stream: Box<dyn ChatStream> = if let Some(acceptor) = tls {
                        match acceptor.accept(tcp_stream).await {
                            Ok(tls_stream) => Box::new(tls_stream),
                            Err(e) => {
                                warn!("[webchat] TLS handshake failed from {}: {}", peer, e);
                                return;
                            }
                        }
                    } else {
                        Box::new(tcp_stream)
                    };

                    if let Err(e) = handle_connection(stream, peer, app, cfg, stop_clone).await {
                        warn!("[webchat] Connection error from {}: {}", peer, e);
                    }
                });
            }
            Ok(Err(e)) => {
                warn!("[webchat] Accept error: {}", e);
            }
            Err(_) => { /* timeout — loop to check stop signal */ }
        }
    }

    Ok(())
}

// ── Connection Handler ─────────────────────────────────────────────────

async fn handle_connection(
    mut stream: Box<dyn ChatStream>,
    peer: std::net::SocketAddr,
    app_handle: tauri::AppHandle,
    config: Arc<WebChatConfig>,
    _stop: Arc<AtomicBool>,
) -> Result<(), String> {
    // Read the HTTP request (consumed — PrefixedStream replays it for WS)
    let mut buf = vec![0u8; 8192];
    let n = stream.read(&mut buf).await.map_err(|e| format!("Read: {e}"))?;
    if n == 0 { return Ok(()); }
    buf.truncate(n);

    let request_str = String::from_utf8_lossy(&buf);
    let first_line = request_str.lines().next().unwrap_or("");
    let is_websocket = request_str.contains("Upgrade: websocket")
        || request_str.contains("upgrade: websocket");

    if is_websocket && first_line.contains("/ws") {
        // Validate session cookie (token is never in the URL)
        let session_id = extract_cookie(&request_str, "paw_session").unwrap_or("");
        let username = match validate_session(session_id) {
            Some(name) => name,
            None => {
                let resp = "HTTP/1.1 403 Forbidden\r\nContent-Length: 16\r\n\r\nSession invalid.";
                let _ = stream.write_all(resp.as_bytes()).await;
                return Ok(());
            }
        };

        info!("[webchat] WebSocket connection from {} ({})", peer, username);

        // Replay the buffered bytes so tungstenite can read the HTTP upgrade
        let prefixed = PrefixedStream::new(buf, stream);
        handle_websocket(prefixed, peer, app_handle, config, username).await
    } else if first_line.starts_with("POST") && first_line.contains("/auth") {
        handle_auth(stream, &buf, &config).await
    } else if first_line.starts_with("GET /") {
        serve_html(stream, &config).await
    } else {
        Ok(())
    }
}

// ── Auth Endpoint ──────────────────────────────────────────────────────

/// POST /auth — validates access token, returns a session cookie.
async fn handle_auth(
    mut stream: Box<dyn ChatStream>,
    request_bytes: &[u8],
    config: &WebChatConfig,
) -> Result<(), String> {
    let request_str = String::from_utf8_lossy(request_bytes);

    // Extract JSON body (after \r\n\r\n)
    let body = request_str.split("\r\n\r\n").nth(1).unwrap_or("");
    let parsed: serde_json::Value = serde_json::from_str(body).unwrap_or(json!({}));

    let token = parsed["token"].as_str().unwrap_or("");
    let name = parsed["name"].as_str().unwrap_or("").trim();

    if token != config.access_token || name.is_empty() {
        let resp = "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nContent-Length: 24\r\nConnection: close\r\n\r\n{\"error\":\"access denied\"}";
        stream.write_all(resp.as_bytes()).await
            .map_err(|e| format!("Write auth 403: {e}"))?;
        return Ok(());
    }

    let session_id = create_session(name.to_string());
    info!("[webchat] Session created for '{}'", name);

    let resp_body = json!({"ok": true}).to_string();
    let cookie = format!(
        "paw_session={}; HttpOnly; SameSite=Strict; Path=/; Max-Age=86400",
        session_id,
    );
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nSet-Cookie: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        cookie, resp_body.len(), resp_body
    );

    stream.write_all(response.as_bytes()).await
        .map_err(|e| format!("Write auth 200: {e}"))?;
    Ok(())
}

// ── HTML Chat Page ─────────────────────────────────────────────────────

async fn serve_html(
    mut stream: Box<dyn ChatStream>,
    config: &WebChatConfig,
) -> Result<(), String> {
    // Request was already consumed by handle_connection's read — just write response
    let html = build_chat_html(&config.page_title);
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        html.len(), html
    );

    stream.write_all(response.as_bytes()).await
        .map_err(|e| format!("Write HTML: {e}"))?;

    Ok(())
}

// ── WebSocket Chat Handler ─────────────────────────────────────────────

async fn handle_websocket<S: AsyncRead + AsyncWrite + Unpin>(
    stream: S,
    peer: std::net::SocketAddr,
    app_handle: tauri::AppHandle,
    config: Arc<WebChatConfig>,
    username: String,
) -> Result<(), String> {
    let ws_stream = tokio_tungstenite::accept_async(stream).await
        .map_err(|e| format!("WebSocket handshake failed: {}", e))?;

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
        // Save updated pending_users
        let _ = save_config(&app_handle, &current_config);
        let _ = app_handle.emit("webchat-status", json!({
            "kind": "pairing_request",
            "username": &username,
            "peer": peer.to_string(),
        }));

        let msg = json!({ "type": "system", "text": denial_msg });
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

// ── Chat HTML Builder ──────────────────────────────────────────────────

fn build_chat_html(title: &str) -> String {
    format!(r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{title}</title>
<style>
*{{margin:0;padding:0;box-sizing:border-box}}
body{{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;background:#1e1e1e;color:#cccccc;height:100vh;display:flex;flex-direction:column}}
.header{{padding:16px 20px;background:#252526;border-bottom:1px solid #3c3c3c;display:flex;align-items:center;gap:12px}}
.header h1{{font-size:16px;font-weight:600;color:#ff00ff}}
.header .dot{{width:8px;height:8px;border-radius:50%;background:#333;transition:background .3s}}
.header .dot.online{{background:#0f0}}
.name-bar{{padding:10px 20px;background:#252526;border-bottom:1px solid #3c3c3c;display:flex;gap:8px;flex-wrap:wrap}}
.name-bar input{{flex:1;min-width:120px;padding:8px 12px;border:1px solid #3c3c3c;border-radius:6px;background:#313131;color:#cccccc;font-size:14px;outline:none}}
.name-bar input:focus{{border-color:#ff00ff}}
.name-bar button{{padding:8px 16px;background:#ff00ff;color:#fff;border:none;border-radius:6px;font-weight:600;cursor:pointer}}
.messages{{flex:1;overflow-y:auto;padding:20px;display:flex;flex-direction:column;gap:10px}}
.msg{{max-width:80%;padding:10px 14px;border-radius:12px;font-size:14px;line-height:1.5;word-wrap:break-word;white-space:pre-wrap}}
.msg.user{{align-self:flex-end;background:#2a2d2e;border:1px solid #ff00ff33}}
.msg.assistant{{align-self:flex-start;background:#252526;border:1px solid #3c3c3c}}
.msg.system{{align-self:center;color:#888;font-size:12px;font-style:italic}}
.msg.error{{align-self:center;color:#f44;font-size:13px}}
.typing{{align-self:flex-start;color:#888;font-size:13px;padding:4px 14px}}
.typing::after{{content:'...';animation:dots 1.2s infinite}}
@keyframes dots{{0%,20%{{content:'.'}}40%{{content:'..'}}60%,100%{{content:'...'}}}}
.input-bar{{padding:16px 20px;background:#252526;border-top:1px solid #3c3c3c;display:flex;gap:8px}}
.input-bar textarea{{flex:1;padding:10px 14px;border:1px solid #3c3c3c;border-radius:8px;background:#313131;color:#cccccc;font-size:14px;font-family:inherit;resize:none;outline:none;max-height:120px}}
.input-bar textarea:focus{{border-color:#ff00ff}}
.input-bar button{{padding:10px 20px;background:#ff00ff;color:#fff;border:none;border-radius:8px;font-weight:600;cursor:pointer;white-space:nowrap}}
.input-bar button:disabled{{opacity:.4;cursor:not-allowed}}
</style>
</head>
<body>
<div class="header">
  <div class="dot" id="dot"></div>
  <h1>{title}</h1>
</div>
<div class="name-bar" id="nameBar">
  <input id="nameInput" placeholder="Your name" autofocus />
  <input id="tokenInput" type="password" placeholder="Access token" />
  <button onclick="connect()">Join</button>
</div>
<div class="messages" id="messages"></div>
<div class="input-bar" id="inputBar" style="display:none">
  <textarea id="chatInput" placeholder="Type a message..." rows="1"></textarea>
  <button id="sendBtn" onclick="send()">Send</button>
</div>
<script>
let ws,name="";
const msgs=document.getElementById("messages");
const inp=document.getElementById("chatInput");
const dot=document.getElementById("dot");

async function connect(){{
  name=document.getElementById("nameInput").value.trim();
  const token=document.getElementById("tokenInput").value.trim();
  if(!name||!token)return;
  try{{
    const res=await fetch("/auth",{{
      method:"POST",
      headers:{{"Content-Type":"application/json"}},
      body:JSON.stringify({{name,token}}),
      credentials:"same-origin"
    }});
    if(!res.ok){{addMsg("error","Invalid token.");return}}
  }}catch(e){{addMsg("error","Auth failed: "+e.message);return}}
  document.getElementById("nameBar").style.display="none";
  document.getElementById("inputBar").style.display="flex";
  const proto=location.protocol==="https:"?"wss:":"ws:";
  ws=new WebSocket(`${{proto}}//${{location.host}}/ws`);
  ws.onopen=()=>{{dot.classList.add("online");inp.focus()}};
  ws.onclose=()=>{{dot.classList.remove("online");addMsg("system","Disconnected.")}};
  ws.onmessage=(e)=>{{
    try{{
      const d=JSON.parse(e.data);
      removeTyping();
      if(d.type==="typing"){{addTyping();return}}
      addMsg(d.type||"assistant",d.text||"");
    }}catch(err){{addMsg("assistant",e.data)}}
  }};
}}

function send(){{
  const t=inp.value.trim();
  if(!t||!ws||ws.readyState!==1)return;
  addMsg("user",t);
  ws.send(JSON.stringify({{type:"message",text:t}}));
  inp.value="";
  inp.style.height="auto";
}}

function addMsg(type,text){{
  const d=document.createElement("div");
  d.className="msg "+type;
  d.textContent=text;
  msgs.appendChild(d);
  msgs.scrollTop=msgs.scrollHeight;
}}

function addTyping(){{
  removeTyping();
  const d=document.createElement("div");
  d.className="typing";
  d.id="typing";
  d.textContent="Thinking";
  msgs.appendChild(d);
  msgs.scrollTop=msgs.scrollHeight;
}}

function removeTyping(){{
  const el=document.getElementById("typing");
  if(el)el.remove();
}}

inp.addEventListener("keydown",(e)=>{{
  if(e.key==="Enter"&&!e.shiftKey){{e.preventDefault();send()}}
}});
inp.addEventListener("input",()=>{{
  inp.style.height="auto";
  inp.style.height=Math.min(inp.scrollHeight,120)+"px";
}});
document.getElementById("tokenInput").addEventListener("keydown",(e)=>{{
  if(e.key==="Enter"){{e.preventDefault();connect()}}
}});
</script>
</body>
</html>"##, title=title)
}
