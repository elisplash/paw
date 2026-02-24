// Paw Agent Engine — discord_setup_channels tool
//
// Creates Discord categories and channels directly via the REST API.
// This bypasses the model having to make 20+ sequential fetch calls —
// the agent calls this tool ONCE with the full channel structure, and
// Rust handles all the API calls server-side in a loop.

use crate::atoms::types::*;
use crate::atoms::error::EngineResult;
use crate::engine::state::EngineState;
use log::{info, warn};
use serde_json::{json, Value};
use tauri::Manager;
use std::time::Duration;

const DISCORD_API: &str = "https://discord.com/api/v10";

pub fn definitions() -> Vec<ToolDefinition> {
    vec![ToolDefinition {
        tool_type: "function".into(),
        function: FunctionDefinition {
            name: "discord_setup_channels".into(),
            description: "Create multiple Discord categories and channels in one call. \
                Pass an array of categories, each with a name and list of channels. \
                Channels default to text (type 0). Set type to 2 for voice channels. \
                The tool handles all API calls and returns a summary of what was created.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "server_id": {
                        "type": "string",
                        "description": "The Discord server (guild) ID"
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
    }]
}

pub async fn execute(
    name: &str,
    args: &Value,
    app_handle: &tauri::AppHandle,
) -> Option<Result<String, String>> {
    match name {
        "discord_setup_channels" => Some(execute_setup(args, app_handle).await.map_err(|e| e.to_string())),
        _ => None,
    }
}

async fn execute_setup(args: &Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let server_id = args["server_id"].as_str()
        .ok_or("discord_setup_channels: missing 'server_id'")?;
    let categories = args["categories"].as_array()
        .ok_or("discord_setup_channels: missing 'categories' array")?;

    // Get bot token from skill vault
    let token = {
        let state = app_handle.try_state::<EngineState>()
            .ok_or("Engine state not available")?;
        let creds = crate::engine::skills::get_skill_credentials(&state.store, "discord")
            .map_err(|e| format!("Failed to get Discord credentials: {}", e))?;
        creds.get("DISCORD_BOT_TOKEN")
            .cloned()
            .ok_or("DISCORD_BOT_TOKEN not found in skill vault")?
    };

    if token.is_empty() {
        return Err("Discord bot token is empty".into());
    }

    let client = reqwest::Client::new();
    let mut results: Vec<String> = Vec::new();
    let mut created_count = 0;
    let mut error_count = 0;

    for category in categories {
        let cat_name = category["name"].as_str().unwrap_or("Unnamed");
        let channels = category["channels"].as_array();

        // Create the category (type 4)
        info!("[discord_setup] Creating category: {}", cat_name);
        let cat_body = json!({
            "name": cat_name,
            "type": 4
        });

        let cat_result = create_channel(&client, &token, server_id, &cat_body).await;
        let category_id = match cat_result {
            Ok(id) => {
                results.push(format!("✅ Category '{}' created ({})", cat_name, id));
                created_count += 1;
                id
            }
            Err(e) => {
                results.push(format!("❌ Category '{}' failed: {}", cat_name, e));
                error_count += 1;
                continue; // Skip channels if category creation failed
            }
        };

        // Small delay to respect rate limits
        tokio::time::sleep(Duration::from_millis(300)).await;

        // Create channels inside this category
        if let Some(channels) = channels {
            for channel in channels {
                let ch_name = channel["name"].as_str().unwrap_or("unnamed");
                let ch_type = channel["type"].as_i64().unwrap_or(0); // 0=text, 2=voice
                let ch_topic = channel["topic"].as_str();

                info!("[discord_setup] Creating channel: {} (type={}) in category {}", ch_name, ch_type, cat_name);

                let mut ch_body = json!({
                    "name": ch_name,
                    "type": ch_type,
                    "parent_id": category_id
                });
                if let Some(topic) = ch_topic {
                    ch_body["topic"] = json!(topic);
                }

                match create_channel(&client, &token, server_id, &ch_body).await {
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

                // Rate limit delay between channels
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
        }
    }

    let summary = format!(
        "Discord server setup complete!\n\
        Created: {} | Failed: {}\n\n{}",
        created_count, error_count, results.join("\n")
    );

    info!("[discord_setup] Done: {} created, {} errors", created_count, error_count);
    Ok(summary)
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
