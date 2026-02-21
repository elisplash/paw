// Paw Agent Engine — Telegram Bot Bridge
//
// Connects Paw to Telegram via the Bot API using long-polling (getUpdates).
// No public URL needed, no webhooks, no middleman — your local Paw pulls
// messages directly from Telegram's servers.
//
// Setup: message @BotFather → get a bot token → paste in Paw → done.
//
// Security:
//   - Allowlist by Telegram user ID (only allowed users can talk to Paw)
//   - Optional pairing mode (first message from unknown user → pending approval)
//   - All communication goes through Telegram's TLS API
//   - Bot token stored encrypted in engine DB

use crate::engine::channels;
use crate::engine::state::EngineState;
use log::{debug, info, warn, error};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use tauri::{Emitter, Manager};
use crate::atoms::error::EngineResult;

// ── Telegram API Types ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct TgResponse<T> {
    ok: bool,
    result: Option<T>,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TgUpdate {
    update_id: i64,
    message: Option<TgMessage>,
}

#[derive(Debug, Deserialize)]
struct TgMessage {
    message_id: i64,
    from: Option<TgUser>,
    chat: TgChat,
    text: Option<String>,
    #[allow(dead_code)]
    date: i64,
}

#[derive(Debug, Deserialize)]
struct TgUser {
    id: i64,
    is_bot: bool,
    first_name: String,
    #[allow(dead_code)]
    last_name: Option<String>,
    username: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TgChat {
    id: i64,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    chat_type: String,
}

// ── Telegram Config ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    pub bot_token: String,
    pub enabled: bool,
    /// "open" = anyone can message, "allowlist" = only allowed_users, "pairing" = approve first message
    pub dm_policy: String,
    /// Telegram user IDs that are allowed to talk to the bot
    pub allowed_users: Vec<i64>,
    /// Pending pairing requests (user_id → username)
    #[serde(default)]
    pub pending_users: Vec<PendingUser>,
    /// Which agent to route messages to (default = "default")
    pub agent_id: Option<String>,
    /// Max messages to keep as context per user session
    pub context_window: Option<usize>,
    /// Known users: maps username (lowercase, no @) → chat_id for proactive messaging
    #[serde(default)]
    pub known_users: std::collections::HashMap<String, i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingUser {
    pub user_id: i64,
    pub username: String,
    pub first_name: String,
    pub requested_at: String,
}

impl Default for TelegramConfig {
    fn default() -> Self {
        TelegramConfig {
            bot_token: String::new(),
            enabled: false,
            dm_policy: "pairing".into(),
            allowed_users: vec![],
            pending_users: vec![],
            agent_id: None,
            context_window: Some(50),
            known_users: std::collections::HashMap::new(),
        }
    }
}

// ── Telegram Bridge Status ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramStatus {
    pub running: bool,
    pub connected: bool,
    pub bot_username: Option<String>,
    pub bot_name: Option<String>,
    pub message_count: u64,
    pub last_message_at: Option<String>,
    pub allowed_users: Vec<i64>,
    pub pending_users: Vec<PendingUser>,
    pub dm_policy: String,
}

// ── Global State ───────────────────────────────────────────────────────

static BRIDGE_RUNNING: AtomicBool = AtomicBool::new(false);
static MESSAGE_COUNT: AtomicI64 = AtomicI64::new(0);

/// Stored bot info after successful getMe check
static BOT_USERNAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
static BOT_NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Channel to signal the polling loop to stop
static STOP_SIGNAL: std::sync::OnceLock<Arc<AtomicBool>> = std::sync::OnceLock::new();

fn get_stop_signal() -> Arc<AtomicBool> {
    STOP_SIGNAL.get_or_init(|| Arc::new(AtomicBool::new(false))).clone()
}

// ── API Helpers ────────────────────────────────────────────────────────

const TG_API: &str = "https://api.telegram.org/bot";

async fn tg_get_me(client: &reqwest::Client, token: &str) -> EngineResult<(String, String)> {
    let url = format!("{}{}/getMe", TG_API, token);
    let resp: TgResponse<serde_json::Value> = client
        .get(&url)
        .send().await?
        .json().await?;

    if !resp.ok {
        return Err(format!("getMe failed: {}", resp.description.unwrap_or_default()).into());
    }

    let result = resp.result.ok_or("getMe: no result")?;
    let username = result["username"].as_str().unwrap_or("unknown").to_string();
    let name = result["first_name"].as_str().unwrap_or("Bot").to_string();
    Ok((username, name))
}

async fn tg_get_updates(
    client: &reqwest::Client,
    token: &str,
    offset: i64,
    timeout: u64,
) -> EngineResult<Vec<TgUpdate>> {
    let url = format!(
        "{}{}/getUpdates?offset={}&timeout={}&allowed_updates=[\"message\"]",
        TG_API, token, offset, timeout
    );
    let resp: TgResponse<Vec<TgUpdate>> = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(timeout + 10))
        .send().await?
        .json().await?;

    if !resp.ok {
        return Err(format!("getUpdates error: {}", resp.description.unwrap_or_default()).into());
    }

    Ok(resp.result.unwrap_or_default())
}

async fn tg_send_message(
    client: &reqwest::Client,
    token: &str,
    chat_id: i64,
    text: &str,
    reply_to: Option<i64>,
) -> EngineResult<()> {
    // Telegram message limit = 4096 chars. Split if needed.
    let chunks = channels::split_message(text, 4000);
    for (i, chunk) in chunks.iter().enumerate() {
        let mut body = serde_json::json!({
            "chat_id": chat_id,
            "text": chunk,
            "parse_mode": "Markdown",
        });
        // Only reply to original message on first chunk
        if i == 0 {
            if let Some(msg_id) = reply_to {
                body["reply_to_message_id"] = serde_json::json!(msg_id);
            }
        }

        let url = format!("{}{}/sendMessage", TG_API, token);
        let resp = client.post(&url)
            .json(&body)
            .send().await;

        match resp {
            Ok(r) => {
                if !r.status().is_success() {
                    // Retry without Markdown parse mode (some responses break MD parsing)
                    let mut retry_body = serde_json::json!({
                        "chat_id": chat_id,
                        "text": chunk,
                    });
                    if i == 0 {
                        if let Some(msg_id) = reply_to {
                            retry_body["reply_to_message_id"] = serde_json::json!(msg_id);
                        }
                    }
                    let _ = client.post(&url).json(&retry_body).send().await;
                }
            }
            Err(e) => {
                warn!("[telegram] sendMessage failed: {}", e);
            }
        }
    }
    Ok(())
}

async fn tg_send_chat_action(
    client: &reqwest::Client,
    token: &str,
    chat_id: i64,
) -> EngineResult<()> {
    let url = format!("{}{}/sendChatAction", TG_API, token);
    let body = serde_json::json!({
        "chat_id": chat_id,
        "action": "typing",
    });
    let _ = client.post(&url).json(&body).send().await;
    Ok(())
}

/// Check if the Telegram bridge is currently running.
pub fn is_bridge_running() -> bool {
    BRIDGE_RUNNING.load(Ordering::Relaxed)
}

// ── Bridge Core ────────────────────────────────────────────────────────

/// Start the Telegram polling bridge. Returns immediately;
/// the actual polling runs in a background tokio task.
pub fn start_bridge(app_handle: tauri::AppHandle) -> EngineResult<()> {
    if BRIDGE_RUNNING.load(Ordering::Relaxed) {
        return Err("Telegram bridge is already running".into());
    }

    // Load config
    let config = load_telegram_config(&app_handle)?;
    if config.bot_token.is_empty() {
        return Err("No bot token configured. Get one from @BotFather on Telegram.".into());
    }
    if !config.enabled {
        return Err("Telegram bridge is disabled. Enable it in Channels settings.".into());
    }

    let stop = get_stop_signal();
    stop.store(false, Ordering::Relaxed);
    BRIDGE_RUNNING.store(true, Ordering::Relaxed);

    info!("[telegram] Starting bridge with policy={}", config.dm_policy);

    tauri::async_runtime::spawn(async move {
        let mut reconnect_attempt: u32 = 0;
        loop {
            match run_polling_loop(app_handle.clone(), config.clone()).await {
                Ok(()) => break,
                Err(e) => {
                    if get_stop_signal().load(Ordering::Relaxed) { break; }
                    error!("[telegram] Bridge error: {} — reconnecting", e);
                    let delay = crate::engine::http::reconnect_delay(reconnect_attempt).await;
                    warn!("[telegram] Reconnecting in {}ms (attempt {})", delay.as_millis(), reconnect_attempt + 1);
                    reconnect_attempt += 1;
                    if get_stop_signal().load(Ordering::Relaxed) { break; }
                }
            }
            reconnect_attempt = 0;
        }
        BRIDGE_RUNNING.store(false, Ordering::Relaxed);
        info!("[telegram] Bridge stopped");
    });

    Ok(())
}

/// Stop the Telegram bridge.
pub fn stop_bridge() {
    let stop = get_stop_signal();
    stop.store(true, Ordering::Relaxed);
    BRIDGE_RUNNING.store(false, Ordering::Relaxed);
    info!("[telegram] Stop signal sent");
}

/// Get the current bridge status.
pub fn get_status(app_handle: &tauri::AppHandle) -> TelegramStatus {
    let config = load_telegram_config(app_handle).unwrap_or_default();
    TelegramStatus {
        running: BRIDGE_RUNNING.load(Ordering::Relaxed),
        connected: BRIDGE_RUNNING.load(Ordering::Relaxed),
        bot_username: BOT_USERNAME.get().cloned(),
        bot_name: BOT_NAME.get().cloned(),
        message_count: MESSAGE_COUNT.load(Ordering::Relaxed) as u64,
        last_message_at: None, // Could track this if needed
        allowed_users: config.allowed_users,
        pending_users: config.pending_users,
        dm_policy: config.dm_policy,
    }
}

/// The main polling loop. Runs forever until stop signal.
async fn run_polling_loop(app_handle: tauri::AppHandle, config: TelegramConfig) -> EngineResult<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;

    // Verify bot token with getMe
    let (username, name) = tg_get_me(&client, &config.bot_token).await?;
    info!("[telegram] Connected as @{} ({})", username, name);
    let _ = BOT_USERNAME.set(username.clone());
    let _ = BOT_NAME.set(name.clone());

    // Emit connected event to frontend
    let _ = app_handle.emit("telegram-status", serde_json::json!({
        "kind": "connected",
        "bot_username": &username,
        "bot_name": &name,
    }));

    let stop = get_stop_signal();
    let mut offset: i64 = 0;
    let token = config.bot_token.clone();
    let mut current_config = config;

    loop {
        if stop.load(Ordering::Relaxed) {
            info!("[telegram] Stop signal received, exiting poll loop");
            break;
        }

        // Long poll (30s timeout on Telegram side)
        match tg_get_updates(&client, &token, offset, 30).await {
            Ok(updates) => {
                for update in updates {
                    offset = update.update_id + 1;

                    if let Some(msg) = update.message {
                        // Skip bot messages
                        if msg.from.as_ref().map(|u| u.is_bot).unwrap_or(true) {
                            continue;
                        }
                        // Skip non-text messages for now
                        let text = match &msg.text {
                            Some(t) if !t.is_empty() => t.clone(),
                            _ => continue,
                        };
                        let user = msg.from.as_ref().unwrap();
                        let user_id = user.id;
                        let username = user.username.clone().unwrap_or_else(|| user.first_name.clone());
                        let chat_id = msg.chat.id;

                        debug!("[telegram] Message from {} ({}): {}", username, user_id, 
                            if text.len() > 50 { format!("{}...", crate::engine::types::truncate_utf8(&text, 50)) } else { text.clone() });

                        // ── Access control ─────────────────────────────
                        match current_config.dm_policy.as_str() {
                            "allowlist" => {
                                if !current_config.allowed_users.contains(&user_id) {
                                    let _ = tg_send_message(&client, &token, chat_id,
                                        "⛔ You're not on the allowlist. Ask the Paw owner to add your user ID.",
                                        Some(msg.message_id)).await;
                                    continue;
                                }
                            }
                            "pairing" => {
                                if !current_config.allowed_users.contains(&user_id) {
                                    // Check if already pending
                                    if !current_config.pending_users.iter().any(|p| p.user_id == user_id) {
                                        // Add to pending
                                        let pending = PendingUser {
                                            user_id,
                                            username: username.clone(),
                                            first_name: user.first_name.clone(),
                                            requested_at: chrono::Utc::now().to_rfc3339(),
                                        };
                                        current_config.pending_users.push(pending);
                                        // Save updated config
                                        let _ = save_telegram_config(&app_handle, &current_config);

                                        // Notify frontend
                                        let _ = app_handle.emit("telegram-status", serde_json::json!({
                                            "kind": "pairing_request",
                                            "user_id": user_id,
                                            "username": &username,
                                        }));
                                    }
                                    let _ = tg_send_message(&client, &token, chat_id,
                                        "🔒 Pairing request sent to Paw. Waiting for approval...",
                                        Some(msg.message_id)).await;
                                    continue;
                                }
                            }
                            // "open" — allow everyone
                            _ => {}
                        }

                        MESSAGE_COUNT.fetch_add(1, Ordering::Relaxed);

                        // ── Store username → chat_id for proactive messaging ──
                        if let Some(uname) = &user.username {
                            let key = uname.to_lowercase();
                            if !current_config.known_users.contains_key(&key) || current_config.known_users.get(&key) != Some(&chat_id) {
                                current_config.known_users.insert(key, chat_id);
                                let _ = save_telegram_config(&app_handle, &current_config);
                            }
                        }

                        // ── Send "typing" indicator ─────────────────────
                        let _ = tg_send_chat_action(&client, &token, chat_id).await;

                        // ── Route to agent loop ─────────────────────────
                        // Prune old messages to bound TG session growth
                        let agent_id_str = current_config.agent_id.as_deref().unwrap_or("default");
                        if let Some(st) = app_handle.try_state::<EngineState>() {
                            let tg_session_id = format!("eng-telegram-{}-{}", agent_id_str, user_id);
                            let _ = st.store.prune_session_messages(&tg_session_id, 50);
                        }
                        let response = channels::run_channel_agent(
                            &app_handle,
                            "telegram",
                            "You are chatting via Telegram. The user is messaging you from their phone. \
                             Keep responses concise and mobile-friendly. Use Markdown formatting supported by Telegram \
                             (bold, italic, code, links). Avoid very long responses unless explicitly asked.",
                            &text,
                            &user_id.to_string(),
                            agent_id_str,
                        ).await;

                        match response {
                            Ok(reply) => {
                                if !reply.is_empty() {
                                    let _ = tg_send_message(&client, &token, chat_id, &reply, Some(msg.message_id)).await;
                                }
                            }
                            Err(e) => {
                                error!("[telegram] Agent error for user {}: {}", user_id, e);
                                let _ = tg_send_message(&client, &token, chat_id,
                                    &format!("⚠️ Error: {}", e), Some(msg.message_id)).await;
                            }
                        }
                    }
                }
            }
            Err(e) => {
                warn!("[telegram] Poll error: {} — retrying in 5s", e);
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }

        // Reload config periodically (picks up allowlist changes, etc.)
        if let Ok(fresh_config) = load_telegram_config(&app_handle) {
            current_config = fresh_config;
        }
    }

    let _ = app_handle.emit("telegram-status", serde_json::json!({
        "kind": "disconnected",
    }));

    Ok(())
}


// ── Config Persistence ─────────────────────────────────────────────────

pub fn load_telegram_config(app_handle: &tauri::AppHandle) -> EngineResult<TelegramConfig> {
    let engine_state = app_handle.try_state::<EngineState>()
        .ok_or("Engine not initialized")?;

    match engine_state.store.get_config("telegram_config") {
        Ok(Some(json)) => {
            Ok(serde_json::from_str::<TelegramConfig>(&json)?)
        }
        _ => Ok(TelegramConfig::default()),
    }
}

pub fn save_telegram_config(app_handle: &tauri::AppHandle, config: &TelegramConfig) -> EngineResult<()> {
    let engine_state = app_handle.try_state::<EngineState>()
        .ok_or("Engine not initialized")?;

    let json = serde_json::to_string(config)?;

    engine_state.store.set_config("telegram_config", &json)?;
    Ok(())
}

/// Approve a pending pairing request. Adds user to allowlist, removes from pending.
/// Sends a confirmation message to the user on Telegram.
pub async fn approve_user(app_handle: &tauri::AppHandle, user_id: i64) -> EngineResult<()> {
    let mut config = load_telegram_config(app_handle)?;

    if !config.allowed_users.contains(&user_id) {
        config.allowed_users.push(user_id);
    }
    config.pending_users.retain(|p| p.user_id != user_id);

    save_telegram_config(app_handle, &config)?;
    info!("[telegram] User {} approved", user_id);

    // Send confirmation to the user on Telegram
    if !config.bot_token.is_empty() {
        let client = reqwest::Client::new();
        let _ = tg_send_message(
            &client,
            &config.bot_token,
            user_id,  // chat_id == user_id for DMs
            "✅ You've been approved! You can now chat with me. Send any message to get started.",
            None,
        ).await;
    }

    Ok(())
}

/// Deny a pending pairing request. Removes from pending.
/// Sends a rejection message to the user on Telegram.
pub async fn deny_user(app_handle: &tauri::AppHandle, user_id: i64) -> EngineResult<()> {
    let config = load_telegram_config(app_handle)?;

    // Send rejection before removing
    if !config.bot_token.is_empty() {
        let client = reqwest::Client::new();
        let _ = tg_send_message(
            &client,
            &config.bot_token,
            user_id,
            "❌ Your pairing request was denied.",
            None,
        ).await;
    }

    let mut config = config;
    config.pending_users.retain(|p| p.user_id != user_id);
    save_telegram_config(app_handle, &config)?;
    info!("[telegram] User {} denied", user_id);
    Ok(())
}

/// Remove a user from the allowlist.
pub fn remove_user(app_handle: &tauri::AppHandle, user_id: i64) -> EngineResult<()> {
    let mut config = load_telegram_config(app_handle)?;
    config.allowed_users.retain(|&id| id != user_id);
    save_telegram_config(app_handle, &config)?;
    info!("[telegram] User {} removed from allowlist", user_id);
    Ok(())
}
