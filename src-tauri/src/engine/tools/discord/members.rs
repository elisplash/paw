// discord/members.rs — Member management
//
// Tools: discord_list_members, discord_get_member, discord_kick, discord_ban, discord_unban

use crate::atoms::types::*;
use crate::atoms::error::EngineResult;
use super::{DISCORD_API, get_bot_token, resolve_server_id, authorized_client, discord_request};
use log::info;
use serde_json::{json, Value};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "discord_list_members".into(),
                description: "List server members. Returns usernames, IDs, roles, and join dates. Max 1000 per call.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "server_id": { "type": "string", "description": "Guild ID." },
                        "limit": { "type": "integer", "description": "Number of members (1-1000, default 100)." },
                        "after": { "type": "string", "description": "Get members after this user ID (for pagination)." }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "discord_get_member".into(),
                description: "Get detailed info about a specific server member including roles, nickname, join date.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "server_id": { "type": "string", "description": "Guild ID." },
                        "user_id": { "type": "string", "description": "The user ID." }
                    },
                    "required": ["user_id"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "discord_kick".into(),
                description: "Kick a member from the server. They can rejoin with an invite. Requires Kick Members permission.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "server_id": { "type": "string", "description": "Guild ID." },
                        "user_id": { "type": "string", "description": "User ID to kick." },
                        "reason": { "type": "string", "description": "Optional kick reason (shown in audit log)." }
                    },
                    "required": ["user_id"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "discord_ban".into(),
                description: "Ban a user from the server. They cannot rejoin until unbanned. Requires Ban Members permission.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "server_id": { "type": "string", "description": "Guild ID." },
                        "user_id": { "type": "string", "description": "User ID to ban." },
                        "reason": { "type": "string", "description": "Optional ban reason (shown in audit log)." },
                        "delete_message_days": { "type": "integer", "description": "Delete messages from the past N days (0-7, default 0)." }
                    },
                    "required": ["user_id"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "discord_unban".into(),
                description: "Unban a user from the server.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "server_id": { "type": "string", "description": "Guild ID." },
                        "user_id": { "type": "string", "description": "User ID to unban." }
                    },
                    "required": ["user_id"]
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
        "discord_list_members" => Some(exec_list(args, app_handle).await.map_err(|e| e.to_string())),
        "discord_get_member"   => Some(exec_get(args, app_handle).await.map_err(|e| e.to_string())),
        "discord_kick"         => Some(exec_kick(args, app_handle).await.map_err(|e| e.to_string())),
        "discord_ban"          => Some(exec_ban(args, app_handle).await.map_err(|e| e.to_string())),
        "discord_unban"        => Some(exec_unban(args, app_handle).await.map_err(|e| e.to_string())),
        _ => None,
    }
}

// ── list ───────────────────────────────────────────────────────────────

async fn exec_list(args: &Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let server_id = resolve_server_id(args, app_handle)?;
    let token = get_bot_token(app_handle)?;
    let (client, auth) = authorized_client(&token);

    let limit = args["limit"].as_i64().unwrap_or(100).min(1000).max(1);
    let mut url = format!("{}/guilds/{}/members?limit={}", DISCORD_API, server_id, limit);
    if let Some(after) = args["after"].as_str() {
        url.push_str(&format!("&after={}", after));
    }

    let data = discord_request(&client, reqwest::Method::GET, &url, &auth, None).await?;
    let members: Vec<Value> = serde_json::from_value(data).unwrap_or_default();

    let mut lines = vec![format!("**Members** ({} returned)\n", members.len())];
    for m in &members {
        let user = &m["user"];
        let username = user["username"].as_str().unwrap_or("?");
        let user_id = user["id"].as_str().unwrap_or("?");
        let nick = m["nick"].as_str().map(|n| format!(" ({})", n)).unwrap_or_default();
        let bot = if user["bot"].as_bool().unwrap_or(false) { " 🤖" } else { "" };
        let roles: Vec<&str> = m["roles"].as_array()
            .map(|r| r.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();
        let role_str = if roles.is_empty() { String::new() } else { format!(" | roles: {}", roles.join(", ")) };
        let joined = m["joined_at"].as_str().map(|j| format!(" | joined: {}", &j[..j.len().min(10)])).unwrap_or_default();
        lines.push(format!("• **{}**{}{} (id: {}){}{}", username, nick, bot, user_id, role_str, joined));
    }
    Ok(lines.join("\n"))
}

// ── get single member ──────────────────────────────────────────────────

async fn exec_get(args: &Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let server_id = resolve_server_id(args, app_handle)?;
    let user_id = args["user_id"].as_str().ok_or("Missing 'user_id'")?;
    let token = get_bot_token(app_handle)?;
    let (client, auth) = authorized_client(&token);

    let url = format!("{}/guilds/{}/members/{}", DISCORD_API, server_id, user_id);
    let data = discord_request(&client, reqwest::Method::GET, &url, &auth, None).await?;

    let user = &data["user"];
    let username = user["username"].as_str().unwrap_or("?");
    let discriminator = user["discriminator"].as_str().unwrap_or("0");
    let nick = data["nick"].as_str().unwrap_or("(none)");
    let bot = if user["bot"].as_bool().unwrap_or(false) { "Yes" } else { "No" };
    let joined = data["joined_at"].as_str().unwrap_or("?");
    let roles: Vec<&str> = data["roles"].as_array()
        .map(|r| r.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    let avatar = user["avatar"].as_str().unwrap_or("(none)");

    Ok(format!(
        "**Member: {}#{}**\n\
        • ID: {}\n\
        • Nickname: {}\n\
        • Bot: {}\n\
        • Joined: {}\n\
        • Roles: {}\n\
        • Avatar: {}",
        username, discriminator, user_id, nick, bot, joined,
        if roles.is_empty() { "(none)".to_string() } else { roles.join(", ") },
        avatar
    ))
}

// ── kick ───────────────────────────────────────────────────────────────

async fn exec_kick(args: &Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let server_id = resolve_server_id(args, app_handle)?;
    let user_id = args["user_id"].as_str().ok_or("Missing 'user_id'")?;
    let token = get_bot_token(app_handle)?;
    let (client, auth) = authorized_client(&token);

    let url = format!("{}/guilds/{}/members/{}", DISCORD_API, server_id, user_id);
    // Add audit log reason as header
    let mut req = client.delete(&url)
        .header("Authorization", &auth)
        .header("Content-Type", "application/json");
    if let Some(reason) = args["reason"].as_str() {
        req = req.header("X-Audit-Log-Reason", reason);
    }

    let resp = req.send().await.map_err(|e| format!("HTTP error: {}", e))?;
    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Discord API error: {}", &text[..text.len().min(300)]).into());
    }

    info!("[discord] Kicked user {} from guild {}", user_id, server_id);
    Ok(format!("Kicked user {} from server {}", user_id, server_id))
}

// ── ban ────────────────────────────────────────────────────────────────

async fn exec_ban(args: &Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let server_id = resolve_server_id(args, app_handle)?;
    let user_id = args["user_id"].as_str().ok_or("Missing 'user_id'")?;
    let token = get_bot_token(app_handle)?;
    let (client, auth) = authorized_client(&token);

    let mut body = json!({});
    if let Some(days) = args["delete_message_days"].as_i64() {
        // Discord API v10 uses delete_message_seconds
        body["delete_message_seconds"] = json!(days.min(7).max(0) * 86400);
    }

    let url = format!("{}/guilds/{}/bans/{}", DISCORD_API, server_id, user_id);
    let mut req = client.put(&url)
        .header("Authorization", &auth)
        .header("Content-Type", "application/json")
        .json(&body);
    if let Some(reason) = args["reason"].as_str() {
        req = req.header("X-Audit-Log-Reason", reason);
    }

    let resp = req.send().await.map_err(|e| format!("HTTP error: {}", e))?;
    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Discord API error: {}", &text[..text.len().min(300)]).into());
    }

    info!("[discord] Banned user {} from guild {}", user_id, server_id);
    Ok(format!("Banned user {} from server {}", user_id, server_id))
}

// ── unban ──────────────────────────────────────────────────────────────

async fn exec_unban(args: &Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let server_id = resolve_server_id(args, app_handle)?;
    let user_id = args["user_id"].as_str().ok_or("Missing 'user_id'")?;
    let token = get_bot_token(app_handle)?;
    let (client, auth) = authorized_client(&token);

    let url = format!("{}/guilds/{}/bans/{}", DISCORD_API, server_id, user_id);
    discord_request(&client, reqwest::Method::DELETE, &url, &auth, None).await?;

    info!("[discord] Unbanned user {} from guild {}", user_id, server_id);
    Ok(format!("Unbanned user {} from server {}", user_id, server_id))
}
