// Paw Agent Engine — Discord management tools
//
// Provides direct Discord server management via the REST API:
//   - discord_setup_channels:  Bulk create categories + channels in one call
//   - discord_list_channels:   List all channels/categories in a server
//   - discord_send_message:    Send a message to a specific channel
//
// All tools auto-resolve the bot token from the skill vault — the agent
// never needs to handle raw tokens. For any other Discord API call, the
// agent can use `fetch` which also auto-injects the bot Authorization header.

use crate::atoms::types::*;
use crate::atoms::error::EngineResult;
use crate::engine::state::EngineState;
use log::{info, warn};
use serde_json::{json, Value};
use tauri::Manager;
use std::time::Duration;

const DISCORD_API: &str = "https://discord.com/api/v10";

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        // ── discord_setup_channels ─────────────────────────────────────
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "discord_setup_channels".into(),
                description: "Create categories and channels in a Discord server. \
                    ONLY for creating/organizing server structure — NOT for sending messages (use discord_send_message for that). \
                    Idempotent: skips categories/channels that already exist (by name). \
                    Channels default to text (type 0). Set type to 2 for voice channels.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "server_id": {
                            "type": "string",
                            "description": "The Discord server (guild) ID. Get from discord_list_channels or skill credentials."
                        },
                        "categories": {
                            "type": "array",
                            "description": "Array of categories to create, each with channels",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "name": {
                                        "type": "string",
                                        "description": "Category name (e.g. 'Welcome & Info')"
                                    },
                                    "channels": {
                                        "type": "array",
                                        "description": "Channels to create inside this category",
                                        "items": {
                                            "type": "object",
                                            "properties": {
                                                "name": {
                                                    "type": "string",
                                                    "description": "Channel name (e.g. 'general-chat')"
                                                },
                                                "type": {
                                                    "type": "integer",
                                                    "description": "Channel type: 0=text (default), 2=voice"
                                                },
                                                "topic": {
                                                    "type": "string",
                                                    "description": "Channel topic/description"
                                                }
                                            },
                                            "required": ["name"]
                                        }
                                    }
                                },
                                "required": ["name", "channels"]
                            }
                        }
                    },
                    "required": ["server_id", "categories"]
                }),
            },
        },
        // ── discord_list_channels ──────────────────────────────────────
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "discord_list_channels".into(),
                description: "List all channels and categories in a Discord server. \
                    Returns channel names, IDs, types, and parent categories. \
                    Use this FIRST to see the current server structure before making changes. \
                    If no server_id is provided, uses the DISCORD_SERVER_ID from skill credentials.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "server_id": {
                            "type": "string",
                            "description": "The Discord server (guild) ID. Optional if DISCORD_SERVER_ID is set in credentials."
                        }
                    }
                }),
            },
        },
        // ── discord_send_message ───────────────────────────────────────
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "discord_send_message".into(),
                description: "Send a message to a Discord channel. \
                    Supports plain text and basic Discord markdown (bold, italic, code blocks, embeds). \
                    If no channel_id is provided, uses the DISCORD_DEFAULT_CHANNEL from skill credentials.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "channel_id": {
                            "type": "string",
                            "description": "The target channel ID. Optional if DISCORD_DEFAULT_CHANNEL is set."
                        },
                        "content": {
                            "type": "string",
                            "description": "The message text to send (max 2000 chars). Supports Discord markdown."
                        },
                        "embed": {
                            "type": "object",
                            "description": "Optional rich embed object with title, description, color, fields, etc.",
                            "properties": {
                                "title": { "type": "string" },
                                "description": { "type": "string" },
                                "color": { "type": "integer", "description": "Decimal color value (e.g. 5814783 for blue)" },
                                "fields": {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "name": { "type": "string" },
                                            "value": { "type": "string" },
                                            "inline": { "type": "boolean" }
                                        },
                                        "required": ["name", "value"]
                                    }
                                }
                            }
                        }
                    },
                    "required": ["content"]
                }),
            },
        },
        // ── discord_delete_channels ────────────────────────────────────
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "discord_delete_channels".into(),
                description: "Delete one or more Discord channels or categories by ID. \
                    Use discord_list_channels first to get channel IDs. \
                    DESTRUCTIVE — deleted channels cannot be recovered. \
                    Useful for cleaning up duplicates or reorganizing.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "channel_ids": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Array of channel/category IDs to delete. Get IDs from discord_list_channels."
                        }
                    },
                    "required": ["channel_ids"]
                }),
            },
        },
    ]
}

pub async fn execute(
    name: &str,
    args: &Value,
    app_handle: &tauri::AppHandle,
) -> Option<Result<String, String>> {
    match name {
        "discord_setup_channels"  => Some(execute_setup(args, app_handle).await.map_err(|e| e.to_string())),
        "discord_list_channels"   => Some(execute_list(args, app_handle).await.map_err(|e| e.to_string())),
        "discord_send_message"    => Some(execute_send(args, app_handle).await.map_err(|e| e.to_string())),
        "discord_delete_channels" => Some(execute_delete(args, app_handle).await.map_err(|e| e.to_string())),
        _ => None,
    }
}

/// Resolve the Discord bot token from the skill vault.
fn get_bot_token(app_handle: &tauri::AppHandle) -> EngineResult<String> {
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
fn resolve_server_id(args: &Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    if let Some(sid) = args["server_id"].as_str() {
        if !sid.is_empty() {
            return Ok(sid.to_string());
        }
    }
    // Fallback to DISCORD_SERVER_ID from skill credentials
    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;
    let creds = crate::engine::skills::get_skill_credentials(&state.store, "discord")
        .map_err(|e| format!("Failed to get Discord credentials: {}", e))?;
    creds.get("DISCORD_SERVER_ID")
        .filter(|s| !s.is_empty())
        .cloned()
        .ok_or("No server_id provided and DISCORD_SERVER_ID not set in skill credentials. Provide server_id or set it in Settings → Skills → Discord.".into())
}

async fn execute_setup(args: &Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let server_id = resolve_server_id(args, app_handle)?;
    let categories = args["categories"].as_array()
        .ok_or("discord_setup_channels: missing 'categories' array")?;

    let token = get_bot_token(app_handle)?;
    let client = reqwest::Client::new();

    // ── Fetch existing channels for idempotency ────────────────────────
    let existing = fetch_existing_channels(&client, &token, &server_id).await?;
    let existing_cats: std::collections::HashMap<String, String> = existing.iter()
        .filter(|c| c["type"].as_i64() == Some(4))
        .filter_map(|c| {
            let name = c["name"].as_str()?.to_lowercase();
            let id = c["id"].as_str()?.to_string();
            Some((name, id))
        })
        .collect();
    let existing_channels: std::collections::HashSet<(String, Option<String>)> = existing.iter()
        .filter(|c| c["type"].as_i64() != Some(4))
        .filter_map(|c| {
            let name = c["name"].as_str()?.to_lowercase();
            let parent = c["parent_id"].as_str().map(|s| s.to_string());
            Some((name, parent))
        })
        .collect();

    let mut results: Vec<String> = Vec::new();
    let mut created_count = 0;
    let mut skipped_count = 0;
    let mut error_count = 0;

    for category in categories {
        let cat_name = category["name"].as_str().unwrap_or("Unnamed");
        let channels = category["channels"].as_array();

        // ── Check if category already exists ──────────────────────────
        let category_id = if let Some(existing_id) = existing_cats.get(&cat_name.to_lowercase()) {
            results.push(format!("⏭️  Category '{}' already exists ({})", cat_name, existing_id));
            skipped_count += 1;
            existing_id.clone()
        } else {
            info!("[discord_setup] Creating category: {}", cat_name);
            let cat_body = json!({
                "name": cat_name,
                "type": 4
            });
            match create_channel(&client, &token, &server_id, &cat_body).await {
                Ok(id) => {
                    results.push(format!("✅ Category '{}' created ({})", cat_name, id));
                    created_count += 1;
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    id
                }
                Err(e) => {
                    results.push(format!("❌ Category '{}' failed: {}", cat_name, e));
                    error_count += 1;
                    continue;
                }
            }
        };

        // Create channels inside this category
        if let Some(channels) = channels {
            for channel in channels {
                let ch_name = channel["name"].as_str().unwrap_or("unnamed");
                let ch_type = channel["type"].as_i64().unwrap_or(0);
                let ch_topic = channel["topic"].as_str();

                // ── Check if channel already exists under this parent ─
                if existing_channels.contains(&(ch_name.to_lowercase(), Some(category_id.clone()))) {
                    let type_str = if ch_type == 2 { "🔊" } else { "#" };
                    results.push(format!("  ⏭️  {}{} already exists", type_str, ch_name));
                    skipped_count += 1;
                    continue;
                }

                info!("[discord_setup] Creating channel: {} (type={}) in category {}", ch_name, ch_type, cat_name);

                let mut ch_body = json!({
                    "name": ch_name,
                    "type": ch_type,
                    "parent_id": category_id
                });
                if let Some(topic) = ch_topic {
                    ch_body["topic"] = json!(topic);
                }

                match create_channel(&client, &token, &server_id, &ch_body).await {
                    Ok(id) => {
                        let type_str = if ch_type == 2 { "🔊" } else { "#" };
                        results.push(format!("  ✅ {}{} created ({})", type_str, ch_name, id));
                        created_count += 1;
                    }
                    Err(e) => {
                        results.push(format!("  ❌ #{} failed: {}", ch_name, e));
                        error_count += 1;
                    }
                }

                tokio::time::sleep(Duration::from_millis(300)).await;
            }
        }
    }

    let summary = if created_count == 0 && error_count == 0 {
        format!(
            "All {} categories/channels already exist — nothing to create.\n\n{}",
            skipped_count, results.join("\n")
        )
    } else {
        format!(
            "Discord server setup complete!\n\
            Created: {} | Skipped (already exist): {} | Failed: {}\n\n{}",
            created_count, skipped_count, error_count, results.join("\n")
        )
    };

    info!("[discord_setup] Done: {} created, {} skipped, {} errors", created_count, skipped_count, error_count);
    Ok(summary)
}

// ── discord_list_channels ──────────────────────────────────────────────

async fn execute_list(args: &Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let server_id = resolve_server_id(args, app_handle)?;
    let token = get_bot_token(app_handle)?;

    info!("[discord] Listing channels for guild {}", server_id);

    let client = reqwest::Client::new();
    let url = format!("{}/guilds/{}/channels", DISCORD_API, server_id);
    let resp = client.get(&url)
        .header("Authorization", format!("Bot {}", token))
        .send()
        .await
        .map_err(|e| format!("HTTP error: {}", e))?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        return Err(format!("Discord API {}: {}", status, &text[..text.len().min(300)]).into());
    }

    let channels: Vec<Value> = serde_json::from_str(&text)
        .map_err(|e| format!("Failed to parse channels: {}", e))?;

    // Build a structured tree: categories → children
    let mut categories: Vec<&Value> = channels.iter()
        .filter(|c| c["type"].as_i64() == Some(4))
        .collect();
    categories.sort_by_key(|c| c["position"].as_i64().unwrap_or(999));

    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("**Discord Server Channels** (guild: {})\n", server_id));

    // Channels without a parent category
    let mut orphans: Vec<&Value> = channels.iter()
        .filter(|c| c["type"].as_i64() != Some(4) && c["parent_id"].is_null())
        .collect();
    orphans.sort_by_key(|c| c["position"].as_i64().unwrap_or(999));
    if !orphans.is_empty() {
        lines.push("**[No Category]**".to_string());
        for ch in &orphans {
            lines.push(format_channel(ch));
        }
        lines.push(String::new());
    }

    for cat in &categories {
        let cat_id = cat["id"].as_str().unwrap_or("?");
        let cat_name = cat["name"].as_str().unwrap_or("?");
        lines.push(format!("**{}** (id: {})", cat_name.to_uppercase(), cat_id));

        let mut children: Vec<&Value> = channels.iter()
            .filter(|c| c["parent_id"].as_str() == Some(cat_id))
            .collect();
        children.sort_by_key(|c| c["position"].as_i64().unwrap_or(999));

        if children.is_empty() {
            lines.push("  (empty)".to_string());
        } else {
            for ch in &children {
                lines.push(format_channel(ch));
            }
        }
        lines.push(String::new());
    }

    lines.push(format!("Total: {} channels in {} categories", channels.len(), categories.len()));
    Ok(lines.join("\n"))
}

fn format_channel(ch: &Value) -> String {
    let name = ch["name"].as_str().unwrap_or("?");
    let id = ch["id"].as_str().unwrap_or("?");
    let ch_type = ch["type"].as_i64().unwrap_or(0);
    let topic = ch["topic"].as_str().unwrap_or("");
    let icon = match ch_type {
        0  => "#",
        2  => "🔊",
        5  => "📢",
        13 => "🎭",
        15 => "💬",
        _  => "•",
    };
    let topic_str = if !topic.is_empty() {
        format!(" — {}", &topic[..topic.len().min(60)])
    } else {
        String::new()
    };
    format!("  {}{} (id: {}){}", icon, name, id, topic_str)
}

// ── discord_send_message ───────────────────────────────────────────────

async fn execute_send(args: &Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let token = get_bot_token(app_handle)?;

    // Resolve channel ID from args or credential fallback
    let channel_id = if let Some(cid) = args["channel_id"].as_str() {
        if !cid.is_empty() { cid.to_string() } else { resolve_default_channel(app_handle)? }
    } else {
        resolve_default_channel(app_handle)?
    };

    let content = args["content"].as_str().unwrap_or("");
    if content.is_empty() && args["embed"].is_null() {
        return Err("discord_send_message: 'content' is required (or provide an embed)".into());
    }

    info!("[discord] Sending message to channel {} ({} chars)", channel_id, content.len());

    let mut body = json!({});
    if !content.is_empty() {
        body["content"] = json!(content);
    }
    if !args["embed"].is_null() {
        body["embeds"] = json!([args["embed"]]);
    }

    let client = reqwest::Client::new();
    let url = format!("{}/channels/{}/messages", DISCORD_API, channel_id);
    let resp = client.post(&url)
        .header("Authorization", format!("Bot {}", token))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("HTTP error: {}", e))?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        return Err(format!("Discord API {}: {}", status, &text[..text.len().min(300)]).into());
    }

    let v: Value = serde_json::from_str(&text).unwrap_or_default();
    let msg_id = v["id"].as_str().unwrap_or("?");
    Ok(format!("Message sent! (id: {}, channel: {})", msg_id, channel_id))
}

fn resolve_default_channel(app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;
    let creds = crate::engine::skills::get_skill_credentials(&state.store, "discord")
        .map_err(|e| format!("Failed to get Discord credentials: {}", e))?;
    creds.get("DISCORD_DEFAULT_CHANNEL")
        .filter(|s| !s.is_empty())
        .cloned()
        .ok_or("No channel_id provided and DISCORD_DEFAULT_CHANNEL not set in skill credentials.".into())
}

// ── Shared helpers ─────────────────────────────────────────────────────

/// Fetch all existing channels for a guild (used for idempotency checks).
async fn fetch_existing_channels(
    client: &reqwest::Client,
    token: &str,
    server_id: &str,
) -> EngineResult<Vec<Value>> {
    let url = format!("{}/guilds/{}/channels", DISCORD_API, server_id);
    let resp = client.get(&url)
        .header("Authorization", format!("Bot {}", token))
        .send()
        .await
        .map_err(|e| format!("HTTP error fetching channels: {}", e))?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        return Err(format!("Discord API {}: {}", status, &text[..text.len().min(300)]).into());
    }

    serde_json::from_str(&text)
        .map_err(|e| format!("Failed to parse channels: {}", e).into())
}

/// Create a single Discord channel/category via REST API.
/// Returns the created channel's ID on success.
async fn create_channel(
    client: &reqwest::Client,
    token: &str,
    server_id: &str,
    body: &Value,
) -> Result<String, String> {
    let url = format!("{}/guilds/{}/channels", DISCORD_API, server_id);

    let resp = client.post(&url)
        .header("Authorization", format!("Bot {}", token))
        .header("Content-Type", "application/json")
        .json(body)
        .send()
        .await
        .map_err(|e| format!("HTTP error: {}", e))?;

    let status = resp.status();
    let resp_text = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        // Check for rate limiting
        if status.as_u16() == 429 {
            // Parse retry_after and wait
            if let Ok(v) = serde_json::from_str::<Value>(&resp_text) {
                let retry_after = v["retry_after"].as_f64().unwrap_or(1.0);
                warn!("[discord_setup] Rate limited, waiting {:.1}s", retry_after);
                tokio::time::sleep(Duration::from_secs_f64(retry_after + 0.1)).await;
                // Retry once
                let resp2 = client.post(&url)
                    .header("Authorization", format!("Bot {}", token))
                    .header("Content-Type", "application/json")
                    .json(body)
                    .send()
                    .await
                    .map_err(|e| format!("Retry HTTP error: {}", e))?;
                let status2 = resp2.status();
                let text2 = resp2.text().await.unwrap_or_default();
                if !status2.is_success() {
                    return Err(format!("API {} after retry: {}", status2, &text2[..text2.len().min(200)]));
                }
                let v2: Value = serde_json::from_str(&text2)
                    .map_err(|_| "Failed to parse retry response")?;
                return v2["id"].as_str()
                    .map(|s| s.to_string())
                    .ok_or_else(|| "No ID in retry response".to_string());
            }
        }
        return Err(format!("API {}: {}", status, &resp_text[..resp_text.len().min(200)]));
    }

    let v: Value = serde_json::from_str(&resp_text)
        .map_err(|_| "Failed to parse response JSON")?;
    v["id"].as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "No ID in response".to_string())
}

// ── discord_delete_channels ────────────────────────────────────────────

async fn execute_delete(args: &Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let token = get_bot_token(app_handle)?;
    let channel_ids = args["channel_ids"].as_array()
        .ok_or("discord_delete_channels: missing 'channel_ids' array")?;

    if channel_ids.is_empty() {
        return Err("discord_delete_channels: 'channel_ids' array is empty".into());
    }

    let client = reqwest::Client::new();
    let mut results: Vec<String> = Vec::new();
    let mut deleted = 0;
    let mut errors = 0;

    for id_val in channel_ids {
        let channel_id = id_val.as_str().unwrap_or("");
        if channel_id.is_empty() { continue; }

        info!("[discord] Deleting channel {}", channel_id);
        let url = format!("{}/channels/{}", DISCORD_API, channel_id);
        let resp = client.delete(&url)
            .header("Authorization", format!("Bot {}", token))
            .send()
            .await;

        match resp {
            Ok(r) if r.status().is_success() => {
                results.push(format!("✅ Deleted {}", channel_id));
                deleted += 1;
            }
            Ok(r) if r.status().as_u16() == 429 => {
                // Rate limited — wait and retry once
                let text = r.text().await.unwrap_or_default();
                let retry_after = serde_json::from_str::<Value>(&text)
                    .ok()
                    .and_then(|v| v["retry_after"].as_f64())
                    .unwrap_or(1.0);
                tokio::time::sleep(Duration::from_secs_f64(retry_after + 0.1)).await;
                let r2 = client.delete(&url)
                    .header("Authorization", format!("Bot {}", token))
                    .send()
                    .await;
                match r2 {
                    Ok(r2) if r2.status().is_success() => {
                        results.push(format!("✅ Deleted {} (after rate limit wait)", channel_id));
                        deleted += 1;
                    }
                    Ok(r2) => {
                        let t = r2.text().await.unwrap_or_default();
                        results.push(format!("❌ {} — {}", channel_id, &t[..t.len().min(100)]));
                        errors += 1;
                    }
                    Err(e) => {
                        results.push(format!("❌ {} — {}", channel_id, e));
                        errors += 1;
                    }
                }
            }
            Ok(r) => {
                let status = r.status();
                let text = r.text().await.unwrap_or_default();
                results.push(format!("❌ {} — API {}: {}", channel_id, status, &text[..text.len().min(100)]));
                errors += 1;
            }
            Err(e) => {
                results.push(format!("❌ {} — {}", channel_id, e));
                errors += 1;
            }
        }

        tokio::time::sleep(Duration::from_millis(300)).await;
    }

    Ok(format!(
        "Delete complete: {} deleted, {} failed\n\n{}",
        deleted, errors, results.join("\n")
    ))
}
