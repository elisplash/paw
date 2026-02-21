// Paw Agent Engine — Mattermost Bridge
//
// Connects Paw to Mattermost via outbound WebSocket + REST API.
// Similar to Slack/Discord — WebSocket for real-time events, REST for replies.
//
// Setup: Mattermost Admin → Integrations → Bot Accounts → Create Bot → Copy token.
//        Or: User Settings → Security → Personal Access Tokens.
//
// Security:
//   - HTTPS enforced — `http://` URLs are auto-coerced to `https://`
//   - Allowlist by Mattermost user ID
//   - Optional pairing mode
//   - All communication goes through the Mattermost server's TLS API

use crate::engine::channels::{self, PendingUser, ChannelStatus};
use log::{debug, info, warn, error};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use tauri::Emitter;
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};
use futures::{SinkExt, StreamExt};
use crate::atoms::error::{EngineResult, EngineError};

// ── Mattermost Config ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MattermostConfig {
    /// Mattermost server URL (e.g. "https://chat.example.com")
    pub server_url: String,
    /// Personal Access Token or Bot Token
    pub token: String,
    pub enabled: bool,
    /// "open" | "allowlist" | "pairing"
    pub dm_policy: String,
    pub allowed_users: Vec<String>,
    #[serde(default)]
    pub pending_users: Vec<PendingUser>,
    pub agent_id: Option<String>,
    /// Whether to respond when @mentioned in channels (not just DMs)
    #[serde(default = "default_true")]
    pub respond_to_mentions: bool,
}

fn default_true() -> bool { true }

impl Default for MattermostConfig {
    fn default() -> Self {
        MattermostConfig {
            server_url: String::new(),
            token: String::new(),
            enabled: false,
            dm_policy: "pairing".into(),
            allowed_users: vec![],
            pending_users: vec![],
            agent_id: None,
            respond_to_mentions: true,
        }
    }
}

// ── Global State ───────────────────────────────────────────────────────

static BRIDGE_RUNNING: AtomicBool = AtomicBool::new(false);
static MESSAGE_COUNT: AtomicI64 = AtomicI64::new(0);
static BOT_USER_ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
static BOT_USERNAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
static STOP_SIGNAL: std::sync::OnceLock<Arc<AtomicBool>> = std::sync::OnceLock::new();

fn get_stop_signal() -> Arc<AtomicBool> {
    STOP_SIGNAL.get_or_init(|| Arc::new(AtomicBool::new(false))).clone()
}

const CONFIG_KEY: &str = "mattermost_config";

/// Normalize the server URL to enforce HTTPS.
/// - Strips trailing slashes
/// - Coerces `http://` → `https://` with a warning
/// - Adds `https://` if no scheme is present
/// - Rejects URLs with non-http(s) schemes
fn normalize_server_url(raw: &str) -> EngineResult<String> {
    let url = raw.trim().trim_end_matches('/');
    if url.is_empty() {
        return Err("Server URL is required.".into());
    }

    if let Some(stripped) = url.strip_prefix("http://") {
        let secure = format!("https://{}", stripped);
        warn!(
            "[mattermost] Coerced server URL from http:// to https:// — \
             credentials must not be sent over plaintext HTTP"
        );
        return Ok(secure);
    }

    if url.starts_with("https://") {
        return Ok(url.to_string());
    }

    // Check for other schemes (ftp://, ws://, etc.)
    if let Some(colon_pos) = url.find("://") {
        let scheme = &url[..colon_pos];
        return Err(format!(
            "Unsupported URL scheme '{}://'. Use https:// for your Mattermost server.",
            scheme
        ).into());
    }

    // No scheme at all — assume https
    warn!("[mattermost] No URL scheme provided, assuming https://{}", url);
    Ok(format!("https://{}", url))
}

// ── Bridge Core ────────────────────────────────────────────────────────

pub fn start_bridge(app_handle: tauri::AppHandle) -> EngineResult<()> {
    if BRIDGE_RUNNING.load(Ordering::Relaxed) {
        return Err("Mattermost bridge is already running".into());
    }

    let mut config: MattermostConfig = channels::load_channel_config(&app_handle, CONFIG_KEY)?;
    if config.server_url.is_empty() || config.token.is_empty() {
        return Err("Server URL and token are required.".into());
    }
    if !config.enabled {
        return Err("Mattermost bridge is disabled.".into());
    }

    // Enforce HTTPS — coerce http:// or bare hostnames to https://
    config.server_url = normalize_server_url(&config.server_url)?;

    let stop = get_stop_signal();
    stop.store(false, Ordering::Relaxed);
    BRIDGE_RUNNING.store(true, Ordering::Relaxed);

    info!("[mattermost] Starting bridge to {}", config.server_url);

    tauri::async_runtime::spawn(async move {
        loop {
            if get_stop_signal().load(Ordering::Relaxed) { break; }
            if let Err(e) = run_ws_loop(&app_handle, &config).await {
                error!("[mattermost] WebSocket error: {}", e);
            }
            if get_stop_signal().load(Ordering::Relaxed) { break; }
            warn!("[mattermost] Reconnecting in 5s...");
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
        BRIDGE_RUNNING.store(false, Ordering::Relaxed);
        info!("[mattermost] Bridge stopped");
    });

    Ok(())
}

pub fn stop_bridge() {
    let stop = get_stop_signal();
    stop.store(true, Ordering::Relaxed);
    BRIDGE_RUNNING.store(false, Ordering::Relaxed);
    info!("[mattermost] Stop signal sent");
}

pub fn get_status(app_handle: &tauri::AppHandle) -> ChannelStatus {
    let config: MattermostConfig = channels::load_channel_config(app_handle, CONFIG_KEY).unwrap_or_default();
    ChannelStatus {
        running: BRIDGE_RUNNING.load(Ordering::Relaxed),
        connected: BRIDGE_RUNNING.load(Ordering::Relaxed),
        bot_name: BOT_USERNAME.get().cloned(),
        bot_id: BOT_USER_ID.get().cloned(),
        message_count: MESSAGE_COUNT.load(Ordering::Relaxed) as u64,
        allowed_users: config.allowed_users,
        pending_users: config.pending_users,
        dm_policy: config.dm_policy,
    }
}

// ── WebSocket Loop ─────────────────────────────────────────────────────

async fn run_ws_loop(app_handle: &tauri::AppHandle, config: &MattermostConfig) -> EngineResult<()> {
    let stop = get_stop_signal();
    let client = reqwest::Client::new();
    let base = config.server_url.trim_end_matches('/');

    // Get bot user info
    let me: serde_json::Value = client.get(format!("{}/api/v4/users/me", base))
        .header("Authorization", format!("Bearer {}", config.token))
        .send().await?
        .json().await?;

    if let Some(err_id) = me.get("id").and_then(|v| v.as_str()) {
        let _ = BOT_USER_ID.set(err_id.to_string());
    } else if me.get("status_code").is_some() {
        return Err(format!("Auth failed: {}", me["message"].as_str().unwrap_or("unknown")).into());
    }

    let bot_id = me["id"].as_str().unwrap_or("").to_string();
    let bot_username = me["username"].as_str().unwrap_or("").to_string();
    let _ = BOT_USER_ID.set(bot_id.clone());
    let _ = BOT_USERNAME.set(bot_username.clone());
    info!("[mattermost] Authenticated as {} ({})", bot_username, bot_id);

    // Build WebSocket URL
    let ws_url = if base.starts_with("https") {
        format!("{}/api/v4/websocket", base.replacen("https", "wss", 1))
    } else {
        format!("{}/api/v4/websocket", base.replacen("http", "ws", 1))
    };

    let (ws_stream, _) = connect_async(&ws_url).await
        .map_err(|e| EngineError::Channel { channel: "mattermost".into(), message: e.to_string() })?;
    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    // Authenticate on WebSocket
    let auth_msg = json!({
        "seq": 1,
        "action": "authentication_challenge",
        "data": { "token": config.token }
    });
    ws_tx.send(WsMessage::Text(auth_msg.to_string())).await
        .map_err(|e| EngineError::Channel { channel: "mattermost".into(), message: e.to_string() })?;

    let _ = app_handle.emit("mattermost-status", json!({
        "kind": "connected",
        "username": &bot_username,
    }));

    let mut current_config = config.clone();
    let mut last_config_reload = std::time::Instant::now();

    // Event loop
    loop {
        if stop.load(Ordering::Relaxed) { break; }

        let msg = tokio::select! {
            msg = ws_rx.next() => msg,
            _ = tokio::time::sleep(std::time::Duration::from_secs(60)) => {
                // Keepalive ping
                let _ = ws_tx.send(WsMessage::Ping(vec![])).await;
                continue;
            }
        };

        let msg = match msg {
            Some(Ok(WsMessage::Text(t))) => t,
            Some(Ok(WsMessage::Ping(d))) => {
                let _ = ws_tx.send(WsMessage::Pong(d)).await;
                continue;
            }
            Some(Ok(WsMessage::Close(_))) => break,
            Some(Err(e)) => {
                warn!("[mattermost] WS error: {}", e);
                break;
            }
            None => break,
            _ => continue,
        };

        let payload: serde_json::Value = match serde_json::from_str(&msg) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let event = payload["event"].as_str().unwrap_or("");

        if event == "posted" {
            let post_json_str = payload["data"]["post"].as_str().unwrap_or("");
            let post: serde_json::Value = match serde_json::from_str(post_json_str) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let sender_id = post["user_id"].as_str().unwrap_or("");
            if sender_id == bot_id { continue; } // Skip own messages

            let channel_id = post["channel_id"].as_str().unwrap_or("").to_string();
            let message = post["message"].as_str().unwrap_or("").to_string();
            if message.is_empty() { continue; }

            // Determine channel type from broadcast
            let channel_type = payload["data"]["channel_type"].as_str().unwrap_or("");
            let is_dm = channel_type == "D" || channel_type == "G"; // D = DM, G = group DM

            // In non-DM channels, check for @mention
            if !is_dm {
                let mention_str = format!("@{}", bot_username);
                let mentioned = message.contains(&mention_str);
                if !current_config.respond_to_mentions || !mentioned { continue; }
            }

            // Strip @mention from content
            let content = message
                .replace(&format!("@{}", bot_username), "")
                .trim()
                .to_string();
            if content.is_empty() { continue; }

            let sender_username = payload["data"]["sender_name"].as_str()
                .unwrap_or(sender_id)
                .trim_start_matches('@')
                .to_string();

            debug!("[mattermost] Message from {} in {}: {}",
                sender_username, channel_id,
                if content.len() > 50 { format!("{}...", &content[..50]) } else { content.clone() });

            // Access control (DMs)
            if is_dm {
                if let Err(denial_msg) = channels::check_access(
                    &current_config.dm_policy,
                    sender_id,
                    &sender_username,
                    &sender_username,
                    &current_config.allowed_users,
                    &mut current_config.pending_users,
                ) {
                    let denial_str = denial_msg.to_string();
                    let _ = channels::save_channel_config(app_handle, CONFIG_KEY, &current_config);
                    let _ = app_handle.emit("mattermost-status", json!({
                        "kind": "pairing_request",
                        "user_id": sender_id,
                        "username": &sender_username,
                    }));
                    let _ = mm_send_message(&client, base, &config.token, &channel_id, &denial_str).await;
                    continue;
                }
            }

            MESSAGE_COUNT.fetch_add(1, Ordering::Relaxed);

            let agent_id = current_config.agent_id.as_deref().unwrap_or("default");
            let ctx = "You are chatting via Mattermost. Use Mattermost markdown for formatting. \
                       Keep responses concise. Mattermost supports most standard markdown.";

            let response = channels::run_channel_agent(
                app_handle, "mattermost", ctx, &content, sender_id, agent_id,
            ).await;

            match response {
                Ok(reply) if !reply.is_empty() => {
                    for chunk in channels::split_message(&reply, 16383) {
                        let _ = mm_send_message(&client, base, &config.token, &channel_id, &chunk).await;
                    }
                }
                Err(e) => {
                    error!("[mattermost] Agent error for {}: {}", sender_id, e);
                    let _ = mm_send_message(&client, base, &config.token, &channel_id,
                        &format!("⚠️ Error: {}", e)).await;
                }
                _ => {}
            }
        }

        // Reload config periodically
        if last_config_reload.elapsed() > std::time::Duration::from_secs(30) {
            if let Ok(fresh) = channels::load_channel_config::<MattermostConfig>(app_handle, CONFIG_KEY) {
                current_config = fresh;
            }
            last_config_reload = std::time::Instant::now();
        }
    }

    let _ = app_handle.emit("mattermost-status", json!({
        "kind": "disconnected",
    }));

    Ok(())
}

// ── Mattermost REST API ────────────────────────────────────────────────

async fn mm_send_message(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    channel_id: &str,
    message: &str,
) -> EngineResult<()> {
    let url = format!("{}/api/v4/posts", base);
    let body = json!({
        "channel_id": channel_id,
        "message": message,
    });

    match client.post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&body)
        .send().await
    {
        Ok(r) if !r.status().is_success() => {
            warn!("[mattermost] sendMessage failed: {}", r.status());
        }
        Err(e) => warn!("[mattermost] sendMessage error: {}", e),
        _ => {}
    }
    Ok(())
}

// ── Config Persistence ─────────────────────────────────────────────────

pub fn load_config(app_handle: &tauri::AppHandle) -> EngineResult<MattermostConfig> {
    channels::load_channel_config(app_handle, CONFIG_KEY)
}

pub fn save_config(app_handle: &tauri::AppHandle, config: &MattermostConfig) -> EngineResult<()> {
    // Normalize URL at save time so the UI reflects the coerced value
    let mut config = config.clone();
    if !config.server_url.is_empty() {
        config.server_url = normalize_server_url(&config.server_url)?;
    }
    channels::save_channel_config(app_handle, CONFIG_KEY, &config)
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
