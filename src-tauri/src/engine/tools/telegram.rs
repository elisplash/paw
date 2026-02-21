// Paw Agent Engine — Telegram tools
// telegram_send, telegram_read

use crate::atoms::types::*;
use crate::atoms::error::EngineResult;
use log::info;
use std::time::Duration;

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "telegram_send".into(),
                description: "Send a proactive message to a Telegram user. The user must have messaged the bot at least once so their chat_id is known. You can specify the user by their @username (without the @) or by numeric chat_id. If neither is specified, sends to the first known user (the owner). Bot token is loaded automatically from the Telegram bridge config.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "text": { "type": "string", "description": "The message text to send" },
                        "username": { "type": "string", "description": "Telegram username (without @) to send to. Optional — defaults to first known user." },
                        "chat_id": { "type": "integer", "description": "Numeric Telegram chat ID. Alternative to username." }
                    },
                    "required": ["text"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "telegram_read".into(),
                description: "Get information about the Telegram bridge status, known users, and configuration. Useful for checking if the Telegram bridge is running and who has messaged the bot.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "info": { "type": "string", "enum": ["status", "users"], "description": "What to retrieve: 'status' for bridge health, 'users' for list of known users (default: status)" }
                    }
                }),
            },
        },
    ]
}

pub async fn execute(
    name: &str,
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
) -> Option<Result<String, String>> {
    match name {
        "telegram_send" => Some(execute_telegram_send(args, app_handle).await.map_err(|e| e.to_string())),
        "telegram_read" => Some(execute_telegram_read(args, app_handle).await.map_err(|e| e.to_string())),
        _ => None,
    }
}

impl ToolDefinition {
    pub fn telegram_send() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "telegram_send".into(),
                description: "Send a proactive message to a Telegram user. The user must have messaged the bot at least once so their chat_id is known.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "text": { "type": "string", "description": "The message text to send" },
                        "username": { "type": "string", "description": "Telegram username (without @) to send to. Optional." },
                        "chat_id": { "type": "integer", "description": "Numeric Telegram chat ID. Alternative to username." }
                    },
                    "required": ["text"]
                }),
            },
        }
    }

    pub fn telegram_read() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "telegram_read".into(),
                description: "Get information about the Telegram bridge status and known users.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "info": { "type": "string", "enum": ["status", "users"], "description": "What to retrieve: 'status' for bridge health, 'users' for list of known users (default: status)" }
                    }
                }),
            },
        }
    }
}

async fn execute_telegram_send(args: &serde_json::Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    use crate::engine::telegram::load_telegram_config;

    let text = args["text"].as_str().ok_or("telegram_send: missing 'text'")?;
    let config = load_telegram_config(app_handle)?;

    if config.bot_token.is_empty() {
        return Err("Telegram bot is not configured. Ask the user to set up their Telegram bot token in the Telegram channel settings.".into());
    }

    let chat_id: i64 = if let Some(cid) = args["chat_id"].as_i64() {
        cid
    } else if let Some(username) = args["username"].as_str() {
        let key = username.trim_start_matches('@').to_lowercase();
        *config.known_users.get(&key)
            .ok_or_else(|| format!(
                "telegram_send: unknown username '{}'. Known users: {}. The user needs to message the bot first.",
                username,
                if config.known_users.is_empty() {
                    "none — no users have messaged the bot yet".into()
                } else {
                    config.known_users.keys().map(|k| format!("@{}", k)).collect::<Vec<_>>().join(", ")
                }
            ))?
    } else if let Some((_name, &cid)) = config.known_users.iter().next() {
        cid
    } else if let Some(&uid) = config.allowed_users.first() {
        uid
    } else {
        return Err("telegram_send: no target specified and no known users. Someone needs to message the bot first so we learn their chat_id.".into());
    };

    info!("[tool:telegram_send] Sending to chat_id {}: {}...", chat_id,
        if text.len() > 50 { &text[..50] } else { text });

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?;

    let chunks: Vec<String> = if text.len() > 4000 {
        text.chars().collect::<Vec<_>>()
            .chunks(4000)
            .map(|c| c.iter().collect::<String>())
            .collect()
    } else {
        vec![text.to_string()]
    };

    for chunk in &chunks {
        let body = serde_json::json!({
            "chat_id": chat_id,
            "text": chunk,
            "parse_mode": "Markdown",
        });
        let resp = client.post(format!("https://api.telegram.org/bot{}/sendMessage", config.bot_token))
            .json(&body)
            .send()
            .await?;
        let result: serde_json::Value = resp.json().await?;
        if !result["ok"].as_bool().unwrap_or(false) {
            let desc = result["description"].as_str().unwrap_or("unknown error");
            return Err(format!("Telegram API error: {}", desc).into());
        }
    }

    Ok(format!("Message sent to Telegram (chat_id: {}, {} chars, {} chunk(s))", chat_id, text.len(), chunks.len()))
}

async fn execute_telegram_read(args: &serde_json::Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    use crate::engine::telegram::load_telegram_config;

    let info = args["info"].as_str().unwrap_or("status");
    let config = load_telegram_config(app_handle)?;

    match info {
        "users" => {
            let mut output = String::from("Known Telegram users:\n\n");
            if config.known_users.is_empty() {
                output.push_str("No users have messaged the bot yet.\n");
            } else {
                for (username, chat_id) in &config.known_users {
                    output.push_str(&format!("  @{} (chat_id: {})\n", username, chat_id));
                }
            }
            output.push_str(&format!("\nAllowed user IDs: {:?}\n", config.allowed_users));
            if !config.pending_users.is_empty() {
                output.push_str(&format!("Pending approvals: {}\n", config.pending_users.len()));
            }
            Ok(output)
        }
        _ => {
            let running = crate::engine::telegram::is_bridge_running();
            Ok(format!(
                "Telegram Bridge Status:\n  Running: {}\n  Bot configured: {}\n  DM policy: {}\n  Allowed users: {}\n  Known users: {}\n  Agent: {}",
                running,
                !config.bot_token.is_empty(),
                config.dm_policy,
                config.allowed_users.len(),
                config.known_users.len(),
                config.agent_id.as_deref().unwrap_or("default"),
            ))
        }
    }
}
