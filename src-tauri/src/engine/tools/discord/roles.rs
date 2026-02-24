// discord/roles.rs — Role management
//
// Tools: discord_list_roles, discord_create_role, discord_delete_role,
//        discord_assign_role, discord_remove_role

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
                name: "discord_list_roles".into(),
                description: "List all roles in a Discord server with IDs, names, colors, and permissions.".into(),
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
                name: "discord_create_role".into(),
                description: "Create a new role in a Discord server.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "server_id": { "type": "string", "description": "Guild ID." },
                        "name": { "type": "string", "description": "Role name." },
                        "color": { "type": "integer", "description": "Decimal color value (e.g. 0xFF0000 = 16711680 for red)." },
                        "hoist": { "type": "boolean", "description": "Show role members separately in sidebar (default false)." },
                        "mentionable": { "type": "boolean", "description": "Allow anyone to @mention this role (default false)." },
                        "permissions": { "type": "string", "description": "Permission bitfield as string (e.g. '0' for none, '8' for admin)." }
                    },
                    "required": ["server_id", "name"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "discord_delete_role".into(),
                description: "Delete a role from a Discord server. DESTRUCTIVE.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "server_id": { "type": "string", "description": "Guild ID." },
                        "role_id": { "type": "string", "description": "Role ID to delete." }
                    },
                    "required": ["server_id", "role_id"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "discord_assign_role".into(),
                description: "Assign a role to a server member.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "server_id": { "type": "string", "description": "Guild ID." },
                        "user_id": { "type": "string", "description": "User ID to assign the role to." },
                        "role_id": { "type": "string", "description": "Role ID to assign." }
                    },
                    "required": ["server_id", "user_id", "role_id"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "discord_remove_role".into(),
                description: "Remove a role from a server member.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "server_id": { "type": "string", "description": "Guild ID." },
                        "user_id": { "type": "string", "description": "User ID." },
                        "role_id": { "type": "string", "description": "Role ID to remove." }
                    },
                    "required": ["server_id", "user_id", "role_id"]
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
        "discord_list_roles"   => Some(exec_list(args, app_handle).await.map_err(|e| e.to_string())),
        "discord_create_role"  => Some(exec_create(args, app_handle).await.map_err(|e| e.to_string())),
        "discord_delete_role"  => Some(exec_delete(args, app_handle).await.map_err(|e| e.to_string())),
        "discord_assign_role"  => Some(exec_assign(args, app_handle).await.map_err(|e| e.to_string())),
        "discord_remove_role"  => Some(exec_remove(args, app_handle).await.map_err(|e| e.to_string())),
        _ => None,
    }
}

// ── list ───────────────────────────────────────────────────────────────

async fn exec_list(args: &Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let server_id = resolve_server_id(args, app_handle)?;
    let token = get_bot_token(app_handle)?;
    let (client, auth) = authorized_client(&token);

    let url = format!("{}/guilds/{}/roles", DISCORD_API, server_id);
    let data = discord_request(&client, reqwest::Method::GET, &url, &auth, None).await?;
    let roles: Vec<Value> = serde_json::from_value(data).unwrap_or_default();

    let mut lines = vec![format!("**Roles** ({} total)\n", roles.len())];
    for role in &roles {
        let name = role["name"].as_str().unwrap_or("?");
        let id = role["id"].as_str().unwrap_or("?");
        let color = role["color"].as_i64().unwrap_or(0);
        let members = role["member_count"].as_i64().map(|n| format!(" ({} members)", n)).unwrap_or_default();
        let hoist = if role["hoist"].as_bool().unwrap_or(false) { " [hoisted]" } else { "" };
        let managed = if role["managed"].as_bool().unwrap_or(false) { " [managed]" } else { "" };
        let color_hex = if color > 0 { format!(" #{:06X}", color) } else { String::new() };
        lines.push(format!("• **{}** (id: {}){}{}{}{}", name, id, color_hex, hoist, managed, members));
    }
    Ok(lines.join("\n"))
}

// ── create ─────────────────────────────────────────────────────────────

async fn exec_create(args: &Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let server_id = resolve_server_id(args, app_handle)?;
    let token = get_bot_token(app_handle)?;
    let (client, auth) = authorized_client(&token);

    let name = args["name"].as_str().ok_or("Missing 'name'")?;
    let mut body = json!({ "name": name });
    if let Some(c) = args["color"].as_i64() { body["color"] = json!(c); }
    if let Some(h) = args["hoist"].as_bool() { body["hoist"] = json!(h); }
    if let Some(m) = args["mentionable"].as_bool() { body["mentionable"] = json!(m); }
    if let Some(p) = args["permissions"].as_str() { body["permissions"] = json!(p); }

    let url = format!("{}/guilds/{}/roles", DISCORD_API, server_id);
    let result = discord_request(&client, reqwest::Method::POST, &url, &auth, Some(&body)).await?;

    let role_id = result["id"].as_str().unwrap_or("?");
    info!("[discord] Created role '{}' ({})", name, role_id);
    Ok(format!("Role '{}' created (id: {})", name, role_id))
}

// ── delete ─────────────────────────────────────────────────────────────

async fn exec_delete(args: &Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let server_id = resolve_server_id(args, app_handle)?;
    let role_id = args["role_id"].as_str().ok_or("Missing 'role_id'")?;
    let token = get_bot_token(app_handle)?;
    let (client, auth) = authorized_client(&token);

    let url = format!("{}/guilds/{}/roles/{}", DISCORD_API, server_id, role_id);
    discord_request(&client, reqwest::Method::DELETE, &url, &auth, None).await?;

    info!("[discord] Deleted role {}", role_id);
    Ok(format!("Role {} deleted", role_id))
}

// ── assign ─────────────────────────────────────────────────────────────

async fn exec_assign(args: &Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let server_id = resolve_server_id(args, app_handle)?;
    let user_id = args["user_id"].as_str().ok_or("Missing 'user_id'")?;
    let role_id = args["role_id"].as_str().ok_or("Missing 'role_id'")?;
    let token = get_bot_token(app_handle)?;
    let (client, auth) = authorized_client(&token);

    let url = format!("{}/guilds/{}/members/{}/roles/{}", DISCORD_API, server_id, user_id, role_id);
    discord_request(&client, reqwest::Method::PUT, &url, &auth, None).await?;

    Ok(format!("Assigned role {} to user {} in guild {}", role_id, user_id, server_id))
}

// ── remove ─────────────────────────────────────────────────────────────

async fn exec_remove(args: &Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let server_id = resolve_server_id(args, app_handle)?;
    let user_id = args["user_id"].as_str().ok_or("Missing 'user_id'")?;
    let role_id = args["role_id"].as_str().ok_or("Missing 'role_id'")?;
    let token = get_bot_token(app_handle)?;
    let (client, auth) = authorized_client(&token);

    let url = format!("{}/guilds/{}/members/{}/roles/{}", DISCORD_API, server_id, user_id, role_id);
    discord_request(&client, reqwest::Method::DELETE, &url, &auth, None).await?;

    Ok(format!("Removed role {} from user {} in guild {}", role_id, user_id, server_id))
}
