// Paw Agent Engine — Slack tools
// slack_send, slack_read

use crate::atoms::types::*;
use log::info;
use std::time::Duration;
use crate::atoms::error::EngineResult;

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "slack_send".into(),
                description: "Send a message to a Slack channel or DM. Credentials are stored securely.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "channel": { "type": "string", "description": "Channel ID (C...) or user ID (U...) to send to. Uses default channel if not specified." },
                        "text": { "type": "string", "description": "The message text to send" }
                    },
                    "required": ["text"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "slack_read".into(),
                description: "Read recent messages from a Slack channel.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "channel": { "type": "string", "description": "Channel ID to read from" },
                        "limit": { "type": "integer", "description": "Number of messages (default: 10)" }
                    },
                    "required": ["channel"]
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
        "slack_send" | "slack_read" => {}
        _ => return None,
    }
    let creds = match super::get_skill_creds("slack", app_handle) {
        Ok(c) => c,
        Err(e) => return Some(Err(e.to_string())),
    };
    Some(match name {
        "slack_send" => execute_slack_send(args, &creds).await.map_err(|e| e.to_string()),
        "slack_read" => execute_slack_read(args, &creds).await.map_err(|e| e.to_string()),
        _ => unreachable!(),
    })
}

async fn execute_slack_send(
    args: &serde_json::Value,
    creds: &std::collections::HashMap<String, String>,
) -> EngineResult<String> {
    let text = args["text"].as_str().ok_or("slack_send: missing 'text'")?;
    let token = creds.get("SLACK_BOT_TOKEN").ok_or("Missing SLACK_BOT_TOKEN")?;
    let channel = args["channel"].as_str()
        .map(|s| s.to_string())
        .or_else(|| creds.get("SLACK_DEFAULT_CHANNEL").cloned())
        .ok_or("slack_send: no channel specified and no default channel configured")?;

    info!("[skill:slack] Sending to channel {}", channel);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?;

    let resp = client.post("https://slack.com/api/chat.postMessage")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({ "channel": channel, "text": text }))
        .send()
        .await?;

    let body: serde_json::Value = resp.json().await?;

    if body["ok"].as_bool().unwrap_or(false) {
        let ts = body["ts"].as_str().unwrap_or("unknown");
        Ok(format!("Message sent to Slack channel {} (ts: {})", channel, ts))
    } else {
        let err = body["error"].as_str().unwrap_or("unknown error");
        Err(format!("Slack API error: {}", err).into())
    }
}

async fn execute_slack_read(
    args: &serde_json::Value,
    creds: &std::collections::HashMap<String, String>,
) -> EngineResult<String> {
    let channel = args["channel"].as_str().ok_or("slack_read: missing 'channel'")?;
    let limit = args["limit"].as_u64().unwrap_or(10);
    let token = creds.get("SLACK_BOT_TOKEN").ok_or("Missing SLACK_BOT_TOKEN")?;

    info!("[skill:slack] Reading {} messages from {}", limit, channel);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?;

    let resp = client.get("https://slack.com/api/conversations.history")
        .header("Authorization", format!("Bearer {}", token))
        .query(&[("channel", channel), ("limit", &limit.to_string())])
        .send()
        .await?;

    let body: serde_json::Value = resp.json().await?;

    if body["ok"].as_bool().unwrap_or(false) {
        let empty_vec = vec![];
        let messages = body["messages"].as_array().unwrap_or(&empty_vec);
        let mut output = format!("Last {} messages from {}:\n\n", messages.len(), channel);
        for (i, msg) in messages.iter().enumerate() {
            let user = msg["user"].as_str().unwrap_or("?");
            let text = msg["text"].as_str().unwrap_or("");
            let ts = msg["ts"].as_str().unwrap_or("");
            output.push_str(&format!("{}. [{}] {}: {}\n", i + 1, ts, user, text));
        }
        Ok(output)
    } else {
        let err = body["error"].as_str().unwrap_or("unknown error");
        Err(format!("Slack API error: {}", err).into())
    }
}
