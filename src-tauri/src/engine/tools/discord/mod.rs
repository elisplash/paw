// Paw Agent Engine — Discord Tools (Atomic Module)
//
// Full Discord server management via the REST API.
// Each sub-module handles one domain:
//
//   channels  — list, setup (idempotent create), delete, edit
//   messages  — send, edit, delete/purge, history, pin/unpin, reactions
//   roles     — list, create, delete, assign to member, remove from member
//   members   — list, get info, kick, ban, unban
//   server    — get server info, create invite
//
// Shared helpers (token resolution, API client, rate-limit retry) live here.

pub mod channels;
pub mod messages;
pub mod roles;
pub mod members;
pub mod server;

use crate::atoms::types::*;
use crate::atoms::error::EngineResult;
use crate::engine::state::EngineState;
use log::warn;
use serde_json::Value;
use tauri::Manager;
use std::time::Duration;

pub(crate) const DISCORD_API: &str = "https://discord.com/api/v10";

// ── Public API (called by tools/mod.rs) ────────────────────────────────

/// All Discord tool definitions across sub-modules.
pub fn definitions() -> Vec<ToolDefinition> {
    let mut defs = Vec::new();
    defs.extend(channels::definitions());
    defs.extend(messages::definitions());
    defs.extend(roles::definitions());
    defs.extend(members::definitions());
    defs.extend(server::definitions());
    defs
}

/// Route a tool call to the correct sub-module executor.
pub async fn execute(
    name: &str,
    args: &Value,
    app_handle: &tauri::AppHandle,
) -> Option<Result<String, String>> {
    // Try each sub-module — first Some wins
    None
        .or(channels::execute(name, args, app_handle).await)
        .or(messages::execute(name, args, app_handle).await)
        .or(roles::execute(name, args, app_handle).await)
        .or(members::execute(name, args, app_handle).await)
        .or(server::execute(name, args, app_handle).await)
}

// ── Shared helpers ─────────────────────────────────────────────────────

/// Resolve the Discord bot token from the skill vault.
pub(crate) fn get_bot_token(app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;
    let creds = crate::engine::skills::get_skill_credentials(&state.store, "discord")
        .map_err(|e| format!("Failed to get Discord credentials: {}", e))?;
    let token = creds.get("DISCORD_BOT_TOKEN")
        .cloned()
        .ok_or("DISCORD_BOT_TOKEN not found in skill vault. Enable the Discord skill and add your bot token in Settings → Skills → Discord.")?;
    if token.is_empty() {
        return Err("Discord bot token is empty".into());
    }
    Ok(token)
}

/// Resolve the server (guild) ID from args or credential fallback.
pub(crate) fn resolve_server_id(args: &Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    if let Some(sid) = args["server_id"].as_str() {
        if !sid.is_empty() {
            return Ok(sid.to_string());
        }
    }
    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;
    let creds = crate::engine::skills::get_skill_credentials(&state.store, "discord")
        .map_err(|e| format!("Failed to get Discord credentials: {}", e))?;
    creds.get("DISCORD_SERVER_ID")
        .filter(|s| !s.is_empty())
        .cloned()
        .ok_or("No server_id provided and DISCORD_SERVER_ID not set in skill credentials.".into())
}

/// Resolve the default channel ID from args or credential fallback.
pub(crate) fn resolve_channel_id(args: &Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    if let Some(cid) = args["channel_id"].as_str() {
        if !cid.is_empty() {
            return Ok(cid.to_string());
        }
    }
    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;
    let creds = crate::engine::skills::get_skill_credentials(&state.store, "discord")
        .map_err(|e| format!("Failed to get Discord credentials: {}", e))?;
    creds.get("DISCORD_DEFAULT_CHANNEL")
        .filter(|s| !s.is_empty())
        .cloned()
        .ok_or("No channel_id provided and DISCORD_DEFAULT_CHANNEL not set in skill credentials.".into())
}

/// Build a reqwest client with the bot Authorization header.
pub(crate) fn authorized_client(token: &str) -> (reqwest::Client, String) {
    let client = reqwest::Client::new();
    let auth = format!("Bot {}", token);
    (client, auth)
}

/// Make a Discord API request with automatic rate-limit retry (once).
pub(crate) async fn discord_request(
    client: &reqwest::Client,
    method: reqwest::Method,
    url: &str,
    auth: &str,
    body: Option<&Value>,
) -> EngineResult<Value> {
    let mut req = client.request(method.clone(), url)
        .header("Authorization", auth)
        .header("Content-Type", "application/json");
    if let Some(b) = body {
        req = req.json(b);
    }

    let resp = req.send().await.map_err(|e| format!("HTTP error: {}", e))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();

    if status.as_u16() == 429 {
        // Rate limited — parse retry_after and wait once
        let retry_after = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v["retry_after"].as_f64())
            .unwrap_or(1.0);
        warn!("[discord] Rate limited, waiting {:.1}s", retry_after);
        tokio::time::sleep(Duration::from_secs_f64(retry_after + 0.1)).await;

        let mut req2 = client.request(method, url)
            .header("Authorization", auth)
            .header("Content-Type", "application/json");
        if let Some(b) = body {
            req2 = req2.json(b);
        }
        let resp2 = req2.send().await.map_err(|e| format!("Retry HTTP error: {}", e))?;
        let status2 = resp2.status();
        let text2 = resp2.text().await.unwrap_or_default();
        if !status2.is_success() {
            return Err(format!("Discord API {} (after retry): {}", status2, &text2[..text2.len().min(300)]).into());
        }
        return serde_json::from_str(&text2)
            .or_else(|_| Ok(Value::String(text2)));
    }

    if status.as_u16() == 204 {
        // No content (success for DELETE etc.)
        return Ok(json!({"ok": true}));
    }

    if !status.is_success() {
        return Err(format!("Discord API {}: {}", status, &text[..text.len().min(300)]).into());
    }

    serde_json::from_str(&text)
        .or_else(|_| Ok(Value::String(text)))
}

use serde_json::json;
