// Paw Agent Engine — Shared Channel Bridge Helpers
//
// Common infrastructure that ALL channel bridges (Telegram, Discord, IRC, Slack,
// Matrix, Nostr, Twitch, Mattermost, Nextcloud Talk) share:
//   - run_channel_agent()  — routes a message through the agent loop, returns text
//   - ChannelConfig trait  — common config shape for load/save/user management
//   - split_message()      — splits long responses for platform message limits
//   - Access control       — allowlist / pairing logic

use crate::engine::types::*;
use crate::engine::providers::AnyProvider;
use crate::engine::agent_loop;
use crate::engine::skills;
use crate::engine::memory;
use crate::engine::injection;
use crate::commands::state::{EngineState, PendingApprovals, normalize_model_name, resolve_provider_for_model};
use log::{info, warn, error};
use serde::{Deserialize, Serialize};
use tauri::Manager;

// ── Common Channel Config ──────────────────────────────────────────────

/// Every channel bridge stores its config under a unique DB key (e.g. "discord_config").
/// This helper pair handles load/save for any Serialize+Deserialize config type.
pub fn load_channel_config<T: for<'de> Deserialize<'de> + Default>(
    app_handle: &tauri::AppHandle,
    config_key: &str,
) -> Result<T, String> {
    let engine_state = app_handle.try_state::<EngineState>()
        .ok_or("Engine not initialized")?;

    match engine_state.store.get_config(config_key) {
        Ok(Some(json)) => {
            serde_json::from_str::<T>(&json)
                .map_err(|e| format!("Parse {} config: {}", config_key, e))
        }
        _ => Ok(T::default()),
    }
}

pub fn save_channel_config<T: Serialize>(
    app_handle: &tauri::AppHandle,
    config_key: &str,
    config: &T,
) -> Result<(), String> {
    let engine_state = app_handle.try_state::<EngineState>()
        .ok_or("Engine not initialized")?;

    let json = serde_json::to_string(config)
        .map_err(|e| format!("Serialize {} config: {}", config_key, e))?;

    engine_state.store.set_config(config_key, &json)?;
    Ok(())
}

// ── Shared Pairing Struct ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingUser {
    pub user_id: String,
    pub username: String,
    pub display_name: String,
    pub requested_at: String,
}

// ── Channel Status (generic) ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelStatus {
    pub running: bool,
    pub connected: bool,
    pub bot_name: Option<String>,
    pub bot_id: Option<String>,
    pub message_count: u64,
    pub allowed_users: Vec<String>,
    pub pending_users: Vec<PendingUser>,
    pub dm_policy: String,
}

// ── Agent Routing ──────────────────────────────────────────────────────

/// Run a user message through the agent loop and return the text response.
/// This is the shared core that every channel bridge calls after receiving a message.
///
/// - `channel_prefix`: e.g. "discord", "irc" — used for session IDs ("eng-discord-{user_id}")
/// - `channel_context`: extra system prompt text (e.g. "User is on Discord. Keep replies concise.")
/// - `message`:      the user's message text
/// - `user_id`:      unique user identifier (platform-specific)
/// - `agent_id`:     which agent config to use ("default" if unset)
pub async fn run_channel_agent(
    app_handle: &tauri::AppHandle,
    channel_prefix: &str,
    channel_context: &str,
    message: &str,
    user_id: &str,
    agent_id: &str,
) -> Result<String, String> {
    let engine_state = app_handle.try_state::<EngineState>()
        .ok_or("Engine not initialized")?;

    // ── Prompt injection scan ──────────────────────────────────────
    let scan = injection::scan_for_injection(message);
    if scan.is_injection {
        injection::log_injection_detected(channel_prefix, user_id, &scan);
        // Block critical injections
        if scan.severity == Some(injection::InjectionSeverity::Critical) {
            warn!("[{}] Blocked critical injection from user {}", channel_prefix, user_id);
            return Ok("⚠️ Your message was blocked by the security scanner. If this is a mistake, please rephrase.".into());
        }
    }

    // Per-user per-agent session: eng-{channel}-{agent}-{user_id}
    let session_id = format!("eng-{}-{}-{}", channel_prefix, agent_id, user_id);

    // Get provider config — use model_routing.resolve() so the worker_model
    // setting is respected for chat bridges (instead of burning the expensive
    // default_model on every message)
    let (provider_config, model, system_prompt, max_rounds, tool_timeout) = {
        let cfg = engine_state.config.lock().map_err(|e| format!("Lock: {}", e))?;

        let default_model = cfg.default_model.clone().unwrap_or_else(|| "gpt-4o".into());
        let model = normalize_model_name(
            &cfg.model_routing.resolve(agent_id, "worker", "", &default_model)
        ).to_string();
        let provider = resolve_provider_for_model(&model, &cfg.providers)
            .or_else(|| {
                cfg.default_provider.as_ref()
                    .and_then(|dp| cfg.providers.iter().find(|p| p.id == *dp).cloned())
            })
            .or_else(|| cfg.providers.first().cloned())
            .ok_or("No AI provider configured")?;

        let sp = cfg.default_system_prompt.clone();
        info!("[{}] Resolved model for agent '{}': {} (default: {})", channel_prefix, agent_id, model, default_model);
        (provider, model, sp, cfg.max_tool_rounds, cfg.tool_timeout_secs)
    };

    // Ensure session exists
    let session_exists = engine_state.store.get_session(&session_id)
        .map(|opt| opt.is_some())
        .unwrap_or(false);
    if !session_exists {
        engine_state.store.create_session(&session_id, &model, system_prompt.as_deref(), Some(agent_id))?;
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
        // Channel-specific context
        parts.push(channel_context.to_string());
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

    // Build tools (with HIL disabled — auto-approve for channel bridges)
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

    // Auto-approve all tool calls (no HIL — user is on a remote chat platform)
    let approvals: PendingApprovals = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
    let approvals_clone = approvals.clone();
    let channel_prefix_owned = channel_prefix.to_string();
    let auto_approver = tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let mut map = approvals_clone.lock().unwrap();
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                if let Some(sender) = map.remove(&key) {
                    info!("[{}] Auto-approving tool call: {}", channel_prefix_owned, key);
                    let _ = sender.send(true);
                }
            }
        }
    });

    let pre_loop_msg_count = messages.len();

    // Get daily budget config
    let daily_budget = {
        let cfg = engine_state.config.lock().map_err(|e| format!("Lock: {}", e))?;
        cfg.daily_budget_usd
    };
    let daily_tokens_tracker = engine_state.daily_tokens.clone();

    // Run the agent loop
    let result = agent_loop::run_agent_turn(
        app_handle,
        &provider,
        &model,
        &mut messages,
        &tools,
        &session_id,
        &run_id,
        max_rounds,
        None,
        &approvals,
        tool_timeout,
        agent_id,
        daily_budget,
        Some(&daily_tokens_tracker),
    ).await;

    // Stop the auto-approver
    auto_approver.abort();

    // Store new messages from the agent turn
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
                error!("[{}] Failed to store message: {}", channel_prefix, e);
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

// ── Utility ────────────────────────────────────────────────────────────

/// Split a long message into chunks at a given limit, preferring newline/space breaks.
pub fn split_message(text: &str, max_len: usize) -> Vec<String> {
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
        let split_at = remaining[..max_len]
            .rfind('\n')
            .or_else(|| remaining[..max_len].rfind(' '))
            .unwrap_or(max_len);
        chunks.push(remaining[..split_at].to_string());
        remaining = remaining[split_at..].trim_start();
    }
    chunks
}

/// Check access control. Returns Ok(()) if allowed, Err(denial message) if denied.
/// Also handles adding pending pairing requests.
pub fn check_access(
    dm_policy: &str,
    user_id: &str,
    username: &str,
    display_name: &str,
    allowed_users: &[String],
    pending_users: &mut Vec<PendingUser>,
) -> Result<(), String> {
    match dm_policy {
        "allowlist" => {
            if !allowed_users.contains(&user_id.to_string()) {
                return Err("⛔ You're not on the allowlist. Ask the Paw owner to add you.".into());
            }
        }
        "pairing" => {
            if !allowed_users.contains(&user_id.to_string()) {
                if !pending_users.iter().any(|p| p.user_id == user_id) {
                    pending_users.push(PendingUser {
                        user_id: user_id.to_string(),
                        username: username.to_string(),
                        display_name: display_name.to_string(),
                        requested_at: chrono::Utc::now().to_rfc3339(),
                    });
                }
                return Err("🔒 Pairing request sent to Paw. Waiting for approval...".into());
            }
        }
        // "open" — allow everyone
        _ => {}
    }
    Ok(())
}

/// Generic approve/deny/remove user helpers for any channel config.
pub fn approve_user_generic(
    app_handle: &tauri::AppHandle,
    config_key: &str,
    user_id: &str,
) -> Result<(), String>
where
{
    // Load raw config as Value, modify, save
    let engine_state = app_handle.try_state::<EngineState>()
        .ok_or("Engine not initialized")?;
    let json_str = engine_state.store.get_config(config_key)
        .map_err(|e| format!("Load config: {}", e))?
        .unwrap_or_else(|| "{}".into());
    let mut val: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| format!("Parse config: {}", e))?;

    // Add to allowed_users
    if let Some(arr) = val.get_mut("allowed_users").and_then(|v| v.as_array_mut()) {
        let uid_val = serde_json::Value::String(user_id.to_string());
        if !arr.contains(&uid_val) {
            arr.push(uid_val);
        }
    }
    // Remove from pending_users
    if let Some(arr) = val.get_mut("pending_users").and_then(|v| v.as_array_mut()) {
        arr.retain(|p| p.get("user_id").and_then(|v| v.as_str()) != Some(user_id));
    }

    let new_json = serde_json::to_string(&val).map_err(|e| format!("Serialize: {}", e))?;
    engine_state.store.set_config(config_key, &new_json)?;
    info!("[{}] User {} approved", config_key, user_id);
    Ok(())
}

pub fn deny_user_generic(
    app_handle: &tauri::AppHandle,
    config_key: &str,
    user_id: &str,
) -> Result<(), String> {
    let engine_state = app_handle.try_state::<EngineState>()
        .ok_or("Engine not initialized")?;
    let json_str = engine_state.store.get_config(config_key)
        .map_err(|e| format!("Load config: {}", e))?
        .unwrap_or_else(|| "{}".into());
    let mut val: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| format!("Parse config: {}", e))?;

    if let Some(arr) = val.get_mut("pending_users").and_then(|v| v.as_array_mut()) {
        arr.retain(|p| p.get("user_id").and_then(|v| v.as_str()) != Some(user_id));
    }

    let new_json = serde_json::to_string(&val).map_err(|e| format!("Serialize: {}", e))?;
    engine_state.store.set_config(config_key, &new_json)?;
    info!("[{}] User {} denied", config_key, user_id);
    Ok(())
}

pub fn remove_user_generic(
    app_handle: &tauri::AppHandle,
    config_key: &str,
    user_id: &str,
) -> Result<(), String> {
    let engine_state = app_handle.try_state::<EngineState>()
        .ok_or("Engine not initialized")?;
    let json_str = engine_state.store.get_config(config_key)
        .map_err(|e| format!("Load config: {}", e))?
        .unwrap_or_else(|| "{}".into());
    let mut val: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| format!("Parse config: {}", e))?;

    if let Some(arr) = val.get_mut("allowed_users").and_then(|v| v.as_array_mut()) {
        arr.retain(|v| v.as_str() != Some(user_id));
    }

    let new_json = serde_json::to_string(&val).map_err(|e| format!("Serialize: {}", e))?;
    engine_state.store.set_config(config_key, &new_json)?;
    info!("[{}] User {} removed", config_key, user_id);
    Ok(())
}

// ── Routed Channel Agent ───────────────────────────────────────────────

/// Convenience wrapper: resolve routing config to determine the agent_id,
/// then call run_channel_agent with that agent. Channels should prefer this
/// over calling run_channel_agent directly.
pub async fn run_routed_channel_agent(
    app_handle: &tauri::AppHandle,
    channel_prefix: &str,
    channel_context: &str,
    message: &str,
    user_id: &str,
    channel_id: Option<&str>,
) -> Result<String, String> {
    // Load routing config and resolve agent
    let engine_state = app_handle.try_state::<EngineState>()
        .ok_or("Engine not initialized")?;

    let routing_config = crate::engine::routing::load_routing_config(
        &std::sync::Arc::new(crate::engine::sessions::SessionStore::open()?)
    );

    let route = crate::engine::routing::resolve_route(
        &routing_config,
        channel_prefix,
        user_id,
        channel_id,
    );

    if route.matched_rule_id.is_some() {
        info!(
            "[{}] Routed user {} → agent '{}' (rule: {})",
            channel_prefix, user_id, route.agent_id,
            route.matched_rule_label.as_deref().unwrap_or("?")
        );
    }

    run_channel_agent(
        app_handle,
        channel_prefix,
        channel_context,
        message,
        user_id,
        &route.agent_id,
    ).await
}
