// Paw Agent Engine — Slack Bridge (Socket Mode)
//
// Connects Paw to Slack via Socket Mode — outbound WebSocket, no public URL.
// The bot opens a WebSocket to Slack's servers and receives events push-style.
// Replies are sent via the Slack Web API (chat.postMessage).
//
// Setup: api.slack.com → Create App → Enable Socket Mode → Bot Token + App Token.
//
// Security:
//   - Allowlist by Slack user ID
//   - Optional pairing mode
//   - All communication through Slack's TLS API

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

// ── Slack Config ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackConfig {
    /// Bot User OAuth Token (xoxb-...)
    pub bot_token: String,
    /// App-Level Token (xapp-...) — needed for Socket Mode
    pub app_token: String,
    pub enabled: bool,
    /// "open" | "allowlist" | "pairing"
    pub dm_policy: String,
    pub allowed_users: Vec<String>,
    #[serde(default)]
    pub pending_users: Vec<PendingUser>,
    pub agent_id: Option<String>,
    /// Whether to respond when @mentioned in channels
    #[serde(default = "default_true")]
    pub respond_to_mentions: bool,
}

fn default_true() -> bool { true }

impl Default for SlackConfig {
    fn default() -> Self {
        SlackConfig {
            bot_token: String::new(),
            app_token: String::new(),
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
static STOP_SIGNAL: std::sync::OnceLock<Arc<AtomicBool>> = std::sync::OnceLock::new();

fn get_stop_signal() -> Arc<AtomicBool> {
    STOP_SIGNAL.get_or_init(|| Arc::new(AtomicBool::new(false))).clone()
}

const CONFIG_KEY: &str = "slack_config";

// ── Bridge Core ────────────────────────────────────────────────────────

pub fn start_bridge(app_handle: tauri::AppHandle) -> EngineResult<()> {
    if BRIDGE_RUNNING.load(Ordering::Relaxed) {
        return Err("Slack bridge is already running".into());
    }

    let config: SlackConfig = channels::load_channel_config(&app_handle, CONFIG_KEY)?;
    if config.bot_token.is_empty() || config.app_token.is_empty() {
        return Err("Bot token and App token are both required for Socket Mode.".into());
    }
    if !config.enabled {
        return Err("Slack bridge is disabled.".into());
    }

    let stop = get_stop_signal();
    stop.store(false, Ordering::Relaxed);
    BRIDGE_RUNNING.store(true, Ordering::Relaxed);

    info!("[slack] Starting Socket Mode bridge");

    tauri::async_runtime::spawn(async move {
        let mut reconnect_attempt: u32 = 0;
        loop {
            match run_socket_mode(app_handle.clone(), config.clone()).await {
                Ok(()) => break, // Clean shutdown
                Err(e) => {
                    if get_stop_signal().load(Ordering::Relaxed) { break; }
                    error!("[slack] Bridge error: {} — reconnecting", e);
                    let delay = crate::engine::http::reconnect_delay(reconnect_attempt).await;
                    warn!("[slack] Reconnecting in {}ms (attempt {})", delay.as_millis(), reconnect_attempt + 1);
                    reconnect_attempt += 1;
                    if get_stop_signal().load(Ordering::Relaxed) { break; }
                }
            }
            reconnect_attempt = 0;
        }
        BRIDGE_RUNNING.store(false, Ordering::Relaxed);
        info!("[slack] Bridge stopped");
    });

    Ok(())
}

pub fn stop_bridge() {
    let stop = get_stop_signal();
    stop.store(true, Ordering::Relaxed);
    BRIDGE_RUNNING.store(false, Ordering::Relaxed);
    info!("[slack] Stop signal sent");
}

pub fn get_status(app_handle: &tauri::AppHandle) -> ChannelStatus {
    let config: SlackConfig = channels::load_channel_config(app_handle, CONFIG_KEY).unwrap_or_default();
    ChannelStatus {
        running: BRIDGE_RUNNING.load(Ordering::Relaxed),
        connected: BRIDGE_RUNNING.load(Ordering::Relaxed),
        bot_name: Some("Slack Bot".into()),
        bot_id: BOT_USER_ID.get().cloned(),
        message_count: MESSAGE_COUNT.load(Ordering::Relaxed) as u64,
        allowed_users: config.allowed_users,
        pending_users: config.pending_users,
        dm_policy: config.dm_policy,
    }
}

// ── Slack Socket Mode ──────────────────────────────────────────────────

async fn run_socket_mode(app_handle: tauri::AppHandle, config: SlackConfig) -> EngineResult<()> {
    let stop = get_stop_signal();
    let http_client = reqwest::Client::new();

    // Get bot user ID via auth.test
    let auth_resp = http_client.post("https://slack.com/api/auth.test")
        .header("Authorization", format!("Bearer {}", config.bot_token))
        .send().await?;
    let auth_json: serde_json::Value = auth_resp.json().await?;
    if !auth_json["ok"].as_bool().unwrap_or(false) {
        return Err(format!("auth.test error: {}", auth_json["error"].as_str().unwrap_or("unknown")).into());
    }
    let bot_user_id = auth_json["user_id"].as_str().unwrap_or("").to_string();
    let _ = BOT_USER_ID.set(bot_user_id.clone());
    info!("[slack] Authenticated as user_id={}", bot_user_id);

    // Open a Socket Mode connection
    let ws_url = get_socket_mode_url(&http_client, &config.app_token).await?;

    let (ws_stream, _) = connect_async(&ws_url)
        .await
        .map_err(|e| EngineError::Channel { channel: "slack".into(), message: e.to_string() })?;

    let (mut write, mut read) = ws_stream.split();

    let _ = app_handle.emit("slack-status", json!({
        "kind": "connected",
        "bot_id": &bot_user_id,
    }));

    info!("[slack] Socket Mode connected");

    let mut current_config = config.clone();
    let mut last_config_reload = std::time::Instant::now();

    while let Some(msg_result) = read.next().await {
        if stop.load(Ordering::Relaxed) { break; }

        let msg = match msg_result {
            Ok(m) => m,
            Err(e) => {
                warn!("[slack] WS read error: {}", e);
                break;
            }
        };

        let text = match msg {
            WsMessage::Text(t) => t,
            WsMessage::Close(_) => { info!("[slack] WS closed"); break; }
            WsMessage::Ping(data) => {
                let _ = write.send(WsMessage::Pong(data)).await;
                continue;
            }
            _ => continue,
        };

        let envelope: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let envelope_id = envelope["envelope_id"].as_str().unwrap_or("");

        // Acknowledge every envelope immediately (Slack requires this within 3 seconds)
        if !envelope_id.is_empty() {
            let ack = json!({ "envelope_id": envelope_id });
            let _ = write.send(WsMessage::Text(ack.to_string())).await;
        }

        let event_type = envelope["type"].as_str().unwrap_or("");

        if event_type == "events_api" {
            let payload = &envelope["payload"];
            let event = &payload["event"];
            let inner_type = event["type"].as_str().unwrap_or("");

            if inner_type == "message" || inner_type == "app_mention" {
                // Skip bot messages, subtypes (edits, joins, etc.)
                if event["bot_id"].is_string() { continue; }
                if event["subtype"].is_string() { continue; }

                let user_id = event["user"].as_str().unwrap_or("").to_string();
                let text_content = event["text"].as_str().unwrap_or("").to_string();
                let channel_id = event["channel"].as_str().unwrap_or("").to_string();
                let channel_type = event["channel_type"].as_str().unwrap_or("");

                if text_content.is_empty() || user_id.is_empty() { continue; }
                if user_id == bot_user_id { continue; } // Skip own messages

                let is_dm = channel_type == "im";
                let is_mention = inner_type == "app_mention";

                // In channels, only respond to mentions
                if !is_dm && !is_mention { continue; }
                if !is_dm && !current_config.respond_to_mentions { continue; }

                // Strip bot mention from text
                let content = {
                    let mention_pat = format!("<@{}>", bot_user_id);
                    text_content.replace(&mention_pat, "").trim().to_string()
                };

                if content.is_empty() { continue; }

                debug!("[slack] Message from {} in {}: {}", user_id, channel_id,
                    if content.len() > 50 { format!("{}...", &content[..50]) } else { content.clone() });

                // Access control (DMs)
                if is_dm {
                    match channels::check_access(
                        &current_config.dm_policy,
                        &user_id,
                        &user_id,
                        &user_id,
                        &current_config.allowed_users,
                        &mut current_config.pending_users,
                    ) {
                        Err(denial_msg) => {
                            let denial_str = denial_msg.to_string();
                            let _ = channels::save_channel_config(&app_handle, CONFIG_KEY, &current_config);
                            let _ = app_handle.emit("slack-status", json!({
                                "kind": "pairing_request",
                                "user_id": &user_id,
                            }));
                            let _ = slack_send_message(&http_client, &config.bot_token, &channel_id, &denial_str).await;
                            continue;
                        }
                        Ok(()) => {}
                    }
                }

                MESSAGE_COUNT.fetch_add(1, Ordering::Relaxed);

                // Route to agent
                let agent_id = current_config.agent_id.as_deref().unwrap_or("default");
                let ctx = "You are chatting via Slack. Use Slack-flavored markdown: \
                           *bold*, _italic_, `code`, ```code blocks```, ~strikethrough~. \
                           Keep responses concise and workplace-appropriate.";

                let response = channels::run_channel_agent(
                    &app_handle, "slack", ctx, &content, &user_id, agent_id,
                ).await;

                match response {
                    Ok(reply) if !reply.is_empty() => {
                        // Slack has a 40000 char limit per message (effectively unlimited)
                        let _ = slack_send_message(&http_client, &config.bot_token, &channel_id, &reply).await;
                    }
                    Err(e) => {
                        error!("[slack] Agent error for {}: {}", user_id, e);
                        let _ = slack_send_message(&http_client, &config.bot_token, &channel_id,
                            &format!(":warning: Error: {}", e)).await;
                    }
                    _ => {}
                }
            }
        } else if event_type == "disconnect" {
            info!("[slack] Disconnect event received, reason: {}", envelope["reason"].as_str().unwrap_or("?"));
            break;
        }

        // Reload config
        if last_config_reload.elapsed() > std::time::Duration::from_secs(30) {
            if let Ok(fresh) = channels::load_channel_config::<SlackConfig>(&app_handle, CONFIG_KEY) {
                current_config = fresh;
            }
            last_config_reload = std::time::Instant::now();
        }
    }

    let _ = app_handle.emit("slack-status", json!({
        "kind": "disconnected",
    }));

    Ok(())
}

// ── Slack API Helpers ──────────────────────────────────────────────────

async fn get_socket_mode_url(client: &reqwest::Client, app_token: &str) -> EngineResult<String> {
    let resp = client.post("https://slack.com/api/apps.connections.open")
        .header("Authorization", format!("Bearer {}", app_token))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .send().await?;

    let body: serde_json::Value = resp.json().await?;

    if !body["ok"].as_bool().unwrap_or(false) {
        return Err(format!("connections.open error: {}", body["error"].as_str().unwrap_or("unknown")).into());
    }

    body["url"].as_str()
        .map(|s| s.to_string())
        .ok_or("No URL returned from connections.open".into())
}

async fn slack_send_message(
    client: &reqwest::Client,
    bot_token: &str,
    channel: &str,
    text: &str,
) -> EngineResult<()> {
    let resp = client.post("https://slack.com/api/chat.postMessage")
        .header("Authorization", format!("Bearer {}", bot_token))
        .json(&json!({
            "channel": channel,
            "text": text,
        }))
        .send().await;

    match resp {
        Ok(r) => {
            let body: serde_json::Value = r.json().await.unwrap_or_default();
            if !body["ok"].as_bool().unwrap_or(false) {
                warn!("[slack] chat.postMessage error: {}", body["error"].as_str().unwrap_or("unknown"));
            }
        }
        Err(e) => warn!("[slack] chat.postMessage failed: {}", e),
    }
    Ok(())
}

// ── Config Persistence ─────────────────────────────────────────────────

pub fn load_config(app_handle: &tauri::AppHandle) -> EngineResult<SlackConfig> {
    channels::load_channel_config(app_handle, CONFIG_KEY)
}

pub fn save_config(app_handle: &tauri::AppHandle, config: &SlackConfig) -> EngineResult<()> {
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
