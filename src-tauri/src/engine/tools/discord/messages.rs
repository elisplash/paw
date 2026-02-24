// discord/messages.rs — Message operations
//
// Tools: discord_send_message, discord_edit_message, discord_delete_messages,
//        discord_get_messages, discord_pin_message, discord_unpin_message, discord_react

use crate::atoms::types::*;
use crate::atoms::error::EngineResult;
use super::{DISCORD_API, get_bot_token, resolve_channel_id, authorized_client, discord_request};
use log::info;
use serde_json::{json, Value};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "discord_send_message".into(),
                description: "Send a message to a Discord channel. For posting content, announcements, welcome messages. NOT for creating channels.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "channel_id": { "type": "string", "description": "Target channel ID. Optional if DISCORD_DEFAULT_CHANNEL set." },
                        "content": { "type": "string", "description": "Message text (max 2000 chars). Supports Discord markdown." },
                        "embed": {
                            "type": "object",
                            "description": "Optional rich embed",
                            "properties": {
                                "title": { "type": "string" },
                                "description": { "type": "string" },
                                "color": { "type": "integer", "description": "Decimal color (e.g. 5814783 for blue)" },
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
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "discord_edit_message".into(),
                description: "Edit an existing message sent by the bot. Requires the message ID.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "channel_id": { "type": "string", "description": "Channel containing the message." },
                        "message_id": { "type": "string", "description": "The message ID to edit." },
                        "content": { "type": "string", "description": "New message text." },
                        "embed": { "type": "object", "description": "New embed (replaces existing)." }
                    },
                    "required": ["channel_id", "message_id"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "discord_delete_messages".into(),
                description: "Delete messages from a channel. Can delete a single message by ID, or bulk-delete up to 100 recent messages. Bot needs Manage Messages permission for others' messages.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "channel_id": { "type": "string", "description": "Channel ID." },
                        "message_ids": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Specific message IDs to delete (2-100 for bulk, or 1 for single)."
                        },
                        "count": {
                            "type": "integer",
                            "description": "Delete this many recent messages (1-100). Used instead of message_ids."
                        }
                    },
                    "required": ["channel_id"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "discord_get_messages".into(),
                description: "Get recent messages from a channel. Returns up to 100 messages with author, content, and timestamp.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "channel_id": { "type": "string", "description": "Channel ID." },
                        "limit": { "type": "integer", "description": "Number of messages (1-100, default 25)." },
                        "before": { "type": "string", "description": "Get messages before this message ID." },
                        "after": { "type": "string", "description": "Get messages after this message ID." }
                    },
                    "required": ["channel_id"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "discord_pin_message".into(),
                description: "Pin a message in a channel. Max 50 pinned messages per channel.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "channel_id": { "type": "string" },
                        "message_id": { "type": "string" }
                    },
                    "required": ["channel_id", "message_id"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "discord_unpin_message".into(),
                description: "Unpin a message in a channel.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "channel_id": { "type": "string" },
                        "message_id": { "type": "string" }
                    },
                    "required": ["channel_id", "message_id"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "discord_react".into(),
                description: "Add a reaction emoji to a message. Use URL-encoded emoji (e.g. '%F0%9F%91%8D' for 👍, or 'name:id' for custom).".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "channel_id": { "type": "string" },
                        "message_id": { "type": "string" },
                        "emoji": { "type": "string", "description": "URL-encoded emoji (e.g. '%F0%9F%91%8D') or custom emoji 'name:id'." }
                    },
                    "required": ["channel_id", "message_id", "emoji"]
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
        "discord_send_message"    => Some(exec_send(args, app_handle).await.map_err(|e| e.to_string())),
        "discord_edit_message"    => Some(exec_edit(args, app_handle).await.map_err(|e| e.to_string())),
        "discord_delete_messages" => Some(exec_delete(args, app_handle).await.map_err(|e| e.to_string())),
        "discord_get_messages"    => Some(exec_get(args, app_handle).await.map_err(|e| e.to_string())),
        "discord_pin_message"     => Some(exec_pin(args, app_handle, true).await.map_err(|e| e.to_string())),
        "discord_unpin_message"   => Some(exec_pin(args, app_handle, false).await.map_err(|e| e.to_string())),
        "discord_react"           => Some(exec_react(args, app_handle).await.map_err(|e| e.to_string())),
        _ => None,
    }
}

// ── send ───────────────────────────────────────────────────────────────

async fn exec_send(args: &Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let token = get_bot_token(app_handle)?;
    let channel_id = resolve_channel_id(args, app_handle)?;
    let (client, auth) = authorized_client(&token);

    let content = args["content"].as_str().unwrap_or("");
    if content.is_empty() && args["embed"].is_null() {
        return Err("'content' is required (or provide an embed)".into());
    }

    let mut body = json!({});
    if !content.is_empty() { body["content"] = json!(content); }
    if !args["embed"].is_null() { body["embeds"] = json!([args["embed"]]); }

    info!("[discord] Sending message to channel {} ({} chars)", channel_id, content.len());
    let url = format!("{}/channels/{}/messages", DISCORD_API, channel_id);
    let result = discord_request(&client, reqwest::Method::POST, &url, &auth, Some(&body)).await?;

    let msg_id = result["id"].as_str().unwrap_or("?");
    Ok(format!("Message sent! (id: {}, channel: {})", msg_id, channel_id))
}

// ── edit ───────────────────────────────────────────────────────────────

async fn exec_edit(args: &Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let token = get_bot_token(app_handle)?;
    let channel_id = args["channel_id"].as_str().ok_or("Missing 'channel_id'")?;
    let message_id = args["message_id"].as_str().ok_or("Missing 'message_id'")?;
    let (client, auth) = authorized_client(&token);

    let mut body = json!({});
    if let Some(c) = args["content"].as_str() { body["content"] = json!(c); }
    if !args["embed"].is_null() { body["embeds"] = json!([args["embed"]]); }

    let url = format!("{}/channels/{}/messages/{}", DISCORD_API, channel_id, message_id);
    discord_request(&client, reqwest::Method::PATCH, &url, &auth, Some(&body)).await?;

    Ok(format!("Message {} edited in channel {}", message_id, channel_id))
}

// ── delete / purge ─────────────────────────────────────────────────────

async fn exec_delete(args: &Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let token = get_bot_token(app_handle)?;
    let channel_id = args["channel_id"].as_str().ok_or("Missing 'channel_id'")?;
    let (client, auth) = authorized_client(&token);

    // Option 1: specific message IDs
    if let Some(ids) = args["message_ids"].as_array() {
        if ids.len() == 1 {
            // Single delete
            let mid = ids[0].as_str().unwrap_or("");
            let url = format!("{}/channels/{}/messages/{}", DISCORD_API, channel_id, mid);
            discord_request(&client, reqwest::Method::DELETE, &url, &auth, None).await?;
            return Ok(format!("Deleted message {}", mid));
        }
        if ids.len() >= 2 {
            // Bulk delete (2-100)
            let body = json!({ "messages": ids });
            let url = format!("{}/channels/{}/messages/bulk-delete", DISCORD_API, channel_id);
            discord_request(&client, reqwest::Method::POST, &url, &auth, Some(&body)).await?;
            return Ok(format!("Bulk-deleted {} messages", ids.len()));
        }
    }

    // Option 2: delete N recent messages
    if let Some(count) = args["count"].as_i64() {
        let count = count.min(100).max(1);
        // Fetch message IDs first
        let url = format!("{}/channels/{}/messages?limit={}", DISCORD_API, channel_id, count);
        let data = discord_request(&client, reqwest::Method::GET, &url, &auth, None).await?;
        let msgs: Vec<Value> = serde_json::from_value(data).unwrap_or_default();
        let ids: Vec<&str> = msgs.iter().filter_map(|m| m["id"].as_str()).collect();

        if ids.is_empty() {
            return Ok("No messages to delete.".into());
        }

        if ids.len() == 1 {
            let url = format!("{}/channels/{}/messages/{}", DISCORD_API, channel_id, ids[0]);
            discord_request(&client, reqwest::Method::DELETE, &url, &auth, None).await?;
            return Ok("Deleted 1 message.".into());
        }

        let body = json!({ "messages": ids });
        let url = format!("{}/channels/{}/messages/bulk-delete", DISCORD_API, channel_id);
        discord_request(&client, reqwest::Method::POST, &url, &auth, Some(&body)).await?;
        return Ok(format!("Deleted {} recent messages.", ids.len()));
    }

    Err("Provide 'message_ids' (array) or 'count' (number) to specify what to delete.".into())
}

// ── get history ────────────────────────────────────────────────────────

async fn exec_get(args: &Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let token = get_bot_token(app_handle)?;
    let channel_id = args["channel_id"].as_str().ok_or("Missing 'channel_id'")?;
    let (client, auth) = authorized_client(&token);

    let limit = args["limit"].as_i64().unwrap_or(25).min(100).max(1);
    let mut url = format!("{}/channels/{}/messages?limit={}", DISCORD_API, channel_id, limit);
    if let Some(before) = args["before"].as_str() {
        url.push_str(&format!("&before={}", before));
    }
    if let Some(after) = args["after"].as_str() {
        url.push_str(&format!("&after={}", after));
    }

    let data = discord_request(&client, reqwest::Method::GET, &url, &auth, None).await?;
    let msgs: Vec<Value> = serde_json::from_value(data).unwrap_or_default();

    let mut lines = vec![format!("**Messages in channel {}** ({} returned)\n", channel_id, msgs.len())];
    for msg in msgs.iter().rev() {
        let author = msg["author"]["username"].as_str().unwrap_or("?");
        let content = msg["author"]["bot"].as_bool().map(|b| if b { "🤖" } else { "" }).unwrap_or("");
        let text = msg["content"].as_str().unwrap_or("");
        let id = msg["id"].as_str().unwrap_or("?");
        let ts = msg["timestamp"].as_str().unwrap_or("?");
        let preview = if text.len() > 200 { format!("{}…", &text[..text.floor_char_boundary(200)]) } else { text.to_string() };
        lines.push(format!("[{}] {}{}: {} (id: {})", &ts[..ts.len().min(16)], content, author, preview, id));
    }
    Ok(lines.join("\n"))
}

// ── pin / unpin ────────────────────────────────────────────────────────

async fn exec_pin(args: &Value, app_handle: &tauri::AppHandle, pin: bool) -> EngineResult<String> {
    let token = get_bot_token(app_handle)?;
    let channel_id = args["channel_id"].as_str().ok_or("Missing 'channel_id'")?;
    let message_id = args["message_id"].as_str().ok_or("Missing 'message_id'")?;
    let (client, auth) = authorized_client(&token);

    let url = format!("{}/channels/{}/pins/{}", DISCORD_API, channel_id, message_id);
    let method = if pin { reqwest::Method::PUT } else { reqwest::Method::DELETE };
    discord_request(&client, method, &url, &auth, None).await?;

    let action = if pin { "Pinned" } else { "Unpinned" };
    Ok(format!("{} message {} in channel {}", action, message_id, channel_id))
}

// ── react ──────────────────────────────────────────────────────────────

async fn exec_react(args: &Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let token = get_bot_token(app_handle)?;
    let channel_id = args["channel_id"].as_str().ok_or("Missing 'channel_id'")?;
    let message_id = args["message_id"].as_str().ok_or("Missing 'message_id'")?;
    let emoji = args["emoji"].as_str().ok_or("Missing 'emoji'")?;
    let (client, auth) = authorized_client(&token);

    let url = format!("{}/channels/{}/messages/{}/reactions/{}/@me", DISCORD_API, channel_id, message_id, emoji);
    discord_request(&client, reqwest::Method::PUT, &url, &auth, None).await?;

    Ok(format!("Reacted with {} on message {} in channel {}", emoji, message_id, channel_id))
}
