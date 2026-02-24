// Paw Agent Engine — Discord Bot Bridge
//
// Connects Paw to Discord via the Gateway WebSocket (outbound only — no webhooks).
// The bot opens a persistent WebSocket to Discord's gateway, receives message
// events, and replies via the REST API.
//
// Setup: discord.com/developers → New Application → Bot → Copy Token → paste in Paw.
//
// Security:
//   - Allowlist by Discord user ID
//   - Optional pairing mode (first DM from unknown user → pending approval)
//   - All communication goes through Discord's TLS gateway + REST API

use crate::engine::channels::{self, PendingUser, ChannelStatus};
use log::{info, warn, error};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};
use futures::{SinkExt, StreamExt};
use crate::atoms::error::{EngineResult, EngineError};

/// Maximum reconnect attempts before giving up entirely.
const MAX_RECONNECT_ATTEMPTS: u32 = 8;

// ── Discord API Types ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct GatewayPayload {
    op: u8,
    d: Option<serde_json::Value>,
    s: Option<u64>,         // sequence number
    t: Option<String>,      // event name
}

#[derive(Debug, Deserialize)]
struct ReadyEvent {
    user: DiscordUser,
    session_id: String,
    resume_gateway_url: String,
}

#[derive(Debug, Clone, Deserialize)]
struct DiscordUser {
    #[allow(dead_code)]
    id: String,
    username: String,
    #[allow(dead_code)]
    discriminator: Option<String>,
    bot: Option<bool>,
    global_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DiscordMessage {
    #[allow(dead_code)]
    id: String,
    channel_id: String,
    author: DiscordUser,
    content: String,
    guild_id: Option<String>,
    // mentions field for detecting bot mentions in guilds
    mentions: Option<Vec<DiscordUser>>,
}

// ── Discord Config ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordConfig {
    pub bot_token: String,
    pub enabled: bool,
    /// "open" | "allowlist" | "pairing"
    pub dm_policy: String,
    /// Discord user IDs (snowflakes as strings)
    pub allowed_users: Vec<String>,
    #[serde(default)]
    pub pending_users: Vec<PendingUser>,
    /// Which agent to route messages to
    pub agent_id: Option<String>,
    /// Whether to respond in guild (server) channels when mentioned
    #[serde(default = "default_true")]
    pub respond_to_mentions: bool,
    /// Phase C: allow dangerous/side-effect tools for messages from this channel
    #[serde(default)]
    pub allow_dangerous_tools: bool,
}

fn default_true() -> bool { true }

impl Default for DiscordConfig {
    fn default() -> Self {
        DiscordConfig {
            bot_token: String::new(),
            enabled: false,
            dm_policy: "pairing".into(),
            allowed_users: vec![],
            pending_users: vec![],
            agent_id: None,
            respond_to_mentions: true,
            allow_dangerous_tools: false,
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

const DISCORD_GATEWAY_URL: &str = "wss://gateway.discord.gg/?v=10&encoding=json";
const DISCORD_API: &str = "https://discord.com/api/v10";
const CONFIG_KEY: &str = "discord_config";

// ── Bridge Core ────────────────────────────────────────────────────────

pub fn start_bridge(app_handle: tauri::AppHandle) -> EngineResult<()> {
    if BRIDGE_RUNNING.load(Ordering::Relaxed) {
        return Err("Discord bridge is already running".into());
    }

    let mut config: DiscordConfig = channels::load_channel_config(&app_handle, CONFIG_KEY)
        .unwrap_or_default();

    // ── Skill-credential sync ──────────────────────────────────────────
    // The skill vault is the source of truth for the bot token. Users
    // configure it in Settings → Skills → Discord. Always check it and
    // prefer it over whatever's in the bridge config (which may be stale
    // from a previous failed sync).
    if let Some(state) = app_handle.try_state::<crate::engine::state::EngineState>() {
        if let Ok(creds) = crate::engine::skills::get_skill_credentials(&state.store, "discord") {
            if let Some(token) = creds.get("DISCORD_BOT_TOKEN") {
                if !token.is_empty() && *token != config.bot_token {
                    info!("[discord] Syncing bot token from skill credentials");
                    config.bot_token = token.clone();
                    config.enabled = true;
                    let _ = channels::save_channel_config(&app_handle, CONFIG_KEY, &config);
                }
            }
        }
    }

    if config.bot_token.is_empty() {
        return Err("No bot token configured. Add it in Settings → Skills → Discord, or in Channels → Discord.".into());
    }
    if !config.enabled {
        return Err("Discord bridge is disabled.".into());
    }

    let stop = get_stop_signal();
    stop.store(false, Ordering::Relaxed);
    BRIDGE_RUNNING.store(true, Ordering::Relaxed);

    info!("[discord] Starting bridge with policy={}", config.dm_policy);

    tauri::async_runtime::spawn(async move {
        let mut reconnect_attempt: u32 = 0;
        let mut live_config = config;
        loop {
            match run_gateway_loop(app_handle.clone(), live_config.clone()).await {
                Ok(()) => break, // Clean shutdown
                Err(e) => {
                    if get_stop_signal().load(Ordering::Relaxed) { break; }

                    // ── Fatal error classification ─────────────────────────
                    // 4004 and 4014 are non-recoverable without user action.
                    let msg = e.to_string();
                    let is_fatal = msg.contains("4004") || msg.contains("4014");

                    if is_fatal {
                        error!("[discord] Fatal: {} — stopping (user must fix config)", msg);
                        let _ = app_handle.emit("discord-status", json!({
                            "kind": "error",
                            "message": msg,
                        }));
                        break;
                    }

                    reconnect_attempt += 1;
                    if reconnect_attempt > MAX_RECONNECT_ATTEMPTS {
                        error!("[discord] Max reconnect attempts ({}) reached — giving up", MAX_RECONNECT_ATTEMPTS);
                        break;
                    }

                    error!("[discord] Bridge error: {} — reconnecting", e);
                    let delay = crate::engine::http::reconnect_delay(reconnect_attempt - 1).await;
                    warn!("[discord] Reconnecting in {}ms (attempt {})", delay.as_millis(), reconnect_attempt);
                    if get_stop_signal().load(Ordering::Relaxed) { break; }

                    // ── Re-read config on reconnect ────────────────────────
                    // The user may have updated the token while we were
                    // retrying.  Skill vault is source of truth.
                    if let Ok(fresh) = channels::load_channel_config::<DiscordConfig>(&app_handle, CONFIG_KEY) {
                        live_config = fresh;
                    }
                    if let Some(state) = app_handle.try_state::<crate::engine::state::EngineState>() {
                        if let Ok(creds) = crate::engine::skills::get_skill_credentials(&state.store, "discord") {
                            if let Some(token) = creds.get("DISCORD_BOT_TOKEN") {
                                if !token.is_empty() && *token != live_config.bot_token {
                                    info!("[discord] Picked up updated token from skill credentials");
                                    live_config.bot_token = token.clone();
                                    live_config.enabled = true;
                                    let _ = channels::save_channel_config(&app_handle, CONFIG_KEY, &live_config);
                                }
                            }
                        }
                    }

                    if live_config.bot_token.is_empty() {
                        error!("[discord] No token after re-read — stopping");
                        break;
                    }
                }
            }
        }
        BRIDGE_RUNNING.store(false, Ordering::Relaxed);
        info!("[discord] Bridge stopped");
    });

    Ok(())
}

pub fn stop_bridge() {
    let stop = get_stop_signal();
    stop.store(true, Ordering::Relaxed);
    BRIDGE_RUNNING.store(false, Ordering::Relaxed);
    info!("[discord] Stop signal sent");
}

pub fn get_status(app_handle: &tauri::AppHandle) -> ChannelStatus {
    let config: DiscordConfig = channels::load_channel_config(app_handle, CONFIG_KEY).unwrap_or_default();
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

// ── Discord Gateway WebSocket Loop ─────────────────────────────────────

async fn run_gateway_loop(app_handle: tauri::AppHandle, config: DiscordConfig) -> EngineResult<()> {
    let stop = get_stop_signal();
    let http_client = reqwest::Client::new();
    let token = config.bot_token.clone();

    // Connect to Gateway
    let (ws_stream, _) = connect_async(DISCORD_GATEWAY_URL)
        .await
        .map_err(|e| EngineError::Channel { channel: "discord".into(), message: e.to_string() })?;

    let (mut write, mut read) = ws_stream.split();

    // Read Hello (op 10) to get heartbeat interval
    let hello = read.next().await
        .ok_or("Gateway closed before Hello")?
        .map_err(|e| EngineError::Channel { channel: "discord".into(), message: e.to_string() })?;
    let hello_payload: GatewayPayload = serde_json::from_str(
        hello.to_text().map_err(|e| EngineError::Channel { channel: "discord".into(), message: e.to_string() })?
    )?;

    if hello_payload.op != 10 {
        return Err(format!("Expected Hello (op 10), got op {}", hello_payload.op).into());
    }

    let heartbeat_interval = hello_payload.d
        .as_ref()
        .and_then(|d| d["heartbeat_interval"].as_u64())
        .unwrap_or(41250);

    info!("[discord] Connected to gateway, heartbeat_interval={}ms", heartbeat_interval);

    // Send Identify (op 2)
    // Intents: GUILDS (1<<0) + GUILD_MESSAGES (1<<9) + DIRECT_MESSAGES (1<<12) + MESSAGE_CONTENT (1<<15)
    //
    // MESSAGE_CONTENT (1<<15) is a privileged intent.  The user must enable it
    // at discord.com/developers → Bot → Privileged Gateway Intents → Message Content Intent.
    // Without it, guild messages (even @mentions) arrive with empty content.
    // DMs always include content regardless.
    //
    // If the user hasn't enabled the intent, Discord closes with 4014 and
    // the bridge logs an actionable error message pointing them to the portal.
    let intents = (1 << 0) | (1 << 9) | (1 << 12) | (1 << 15);
    let identify = json!({
        "op": 2,
        "d": {
            "token": token,
            "intents": intents,
            "properties": {
                "os": std::env::consts::OS,
                "browser": "paw",
                "device": "paw"
            }
        }
    });
    write.send(WsMessage::Text(identify.to_string()))
        .await
        .map_err(|e| EngineError::Channel { channel: "discord".into(), message: e.to_string() })?;

    // State
    let mut _sequence: Option<u64> = None;
    let mut _session_id_discord: Option<String> = None;
    let mut _resume_url: Option<String> = None;
    let mut current_config = config;
    let mut last_config_reload = std::time::Instant::now();

    // Heartbeat task
    let stop_hb = stop.clone();
    let heartbeat_interval_ms = heartbeat_interval;
    let (hb_tx, mut hb_rx) = tokio::sync::mpsc::channel::<Option<u64>>(16);

    let hb_write = Arc::new(tokio::sync::Mutex::new(write));
    let hb_write_clone = hb_write.clone();

    let heartbeat_task = tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(heartbeat_interval_ms)).await;
            if stop_hb.load(Ordering::Relaxed) { break; }

            // Get latest sequence
            let seq = hb_rx.try_recv().ok().flatten();
            let hb = json!({ "op": 1, "d": seq });
            let mut w = hb_write_clone.lock().await;
            if let Err(e) = w.send(WsMessage::Text(hb.to_string())).await {
                warn!("[discord] Heartbeat send failed: {}", e);
                break;
            }
        }
    });

    // Main event loop
    while let Some(msg_result) = read.next().await {
        if stop.load(Ordering::Relaxed) { break; }

        let msg = match msg_result {
            Ok(m) => m,
            Err(e) => {
                warn!("[discord] WS read error: {}", e);
                break;
            }
        };

        let text = match msg {
            WsMessage::Text(t) => t,
            WsMessage::Close(frame) => {
                let (code, reason) = frame
                    .map(|f| (f.code.into(), f.reason.to_string()))
                    .unwrap_or((0u16, String::new()));
                // Discord close codes: https://discord.com/developers/docs/topics/opcodes-and-status-codes
                match code {
                    4004 => {
                        error!("[discord] Authentication failed (4004) — invalid bot token");
                        return Err(EngineError::Channel {
                            channel: "discord".into(),
                            message: "Invalid bot token. Check your token at discord.com/developers.".into(),
                        });
                    }
                    4014 => {
                        error!("[discord] Disallowed intents (4014) — enable Privileged Gateway Intents at discord.com/developers → Bot");
                        return Err(EngineError::Channel {
                            channel: "discord".into(),
                            message: "Disallowed intents. Enable 'Message Content Intent' in Discord Developer Portal → Bot → Privileged Gateway Intents.".into(),
                        });
                    }
                    1000 | 0 => {
                        info!("[discord] Gateway closed normally (code={})", code);
                        break;
                    }
                    _ => {
                        warn!("[discord] Gateway closed: code={} reason={}", code, reason);
                        return Err(EngineError::Channel {
                            channel: "discord".into(),
                            message: format!("Gateway closed: code={} {}", code, reason),
                        });
                    }
                }
            }
            _ => continue,
        };

        let payload: GatewayPayload = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => continue,
        };

        // Update sequence
        if let Some(s) = payload.s {
            _sequence = Some(s);
            let _ = hb_tx.try_send(Some(s));
        }

        match payload.op {
            // Dispatch (events)
            0 => {
                let event_name = payload.t.as_deref().unwrap_or("");
                match event_name {
                    "READY" => {
                        if let Some(d) = &payload.d {
                            if let Ok(ready) = serde_json::from_value::<ReadyEvent>(d.clone()) {
                                info!("[discord] Ready as {} ({})", ready.user.username, ready.user.id);
                                let _ = BOT_USER_ID.set(ready.user.id.clone());
                                let _ = BOT_USERNAME.set(ready.user.username.clone());
                                _session_id_discord = Some(ready.session_id);
                                _resume_url = Some(ready.resume_gateway_url);

                                let _ = app_handle.emit("discord-status", json!({
                                    "kind": "connected",
                                    "bot_username": &ready.user.username,
                                    "bot_id": &ready.user.id,
                                }));
                            }
                        }
                    }
                    "MESSAGE_CREATE" => {
                        if let Some(d) = payload.d {
                            if let Ok(discord_msg) = serde_json::from_value::<DiscordMessage>(d) {
                                // Skip bot messages (including own)
                                if discord_msg.author.bot.unwrap_or(false) { continue; }
                                if discord_msg.content.is_empty() {
                                    info!("[discord] Ignoring empty content from {} (guild={:?}) — may need MESSAGE_CONTENT intent",
                                        discord_msg.author.username, discord_msg.guild_id);
                                    continue;
                                }

                                let is_dm = discord_msg.guild_id.is_none();
                                let is_mention = discord_msg.mentions.as_ref()
                                    .map(|m| m.iter().any(|u| BOT_USER_ID.get().map(|id| id == &u.id).unwrap_or(false)))
                                    .unwrap_or(false);

                                // Only respond to DMs or @mentions in servers
                                if !is_dm && !is_mention { continue; }
                                if !is_dm && !current_config.respond_to_mentions { continue; }

                                // Strip bot mention from message content
                                let content = if is_mention {
                                    let bot_id = BOT_USER_ID.get().map(|s| s.as_str()).unwrap_or("");
                                    let mention_pat = format!("<@{}>", bot_id);
                                    discord_msg.content.replace(&mention_pat, "").trim().to_string()
                                } else {
                                    discord_msg.content.clone()
                                };

                                if content.is_empty() { continue; }

                                let user_id = discord_msg.author.id.clone();
                                let username = discord_msg.author.username.clone();
                                let display_name = discord_msg.author.global_name.clone().unwrap_or(username.clone());
                                let channel_id = discord_msg.channel_id.clone();

                                info!("[discord] Message from {} ({}) in {}: {}",
                                    username, user_id,
                                    if is_dm { "DM".to_string() } else { format!("guild channel {}", channel_id) },
                                    if content.len() > 80 { format!("{}...", &content[..content.floor_char_boundary(80)]) } else { content.clone() });

                                // Access control (DMs only — mentions in servers bypass for now)
                                if is_dm {
                                    if let Err(denial_msg) = channels::check_access(
                                        &current_config.dm_policy,
                                        &user_id,
                                        &username,
                                        &display_name,
                                        &current_config.allowed_users,
                                        &mut current_config.pending_users,
                                    ) {
                                        let denial_str = denial_msg.to_string();
                                        let _ = channels::save_channel_config(&app_handle, CONFIG_KEY, &current_config);
                                        let _ = app_handle.emit("discord-status", json!({
                                            "kind": "pairing_request",
                                            "user_id": &user_id,
                                            "username": &username,
                                        }));
                                        let _ = send_message(&http_client, &token, &channel_id, &denial_str).await;
                                        continue;
                                    }
                                }

                                MESSAGE_COUNT.fetch_add(1, Ordering::Relaxed);

                                // Spawn agent response in a separate task so we
                                // don't block the gateway event loop (heartbeats
                                // must keep flowing while the LLM is thinking).
                                let ah = app_handle.clone();
                                let http = http_client.clone();
                                let tok = token.clone();
                                let cid = channel_id.clone();
                                let uid = user_id.clone();
                                let cfg_agent = current_config.agent_id.clone();
                                let cfg_dangerous = current_config.allow_dangerous_tools;

                                tauri::async_runtime::spawn(async move {
                                    // Send typing indicator
                                    let _ = send_typing(&http, &tok, &cid).await;

                                    let agent_id = cfg_agent.as_deref().unwrap_or("default");
                                    let ctx = "You are chatting via Discord. Keep responses concise and conversational. \
                                               Use Discord markdown (bold, italic, code blocks, spoilers). \
                                               Max message length is 2000 characters. \
                                               You have the `fetch` tool available for Discord REST API calls — use it directly. \
                                               Do NOT install community skills or search for Discord tools. \
                                               The Discord skill instructions in your system prompt have the full API reference.";

                                    info!("[discord] Routing message from {} to agent '{}'", uid, agent_id);
                                    let response = channels::run_channel_agent(
                                        &ah, "discord", ctx, &content, &uid, agent_id,
                                        cfg_dangerous,
                                    ).await;

                                    match response {
                                        Ok(ref reply) if !reply.is_empty() => {
                                            info!("[discord] Sending reply to {} ({} chars)", uid, reply.len());
                                            for chunk in channels::split_message(reply, 1950) {
                                                let _ = send_message(&http, &tok, &cid, &chunk).await;
                                            }
                                        }
                                        Ok(_) => {
                                            warn!("[discord] Agent returned empty reply for {}", uid);
                                        }
                                        Err(e) => {
                                            error!("[discord] Agent error for {}: {}", uid, e);
                                            let _ = send_message(&http, &tok, &cid,
                                                &format!("⚠️ Error: {}", e)).await;
                                        }
                                    }
                                });
                            }
                        }
                    }
                    _ => {} // Ignore other events
                }
            }
            // Heartbeat ACK
            11 => {}
            // Reconnect
            7 => {
                info!("[discord] Gateway requested reconnect");
                break;
            }
            // Invalid Session
            9 => {
                let resumable = payload.d.as_ref().and_then(|d| d.as_bool()).unwrap_or(false);
                warn!("[discord] Invalid session (resumable={})", resumable);
                return Err(EngineError::Channel {
                    channel: "discord".into(),
                    message: "Invalid session — Discord rejected the connection".into(),
                });
            }
            _ => {}
        }

        // Reload config periodically
        if last_config_reload.elapsed() > std::time::Duration::from_secs(30) {
            if let Ok(fresh) = channels::load_channel_config::<DiscordConfig>(&app_handle, CONFIG_KEY) {
                current_config = fresh;
            }
            last_config_reload = std::time::Instant::now();
        }
    }

    heartbeat_task.abort();

    let _ = app_handle.emit("discord-status", json!({
        "kind": "disconnected",
    }));

    Ok(())
}

// ── Discord REST API Helpers ───────────────────────────────────────────

async fn send_message(
    client: &reqwest::Client,
    token: &str,
    channel_id: &str,
    content: &str,
) -> EngineResult<()> {
    let url = format!("{}/channels/{}/messages", DISCORD_API, channel_id);
    let resp = client.post(&url)
        .header("Authorization", format!("Bot {}", token))
        .json(&json!({ "content": content }))
        .send().await;

    match resp {
        Ok(r) if !r.status().is_success() => {
            let status = r.status();
            let body = r.text().await.unwrap_or_default();
            warn!("[discord] sendMessage {} failed: {} {}", channel_id, status, body);
        }
        Err(e) => warn!("[discord] sendMessage failed: {}", e),
        _ => {}
    }
    Ok(())
}

async fn send_typing(
    client: &reqwest::Client,
    token: &str,
    channel_id: &str,
) -> EngineResult<()> {
    let url = format!("{}/channels/{}/typing", DISCORD_API, channel_id);
    let _ = client.post(&url)
        .header("Authorization", format!("Bot {}", token))
        .send().await;
    Ok(())
}

// ── Config Persistence ─────────────────────────────────────────────────

pub fn load_config(app_handle: &tauri::AppHandle) -> EngineResult<DiscordConfig> {
    channels::load_channel_config(app_handle, CONFIG_KEY)
}

pub fn save_config(app_handle: &tauri::AppHandle, config: &DiscordConfig) -> EngineResult<()> {
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
