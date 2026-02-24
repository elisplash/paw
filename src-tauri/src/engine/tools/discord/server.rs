// discord/server.rs — Server info & invites
//
// Tools: discord_server_info, discord_create_invite

use crate::atoms::types::*;
use crate::atoms::error::EngineResult;
use super::{DISCORD_API, get_bot_token, resolve_server_id, resolve_channel_id, authorized_client, discord_request};
use log::info;
use serde_json::{json, Value};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "discord_server_info".into(),
                description: "Get server (guild) info: name, owner, member count, boost level, features, icon, etc.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "server_id": { "type": "string", "description": "Guild ID. Optional if DISCORD_SERVER_ID set." }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "discord_create_invite".into(),
                description: "Create an invite link for a channel. Returns a discord.gg URL.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "channel_id": { "type": "string", "description": "Channel to create invite for. Optional if DISCORD_DEFAULT_CHANNEL set." },
                        "max_age": { "type": "integer", "description": "Invite expiration in seconds (0 = never, default 86400 = 24h)." },
                        "max_uses": { "type": "integer", "description": "Max number of uses (0 = unlimited, default 0)." },
                        "unique": { "type": "boolean", "description": "If true, always create a new invite. If false, may return existing." }
                    }
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
        "discord_server_info"   => Some(exec_info(args, app_handle).await.map_err(|e| e.to_string())),
        "discord_create_invite" => Some(exec_invite(args, app_handle).await.map_err(|e| e.to_string())),
        _ => None,
    }
}

// ── server info ────────────────────────────────────────────────────────

async fn exec_info(args: &Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let server_id = resolve_server_id(args, app_handle)?;
    let token = get_bot_token(app_handle)?;
    let (client, auth) = authorized_client(&token);

    // Use ?with_counts=true to get online/total member counts
    let url = format!("{}/guilds/{}?with_counts=true", DISCORD_API, server_id);
    let data = discord_request(&client, reqwest::Method::GET, &url, &auth, None).await?;

    let name = data["name"].as_str().unwrap_or("?");
    let owner_id = data["owner_id"].as_str().unwrap_or("?");
    let members = data["approximate_member_count"].as_i64().unwrap_or(0);
    let online = data["approximate_presence_count"].as_i64().unwrap_or(0);
    let boosts = data["premium_subscription_count"].as_i64().unwrap_or(0);
    let boost_tier = data["premium_tier"].as_i64().unwrap_or(0);
    let description = data["description"].as_str().unwrap_or("(none)");
    let verification = match data["verification_level"].as_i64().unwrap_or(0) {
        0 => "None", 1 => "Low", 2 => "Medium", 3 => "High", 4 => "Very High", _ => "?",
    };
    let features: Vec<&str> = data["features"].as_array()
        .map(|f| f.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    let icon = data["icon"].as_str()
        .map(|h| format!("https://cdn.discordapp.com/icons/{}/{}.png", server_id, h))
        .unwrap_or_else(|| "(none)".into());

    Ok(format!(
        "**{}** (id: {})\n\
        • Owner: {}\n\
        • Members: {} ({} online)\n\
        • Boosts: {} (tier {})\n\
        • Verification: {}\n\
        • Description: {}\n\
        • Features: {}\n\
        • Icon: {}",
        name, server_id, owner_id, members, online, boosts, boost_tier,
        verification, description,
        if features.is_empty() { "(none)".to_string() } else { features.join(", ") },
        icon
    ))
}

// ── create invite ──────────────────────────────────────────────────────

async fn exec_invite(args: &Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let channel_id = resolve_channel_id(args, app_handle)?;
    let token = get_bot_token(app_handle)?;
    let (client, auth) = authorized_client(&token);

    let mut body = json!({});
    if let Some(age) = args["max_age"].as_i64() { body["max_age"] = json!(age); }
    if let Some(uses) = args["max_uses"].as_i64() { body["max_uses"] = json!(uses); }
    if let Some(unique) = args["unique"].as_bool() { body["unique"] = json!(unique); }

    let url = format!("{}/channels/{}/invites", DISCORD_API, channel_id);
    let result = discord_request(&client, reqwest::Method::POST, &url, &auth, Some(&body)).await?;

    let code = result["code"].as_str().unwrap_or("?");
    let max_age = result["max_age"].as_i64().unwrap_or(0);
    let max_uses = result["max_uses"].as_i64().unwrap_or(0);
    let expiry = if max_age == 0 { "never".to_string() } else { format!("{}h", max_age / 3600) };
    let uses = if max_uses == 0 { "unlimited".to_string() } else { format!("{}", max_uses) };

    info!("[discord] Created invite: discord.gg/{}", code);
    Ok(format!(
        "Invite created: **https://discord.gg/{}**\n• Expires: {}\n• Max uses: {}",
        code, expiry, uses
    ))
}
