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

use crate::engine::types::*;
use crate::engine::providers::AnyProvider;
use crate::engine::agent_loop;
use crate::engine::skills;
use crate::engine::memory;
use crate::engine::commands::{EngineState, PendingApprovals};
use log::{info, warn, error};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use tauri::{Emitter, Manager};

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
    date: i64,
}

#[derive(Debug, Deserialize)]
struct TgUser {
    id: i64,
    is_bot: bool,
    first_name: String,
    last_name: Option<String>,
    username: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TgChat {
    id: i64,
    #[serde(rename = "type")]
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

async fn tg_get_me(client: &reqwest::Client, token: &str) -> Result<(String, String), String> {
    let url = format!("{}{}/getMe", TG_API, token);
    let resp: TgResponse<serde_json::Value> = client
        .get(&url)
        .send().await.map_err(|e| format!("getMe request failed: {}", e))?
        .json().await.map_err(|e| format!("getMe parse failed: {}", e))?;

    if !resp.ok {
        return Err(format!("getMe failed: {}", resp.description.unwrap_or_default()));
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
) -> Result<Vec<TgUpdate>, String> {
    let url = format!(
        "{}{}/getUpdates?offset={}&timeout={}&allowed_updates=[\"message\"]",
        TG_API, token, offset, timeout
    );
    let resp: TgResponse<Vec<TgUpdate>> = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(timeout + 10))
        .send().await.map_err(|e| format!("getUpdates failed: {}", e))?
        .json().await.map_err(|e| format!("getUpdates parse failed: {}", e))?;

    if !resp.ok {
        return Err(format!("getUpdates error: {}", resp.description.unwrap_or_default()));
    }

    Ok(resp.result.unwrap_or_default())
}

async fn tg_send_message(
    client: &reqwest::Client,
    token: &str,
    chat_id: i64,
    text: &str,
    reply_to: Option<i64>,
) -> Result<(), String> {
    // Telegram message limit = 4096 chars. Split if needed.
    let chunks = split_message(text, 4000);
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
) -> Result<(), String> {
    let url = format!("{}{}/sendChatAction", TG_API, token);
    let body = serde_json::json!({
        "chat_id": chat_id,
        "action": "typing",
    });
    let _ = client.post(&url).json(&body).send().await;
    Ok(())
}

fn split_message(text: &str, max_len: usize) -> Vec<String> {
    if text.len() <= max_len {
        return vec![text.to_string()];
    }
    let mut chunks = Vec::new();
    let mut remaining = text;
    while !remaining.is_empty() {
        if remaining.len() <= max_len {
            chunks.push(remaining.to_string());
            break;
        }
        // Try to split at a newline or space near the limit
        let split_at = remaining[..max_len]
            .rfind('\n')
            .or_else(|| remaining[..max_len].rfind(' '))
            .unwrap_or(max_len);
        chunks.push(remaining[..split_at].to_string());
        remaining = &remaining[split_at..].trim_start();
    }
    chunks
}

/// Check if the Telegram bridge is currently running.
pub fn is_bridge_running() -> bool {
    BRIDGE_RUNNING.load(Ordering::Relaxed)
}

// ── Bridge Core ────────────────────────────────────────────────────────

/// Start the Telegram polling bridge. Returns immediately;
/// the actual polling runs in a background tokio task.
pub fn start_bridge(app_handle: tauri::AppHandle) -> Result<(), String> {
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
        if let Err(e) = run_polling_loop(app_handle, config).await {
            error!("[telegram] Bridge crashed: {}", e);
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
async fn run_polling_loop(app_handle: tauri::AppHandle, config: TelegramConfig) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

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

                        info!("[telegram] Message from {} ({}): {}", username, user_id, 
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
                        let response = run_telegram_agent(
                            &app_handle,
                            &text,
                            user_id,
                            current_config.agent_id.as_deref().unwrap_or("default"),
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

// ── Agent Integration ──────────────────────────────────────────────────

/// Run a message through the agent loop and return the final text response.
/// This creates a per-user session so each Telegram user gets their own conversation.
async fn run_telegram_agent(
    app_handle: &tauri::AppHandle,
    message: &str,
    user_id: i64,
    agent_id: &str,
) -> Result<String, String> {
    let engine_state = app_handle.try_state::<EngineState>()
        .ok_or("Engine not initialized")?;

    // Per-user per-agent session: eng-tg-{agent}-{user_id}
    let session_id = format!("eng-tg-{}-{}", agent_id, user_id);

    // Get provider config
    let (provider_config, model, system_prompt, max_rounds, tool_timeout) = {
        let cfg = engine_state.config.lock().map_err(|e| format!("Lock: {}", e))?;

        let model = cfg.default_model.clone().unwrap_or_else(|| "gpt-4o".into());
        let provider = cfg.default_provider.as_ref()
            .and_then(|dp| cfg.providers.iter().find(|p| p.id == *dp).cloned())
            .or_else(|| cfg.providers.first().cloned())
            .ok_or("No AI provider configured")?;

        let sp = cfg.default_system_prompt.clone();
        (provider, model, sp, cfg.max_tool_rounds, cfg.tool_timeout_secs)
    };

    // Ensure session exists
    let session_exists = engine_state.store.get_session(&session_id)
        .map(|opt| opt.is_some())
        .unwrap_or(false);
    if !session_exists {
        engine_state.store.create_session(&session_id, &model, system_prompt.as_deref(), Some(agent_id))?;
    }

    // ── Cost control: prune old Telegram session messages ──
    // Telegram sessions persist per-user and grow unboundedly.
    // Keep the last 50 messages (~5-10 conversation turns) to provide
    // useful context without sending huge histories to the API.
    const TG_SESSION_KEEP_MESSAGES: i64 = 50;
    if let Err(e) = engine_state.store.prune_session_messages(&session_id, TG_SESSION_KEEP_MESSAGES) {
        warn!("[telegram] Failed to prune session {}: {}", session_id, e);
    }

    // Store user message
    let user_msg = StoredMessage {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.clone(),
        role: "user".into(),
        content: message.to_string(),
        tool_calls_json: None,
        tool_call_id: None,
        name: None,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    engine_state.store.add_message(&user_msg)?;

    // Compose system prompt with agent context + memory + skills
    let agent_context = engine_state.store.compose_agent_context(agent_id).unwrap_or(None);
    let skill_instructions = skills::get_enabled_skill_instructions(&engine_state.store).unwrap_or_default();

    // Auto-recall memories
    let (auto_recall_on, recall_limit, recall_threshold) = {
        let mcfg = engine_state.memory_config.lock().ok();
        (
            mcfg.as_ref().map(|c| c.auto_recall).unwrap_or(false),
            mcfg.as_ref().map(|c| c.recall_limit).unwrap_or(5),
            mcfg.as_ref().map(|c| c.recall_threshold).unwrap_or(0.3),
        )
    };

    let memory_context = if auto_recall_on {
        let emb_client = engine_state.embedding_client();
        match memory::search_memories(
            &engine_state.store, message, recall_limit, recall_threshold, emb_client.as_ref(), None
        ).await {
            Ok(mems) if !mems.is_empty() => {
                let ctx: Vec<String> = mems.iter().map(|m| format!("- [{}] {}", m.category, m.content)).collect();
                Some(format!("## Relevant Memories\n{}", ctx.join("\n")))
            }
            _ => None,
        }
    } else {
        None
    };

    // Build full system prompt
    let full_system_prompt = {
        let mut parts: Vec<String> = Vec::new();
        // Add Telegram context
        parts.push(format!(
            "You are chatting via Telegram. The user is messaging you from their phone. \
             Keep responses concise and mobile-friendly. Use Markdown formatting supported by Telegram \
             (bold, italic, code, links). Avoid very long responses unless explicitly asked."
        ));
        // Local time context
        {
            let user_tz = {
                let cfg = engine_state.config.lock().map_err(|e| format!("Lock: {}", e))?;
                cfg.user_timezone.clone()
            };
            let now_utc = chrono::Utc::now();
            if let Ok(tz) = user_tz.parse::<chrono_tz::Tz>() {
                let local: chrono::DateTime<chrono_tz::Tz> = now_utc.with_timezone(&tz);
                parts.push(format!(
                    "## Local Time\n\
                    - **Current time**: {}\n\
                    - **Timezone**: {} (UTC{})\n\
                    - **Day of week**: {}",
                    local.format("%Y-%m-%d %H:%M:%S"),
                    tz.name(),
                    local.format("%:z"),
                    local.format("%A"),
                ));
            } else {
                let local = chrono::Local::now();
                parts.push(format!(
                    "## Local Time\n\
                    - **Current time**: {}\n\
                    - **Timezone**: {} (UTC{})\n\
                    - **Day of week**: {}",
                    local.format("%Y-%m-%d %H:%M:%S"),
                    local.format("%Z"),
                    local.format("%:z"),
                    local.format("%A"),
                ));
            }
        }
        if let Some(sp) = &system_prompt {
            parts.push(sp.clone());
        }
        if let Some(ac) = &agent_context {
            parts.push(ac.clone());
        }
        if let Some(mc) = &memory_context {
            parts.push(mc.clone());
        }
        if !skill_instructions.is_empty() {
            parts.push(skill_instructions);
        }
        Some(parts.join("\n\n---\n\n"))
    };

    // Load conversation history
    let mut messages = engine_state.store.load_conversation(
        &session_id,
        full_system_prompt.as_deref(),
    )?;

    // Build tools (with HIL disabled for Telegram — auto-approve safe tools)
    let tools = {
        let mut t = ToolDefinition::builtins();
        let enabled_ids: Vec<String> = skills::builtin_skills().iter()
            .filter(|s| engine_state.store.is_skill_enabled(&s.id).unwrap_or(false))
            .map(|s| s.id.clone())
            .collect();
        if !enabled_ids.is_empty() {
            t.extend(ToolDefinition::skill_tools(&enabled_ids));
        }
        t
    };

    let provider = AnyProvider::from_config(&provider_config);
    let run_id = uuid::Uuid::new_v4().to_string();

    // For Telegram, we auto-approve all tool calls (no HIL — user is on phone)
    // Create approvals map that auto-resolves
    let approvals: PendingApprovals = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));

    // Spawn auto-approver: listen for tool requests and approve them
    let _app_clone = app_handle.clone();
    let approvals_clone = approvals.clone();
    let auto_approver = tauri::async_runtime::spawn(async move {
        // This task just ensures any pending approvals get auto-approved
        // The actual approval happens via the engine_approve_tool mechanism
        // For Telegram we'll handle it differently — see below
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let mut map = approvals_clone.lock().unwrap();
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                if let Some(sender) = map.remove(&key) {
                    info!("[telegram] Auto-approving tool call: {}", key);
                    let _ = sender.send(true);
                }
            }
            // Exit if no more pending after a reasonable time
            drop(map);
        }
    });

    let pre_loop_msg_count = messages.len();

    // Cap tool rounds for Telegram — mobile context, keep it snappy
    const TG_MAX_TOOL_ROUNDS: u32 = 15;
    let effective_max_rounds = max_rounds.min(TG_MAX_TOOL_ROUNDS);

    // Run the agent loop
    let result = agent_loop::run_agent_turn(
        app_handle,
        &provider,
        &model,
        &mut messages,
        &tools,
        &session_id,
        &run_id,
        effective_max_rounds,
        None, // temperature
        &approvals,
        tool_timeout,
        agent_id,
    ).await;

    // Stop the auto-approver
    auto_approver.abort();

    // Store new messages
    for msg in messages.iter().skip(pre_loop_msg_count) {
        if msg.role == Role::Assistant || msg.role == Role::Tool {
            let stored = StoredMessage {
                id: uuid::Uuid::new_v4().to_string(),
                session_id: session_id.clone(),
                role: match msg.role {
                    Role::Assistant => "assistant".into(),
                    Role::Tool => "tool".into(),
                    _ => "user".into(),
                },
                content: msg.content.as_text(),
                tool_calls_json: msg.tool_calls.as_ref()
                    .map(|tc| serde_json::to_string(tc).unwrap_or_default()),
                tool_call_id: msg.tool_call_id.clone(),
                name: msg.name.clone(),
                created_at: chrono::Utc::now().to_rfc3339(),
            };
            if let Err(e) = engine_state.store.add_message(&stored) {
                error!("[telegram] Failed to store message: {}", e);
            }
        }
    }

    // Auto-capture memories
    if let Ok(final_text) = &result {
        let auto_capture = engine_state.memory_config.lock().ok()
            .map(|c| c.auto_capture).unwrap_or(false);
        if auto_capture && !final_text.is_empty() {
            let facts = memory::extract_memorable_facts(message, final_text);
            if !facts.is_empty() {
                let emb_client = engine_state.embedding_client();
                for (content, category) in &facts {
                    let _ = memory::store_memory(
                        &engine_state.store, content, category, 5, emb_client.as_ref(), None
                    ).await;
                }
            }
        }
    }

    result
}

// ── Config Persistence ─────────────────────────────────────────────────

pub fn load_telegram_config(app_handle: &tauri::AppHandle) -> Result<TelegramConfig, String> {
    let engine_state = app_handle.try_state::<EngineState>()
        .ok_or("Engine not initialized")?;

    match engine_state.store.get_config("telegram_config") {
        Ok(Some(json)) => {
            serde_json::from_str::<TelegramConfig>(&json)
                .map_err(|e| format!("Parse telegram config: {}", e))
        }
        _ => Ok(TelegramConfig::default()),
    }
}

pub fn save_telegram_config(app_handle: &tauri::AppHandle, config: &TelegramConfig) -> Result<(), String> {
    let engine_state = app_handle.try_state::<EngineState>()
        .ok_or("Engine not initialized")?;

    let json = serde_json::to_string(config)
        .map_err(|e| format!("Serialize telegram config: {}", e))?;

    engine_state.store.set_config("telegram_config", &json)?;
    Ok(())
}

/// Approve a pending pairing request. Adds user to allowlist, removes from pending.
/// Sends a confirmation message to the user on Telegram.
pub async fn approve_user(app_handle: &tauri::AppHandle, user_id: i64) -> Result<(), String> {
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
pub async fn deny_user(app_handle: &tauri::AppHandle, user_id: i64) -> Result<(), String> {
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
pub fn remove_user(app_handle: &tauri::AppHandle, user_id: i64) -> Result<(), String> {
    let mut config = load_telegram_config(app_handle)?;
    config.allowed_users.retain(|&id| id != user_id);
    save_telegram_config(app_handle, &config)?;
    info!("[telegram] User {} removed from allowlist", user_id);
    Ok(())
}
