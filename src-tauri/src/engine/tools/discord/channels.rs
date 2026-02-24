// discord/channels.rs — Channel & category management
//
// Tools: discord_list_channels, discord_setup_channels, discord_delete_channels, discord_edit_channel

use crate::atoms::types::*;
use crate::atoms::error::EngineResult;
use super::{DISCORD_API, get_bot_token, resolve_server_id, authorized_client, discord_request};
use log::info;
use serde_json::{json, Value};
use std::time::Duration;

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "discord_list_channels".into(),
                description: "List all channels and categories in a Discord server. Returns names, IDs, types, and parent categories. Use FIRST to see current structure before making changes.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "server_id": { "type": "string", "description": "Guild ID. Optional if DISCORD_SERVER_ID is set." }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "discord_setup_channels".into(),
                description: "Create categories and channels. Idempotent — skips existing (by name). ONLY for server structure, NOT for sending messages.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "server_id": { "type": "string", "description": "Guild ID." },
                        "categories": {
                            "type": "array",
                            "description": "Categories to create, each with channels",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "name": { "type": "string" },
                                    "channels": {
                                        "type": "array",
                                        "items": {
                                            "type": "object",
                                            "properties": {
                                                "name": { "type": "string" },
                                                "type": { "type": "integer", "description": "0=text, 2=voice" },
                                                "topic": { "type": "string" }
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
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "discord_delete_channels".into(),
                description: "Delete channels or categories by ID. DESTRUCTIVE — cannot be recovered. Use discord_list_channels first to get IDs.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "channel_ids": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Channel/category IDs to delete."
                        }
                    },
                    "required": ["channel_ids"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "discord_edit_channel".into(),
                description: "Edit a channel's name, topic, or position. Cannot change channel type.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "channel_id": { "type": "string", "description": "The channel ID to edit." },
                        "name": { "type": "string", "description": "New channel name." },
                        "topic": { "type": "string", "description": "New channel topic." },
                        "position": { "type": "integer", "description": "New position in the channel list." },
                        "nsfw": { "type": "boolean", "description": "Mark channel as NSFW." },
                        "parent_id": { "type": "string", "description": "Move channel to a different category." }
                    },
                    "required": ["channel_id"]
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
        "discord_list_channels"   => Some(execute_list(args, app_handle).await.map_err(|e| e.to_string())),
        "discord_setup_channels"  => Some(execute_setup(args, app_handle).await.map_err(|e| e.to_string())),
        "discord_delete_channels" => Some(execute_delete(args, app_handle).await.map_err(|e| e.to_string())),
        "discord_edit_channel"    => Some(execute_edit(args, app_handle).await.map_err(|e| e.to_string())),
        _ => None,
    }
}

// ── list ───────────────────────────────────────────────────────────────

async fn execute_list(args: &Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let server_id = resolve_server_id(args, app_handle)?;
    let token = get_bot_token(app_handle)?;
    let (client, auth) = authorized_client(&token);

    let url = format!("{}/guilds/{}/channels", DISCORD_API, server_id);
    let data = discord_request(&client, reqwest::Method::GET, &url, &auth, None).await?;

    let channels: Vec<Value> = serde_json::from_value(data)
        .map_err(|e| format!("Failed to parse channels: {}", e))?;

    let mut categories: Vec<&Value> = channels.iter()
        .filter(|c| c["type"].as_i64() == Some(4))
        .collect();
    categories.sort_by_key(|c| c["position"].as_i64().unwrap_or(999));

    let mut lines = vec![format!("**Discord Server Channels** (guild: {})\n", server_id)];

    // Orphan channels (no category)
    let mut orphans: Vec<&Value> = channels.iter()
        .filter(|c| c["type"].as_i64() != Some(4) && c["parent_id"].is_null())
        .collect();
    orphans.sort_by_key(|c| c["position"].as_i64().unwrap_or(999));
    if !orphans.is_empty() {
        lines.push("**[No Category]**".into());
        for ch in &orphans { lines.push(format_channel(ch)); }
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
            lines.push("  (empty)".into());
        } else {
            for ch in &children { lines.push(format_channel(ch)); }
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
        0 => "#", 2 => "🔊", 5 => "📢", 13 => "🎭", 15 => "💬", _ => "•",
    };
    let topic_str = if !topic.is_empty() {
        format!(" — {}", &topic[..topic.len().min(60)])
    } else { String::new() };
    format!("  {}{} (id: {}){}", icon, name, id, topic_str)
}

// ── setup (idempotent) ─────────────────────────────────────────────────

async fn execute_setup(args: &Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let server_id = resolve_server_id(args, app_handle)?;
    let categories = args["categories"].as_array()
        .ok_or("discord_setup_channels: missing 'categories' array")?;

    let token = get_bot_token(app_handle)?;
    let (client, auth) = authorized_client(&token);

    // Fetch existing for idempotency
    let url = format!("{}/guilds/{}/channels", DISCORD_API, server_id);
    let data = discord_request(&client, reqwest::Method::GET, &url, &auth, None).await?;
    let existing: Vec<Value> = serde_json::from_value(data).unwrap_or_default();

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
    let mut created = 0u32;
    let mut skipped = 0u32;
    let mut errors = 0u32;

    let create_url = format!("{}/guilds/{}/channels", DISCORD_API, server_id);

    for category in categories {
        let cat_name = category["name"].as_str().unwrap_or("Unnamed");
        let channels = category["channels"].as_array();

        let category_id = if let Some(id) = existing_cats.get(&cat_name.to_lowercase()) {
            results.push(format!("⏭️  Category '{}' already exists ({})", cat_name, id));
            skipped += 1;
            id.clone()
        } else {
            let body = json!({ "name": cat_name, "type": 4 });
            match discord_request(&client, reqwest::Method::POST, &create_url, &auth, Some(&body)).await {
                Ok(v) => {
                    let id = v["id"].as_str().unwrap_or("?").to_string();
                    results.push(format!("✅ Category '{}' created ({})", cat_name, id));
                    created += 1;
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    id
                }
                Err(e) => {
                    results.push(format!("❌ Category '{}' failed: {}", cat_name, e));
                    errors += 1;
                    continue;
                }
            }
        };

        if let Some(channels) = channels {
            for channel in channels {
                let ch_name = channel["name"].as_str().unwrap_or("unnamed");
                let ch_type = channel["type"].as_i64().unwrap_or(0);
                let ch_topic = channel["topic"].as_str();

                if existing_channels.contains(&(ch_name.to_lowercase(), Some(category_id.clone()))) {
                    let icon = if ch_type == 2 { "🔊" } else { "#" };
                    results.push(format!("  ⏭️  {}{} already exists", icon, ch_name));
                    skipped += 1;
                    continue;
                }

                let mut body = json!({ "name": ch_name, "type": ch_type, "parent_id": category_id });
                if let Some(topic) = ch_topic { body["topic"] = json!(topic); }

                match discord_request(&client, reqwest::Method::POST, &create_url, &auth, Some(&body)).await {
                    Ok(v) => {
                        let id = v["id"].as_str().unwrap_or("?");
                        let icon = if ch_type == 2 { "🔊" } else { "#" };
                        results.push(format!("  ✅ {}{} created ({})", icon, ch_name, id));
                        created += 1;
                    }
                    Err(e) => {
                        results.push(format!("  ❌ #{} failed: {}", ch_name, e));
                        errors += 1;
                    }
                }
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
        }
    }

    let summary = if created == 0 && errors == 0 {
        format!("All {} items already exist — nothing to create.\n\n{}", skipped, results.join("\n"))
    } else {
        format!("Setup complete! Created: {} | Skipped: {} | Failed: {}\n\n{}", created, skipped, errors, results.join("\n"))
    };

    info!("[discord] Setup: {} created, {} skipped, {} errors", created, skipped, errors);
    Ok(summary)
}

// ── delete ─────────────────────────────────────────────────────────────

async fn execute_delete(args: &Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let token = get_bot_token(app_handle)?;
    let (client, auth) = authorized_client(&token);
    let ids = args["channel_ids"].as_array()
        .ok_or("Missing 'channel_ids' array")?;

    let mut results: Vec<String> = Vec::new();
    let mut deleted = 0u32;
    let mut errors = 0u32;

    for id_val in ids {
        let cid = id_val.as_str().unwrap_or("");
        if cid.is_empty() { continue; }

        let url = format!("{}/channels/{}", DISCORD_API, cid);
        match discord_request(&client, reqwest::Method::DELETE, &url, &auth, None).await {
            Ok(_) => { results.push(format!("✅ Deleted {}", cid)); deleted += 1; }
            Err(e) => { results.push(format!("❌ {} — {}", cid, e)); errors += 1; }
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }

    Ok(format!("Delete: {} deleted, {} failed\n\n{}", deleted, errors, results.join("\n")))
}

// ── edit ───────────────────────────────────────────────────────────────

async fn execute_edit(args: &Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let channel_id = args["channel_id"].as_str()
        .ok_or("Missing 'channel_id'")?;
    let token = get_bot_token(app_handle)?;
    let (client, auth) = authorized_client(&token);

    let mut body = json!({});
    if let Some(v) = args.get("name").and_then(|v| v.as_str()) { body["name"] = json!(v); }
    if let Some(v) = args.get("topic").and_then(|v| v.as_str()) { body["topic"] = json!(v); }
    if let Some(v) = args.get("position").and_then(|v| v.as_i64()) { body["position"] = json!(v); }
    if let Some(v) = args.get("nsfw").and_then(|v| v.as_bool()) { body["nsfw"] = json!(v); }
    if let Some(v) = args.get("parent_id").and_then(|v| v.as_str()) { body["parent_id"] = json!(v); }

    if body.as_object().map(|o| o.is_empty()).unwrap_or(true) {
        return Err("No fields to update. Provide at least one of: name, topic, position, nsfw, parent_id".into());
    }

    let url = format!("{}/channels/{}", DISCORD_API, channel_id);
    let result = discord_request(&client, reqwest::Method::PATCH, &url, &auth, Some(&body)).await?;

    let new_name = result["name"].as_str().unwrap_or("?");
    Ok(format!("Channel updated: #{} ({})", new_name, channel_id))
}
