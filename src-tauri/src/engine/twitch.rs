// Paw Agent Engine — Twitch Bridge
//
// Connects Paw to Twitch chat via IRC-over-WebSocket (outbound only).
// Uses Twitch's standard chat interface: wss://irc-ws.chat.twitch.tv
//
// Setup: Go to dev.twitch.tv → Register App → Get OAuth token.
//        Or use https://twitchapps.com/tmi/ to generate a quick token.
//
// Security:
//   - Allowlist by Twitch username
//   - Optional pairing mode
//   - All communication goes through Twitch's TLS IRC gateway

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

// ── Twitch Config ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitchConfig {
    /// OAuth token (with oauth: prefix or without — we'll add it)
    pub oauth_token: String,
    /// Bot's Twitch username
    pub bot_username: String,
    /// Channels to join (e.g. ["#mychannel", "#friend"])
    pub channels_to_join: Vec<String>,
    pub enabled: bool,
    /// "open" | "allowlist" | "pairing"
    pub dm_policy: String,
    pub allowed_users: Vec<String>,
    #[serde(default)]
    pub pending_users: Vec<PendingUser>,
    pub agent_id: Option<String>,
    /// Only respond when directly addressed (e.g. "@botname hello")
    #[serde(default)]
    pub require_mention: bool,
}

impl Default for TwitchConfig {
    fn default() -> Self {
        TwitchConfig {
            oauth_token: String::new(),
            bot_username: String::new(),
            channels_to_join: vec![],
            enabled: false,
            dm_policy: "open".into(),
            allowed_users: vec![],
            pending_users: vec![],
            agent_id: None,
            require_mention: true,
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

const CONFIG_KEY: &str = "twitch_config";

// ── Bridge Core ────────────────────────────────────────────────────────

pub fn start_bridge(app_handle: tauri::AppHandle) -> EngineResult<()> {
    if BRIDGE_RUNNING.load(Ordering::Relaxed) {
        return Err("Twitch bridge is already running".into());
    }

    let config: TwitchConfig = channels::load_channel_config(&app_handle, CONFIG_KEY)?;
    if config.oauth_token.is_empty() || config.bot_username.is_empty() {
        return Err("OAuth token and bot username are required.".into());
    }
    if config.channels_to_join.is_empty() {
        return Err("At least one channel to join is required.".into());
    }
    if !config.enabled {
        return Err("Twitch bridge is disabled.".into());
    }

    let stop = get_stop_signal();
    stop.store(false, Ordering::Relaxed);
    BRIDGE_RUNNING.store(true, Ordering::Relaxed);

    info!("[twitch] Starting bridge as {}", config.bot_username);

    tauri::async_runtime::spawn(async move {
        loop {
            if get_stop_signal().load(Ordering::Relaxed) { break; }
            if let Err(e) = run_ws_loop(&app_handle, &config).await {
                error!("[twitch] WebSocket error: {}", e);
            }
            if get_stop_signal().load(Ordering::Relaxed) { break; }
            warn!("[twitch] Reconnecting in 5s...");
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
        BRIDGE_RUNNING.store(false, Ordering::Relaxed);
        info!("[twitch] Bridge stopped");
    });

    Ok(())
}

pub fn stop_bridge() {
    let stop = get_stop_signal();
    stop.store(true, Ordering::Relaxed);
    BRIDGE_RUNNING.store(false, Ordering::Relaxed);
    info!("[twitch] Stop signal sent");
}

pub fn get_status(app_handle: &tauri::AppHandle) -> ChannelStatus {
    let config: TwitchConfig = channels::load_channel_config(app_handle, CONFIG_KEY).unwrap_or_default();
    ChannelStatus {
        running: BRIDGE_RUNNING.load(Ordering::Relaxed),
        connected: BRIDGE_RUNNING.load(Ordering::Relaxed),
        bot_name: Some(config.bot_username.clone()),
        bot_id: Some(config.bot_username.clone()),
        message_count: MESSAGE_COUNT.load(Ordering::Relaxed) as u64,
        allowed_users: config.allowed_users,
        pending_users: config.pending_users,
        dm_policy: config.dm_policy,
    }
}

// ── IRC-over-WebSocket Loop ────────────────────────────────────────────

async fn run_ws_loop(app_handle: &tauri::AppHandle, config: &TwitchConfig) -> EngineResult<()> {
    let stop = get_stop_signal();

    // Twitch IRC over WebSocket endpoint
    let url = "wss://irc-ws.chat.twitch.tv:443";
    let (ws_stream, _) = connect_async(url).await
        .map_err(|e| EngineError::Channel { channel: "twitch".into(), message: e.to_string() })?;
    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    // Authenticate
    let token = if config.oauth_token.starts_with("oauth:") {
        config.oauth_token.clone()
    } else {
        format!("oauth:{}", config.oauth_token)
    };
    let nick = config.bot_username.to_lowercase();

    ws_tx.send(WsMessage::Text(format!("PASS {}", token))).await
        .map_err(|e| EngineError::Channel { channel: "twitch".into(), message: e.to_string() })?;
    ws_tx.send(WsMessage::Text(format!("NICK {}", nick))).await
        .map_err(|e| EngineError::Channel { channel: "twitch".into(), message: e.to_string() })?;

    // Request tags capability for user display names etc
    ws_tx.send(WsMessage::Text("CAP REQ :twitch.tv/tags twitch.tv/commands".into())).await
        .map_err(|e| EngineError::Channel { channel: "twitch".into(), message: e.to_string() })?;

    // Wait for successful auth (001/376)
    let mut authed = false;
    for _ in 0..30 {
        let msg = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            ws_rx.next()
        ).await;

        match msg {
            Ok(Some(Ok(WsMessage::Text(t)))) => {
                for line in t.lines() {
                    if line.contains("001") || line.contains("376") {
                        authed = true;
                        break;
                    }
                    if line.contains("NOTICE") && line.contains("Login authentication failed") {
                        return Err("Twitch auth failed — check OAuth token.".into());
                    }
                    if line.starts_with("PING") {
                        let pong = line.replacen("PING", "PONG", 1);
                        let _ = ws_tx.send(WsMessage::Text(pong)).await;
                    }
                }
                if authed { break; }
            }
            _ => continue,
        }
    }

    if !authed {
        return Err("Twitch auth timeout".into());
    }
    info!("[twitch] Authenticated as {}", nick);

    // Join channels
    for ch in &config.channels_to_join {
        let channel = if ch.starts_with('#') { ch.clone() } else { format!("#{}", ch) };
        ws_tx.send(WsMessage::Text(format!("JOIN {}", channel))).await
            .map_err(|e| EngineError::Channel { channel: "twitch".into(), message: e.to_string() })?;
        info!("[twitch] Joined {}", channel);
    }

    let _ = app_handle.emit("twitch-status", json!({
        "kind": "connected",
        "username": &nick,
    }));

    let mut current_config = config.clone();
    let mut last_config_reload = std::time::Instant::now();

    // Message loop
    loop {
        if stop.load(Ordering::Relaxed) { break; }

        let msg = tokio::select! {
            msg = ws_rx.next() => msg,
            _ = tokio::time::sleep(std::time::Duration::from_secs(300)) => {
                // Keepalive
                let _ = ws_tx.send(WsMessage::Text("PING :tmi.twitch.tv".into())).await;
                continue;
            }
        };

        let text = match msg {
            Some(Ok(WsMessage::Text(t))) => t,
            Some(Ok(WsMessage::Close(_))) => break,
            Some(Err(e)) => {
                warn!("[twitch] WS error: {}", e);
                break;
            }
            None => break,
            _ => continue,
        };

        for line in text.lines() {
            // Handle PING
            if line.starts_with("PING") {
                let pong = line.replacen("PING", "PONG", 1);
                let _ = ws_tx.send(WsMessage::Text(pong)).await;
                continue;
            }

            // Parse PRIVMSG: @tags :nick!user@host PRIVMSG #channel :message
            let (tags, rest) = if line.starts_with('@') {
                let space_idx = line.find(' ').unwrap_or(0);
                (&line[1..space_idx], &line[space_idx + 1..])
            } else {
                ("", line)
            };

            if !rest.contains("PRIVMSG") { continue; }

            // Extract sender nick from :nick!user@host
            let sender = if let Some(stripped) = rest.strip_prefix(':') {
                stripped.split('!').next().unwrap_or("")
            } else {
                ""
            };
            if sender.is_empty() || sender.to_lowercase() == nick { continue; }

            // Extract channel and message
            let privmsg_idx = match rest.find("PRIVMSG") {
                Some(i) => i,
                None => continue,
            };
            let after_privmsg = &rest[privmsg_idx + 8..]; // skip "PRIVMSG "
            let (channel, msg_content) = match after_privmsg.find(" :") {
                Some(i) => (&after_privmsg[..i], &after_privmsg[i + 2..]),
                None => continue,
            };

            if msg_content.is_empty() { continue; }

            // Extract display name from tags
            let display_name = parse_tag(tags, "display-name")
                .unwrap_or(sender.to_string());

            // Check if bot is mentioned
            let mention = format!("@{}", nick);
            let is_mentioned = msg_content.to_lowercase().contains(&mention);

            // If require_mention is on, only respond to mentions
            if current_config.require_mention && !is_mentioned { continue; }

            // Strip mention from content
            let content = msg_content
                .replace(&mention, "")
                .replace(&format!("@{}", config.bot_username), "")
                .trim()
                .to_string();
            if content.is_empty() { continue; }

            debug!("[twitch] {} in {}: {}",
                display_name, channel,
                if content.len() > 50 { format!("{}...", crate::engine::types::truncate_utf8(&content, 50)) } else { content.clone() });

            // Access control
            let sender_lower = sender.to_lowercase();
            if let Err(_denial_msg) = channels::check_access(
                &current_config.dm_policy,
                &sender_lower,
                &display_name,
                &display_name,
                &current_config.allowed_users,
                &mut current_config.pending_users,
            ) {
                let _ = channels::save_channel_config(app_handle, CONFIG_KEY, &current_config);
                let _ = app_handle.emit("twitch-status", json!({
                    "kind": "pairing_request",
                    "user_id": &sender_lower,
                    "username": &display_name,
                }));
                // Don't send denial in Twitch chat (too public)
                continue;
            }

            MESSAGE_COUNT.fetch_add(1, Ordering::Relaxed);

            let agent_id = current_config.agent_id.as_deref().unwrap_or("default");
            let ctx = "You are chatting via Twitch chat. Keep responses SHORT — Twitch has a 500 character limit \
                       per message. Use simple text, no markdown. Be casual and fun. \
                       Twitch users expect quick, concise responses.";

            let response = channels::run_channel_agent(
                app_handle, "twitch", ctx, &content, &sender_lower, agent_id,
            ).await;

            match response {
                Ok(reply) if !reply.is_empty() => {
                    // Twitch limit is 500 chars per message
                    for chunk in channels::split_message(&reply, 490) {
                        let irc_msg = format!("PRIVMSG {} :{}", channel, chunk);
                        let _ = ws_tx.send(WsMessage::Text(irc_msg)).await;
                        // Twitch rate limit: ~20 msgs per 30s for regular, ~100 for mods
                        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                    }
                }
                Err(e) => {
                    error!("[twitch] Agent error for {}: {}", sender, e);
                    let err_msg = format!("PRIVMSG {} :⚠️ Error processing your message", channel);
                    let _ = ws_tx.send(WsMessage::Text(err_msg)).await;
                }
                _ => {}
            }
        }

        // Reload config periodically
        if last_config_reload.elapsed() > std::time::Duration::from_secs(30) {
            if let Ok(fresh) = channels::load_channel_config::<TwitchConfig>(app_handle, CONFIG_KEY) {
                current_config = fresh;
            }
            last_config_reload = std::time::Instant::now();
        }
    }

    let _ = app_handle.emit("twitch-status", json!({
        "kind": "disconnected",
    }));

    Ok(())
}

/// Parse a single IRC tag value from the tags string (key1=val1;key2=val2;...)
fn parse_tag(tags: &str, key: &str) -> Option<String> {
    for pair in tags.split(';') {
        let mut kv = pair.splitn(2, '=');
        if kv.next() == Some(key) {
            return kv.next().map(|v| v.to_string());
        }
    }
    None
}

// ── Config Persistence ─────────────────────────────────────────────────

pub fn load_config(app_handle: &tauri::AppHandle) -> EngineResult<TwitchConfig> {
    channels::load_channel_config(app_handle, CONFIG_KEY)
}

pub fn save_config(app_handle: &tauri::AppHandle, config: &TwitchConfig) -> EngineResult<()> {
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
