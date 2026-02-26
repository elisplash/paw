// Paw Agent Engine — fetch tool
// HTTP requests to any URL.

use crate::atoms::error::EngineResult;
use crate::atoms::types::*;
use log::info;
use std::time::Duration;
use tauri::Manager;

pub fn definitions() -> Vec<ToolDefinition> {
    vec![ToolDefinition {
        tool_type: "function".into(),
        function: FunctionDefinition {
            name: "fetch".into(),
            description: "Make an HTTP request to any URL. Returns the response body. Use for web page scraping or downloading public content. NEVER use fetch to call external service APIs (Slack, Discord, GitHub, etc.) — use mcp_* tools for those.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "The URL to fetch" },
                    "method": {
                        "type": "string",
                        "enum": ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"],
                        "description": "HTTP method (default: GET)"
                    },
                    "headers": { "type": "object", "description": "HTTP headers as key-value pairs" },
                    "body": { "description": "Request body for POST/PUT/PATCH. Pass a JSON object directly (preferred) or a JSON string." }
                },
                "required": ["url"]
            }),
        },
    }]
}

pub async fn execute(
    name: &str,
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
) -> Option<Result<String, String>> {
    match name {
        "fetch" => Some(
            execute_fetch(args, app_handle)
                .await
                .map_err(|e| e.to_string()),
        ),
        _ => None,
    }
}

async fn execute_fetch(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
) -> EngineResult<String> {
    let url = args["url"]
        .as_str()
        .ok_or("fetch: missing 'url' argument")?;
    let method = args["method"].as_str().unwrap_or("GET");

    info!("[engine] fetch: {} {}", method, url);

    // Network policy enforcement
    if let Some(state) = app_handle.try_state::<crate::engine::state::EngineState>() {
        if let Ok(Some(policy_json)) = state.store.get_config("network_policy") {
            if let Ok(policy) =
                serde_json::from_str::<crate::commands::browser::NetworkPolicy>(&policy_json)
            {
                let domain = crate::commands::browser::extract_domain_from_url(url);
                if policy
                    .blocked_domains
                    .iter()
                    .any(|d| crate::commands::browser::domain_matches_pub(&domain, d))
                {
                    return Err(format!("Network policy: domain '{}' is blocked", domain).into());
                }
                if policy.enabled {
                    let allowed = policy
                        .allowed_domains
                        .iter()
                        .any(|d| crate::commands::browser::domain_matches_pub(&domain, d));
                    if !allowed {
                        return Err(format!(
                            "Network policy: domain '{}' is not in the allowlist",
                            domain
                        )
                        .into());
                    }
                }
            }
        }
    }

    // ── Auto-inject credentials for known API domains ─────────────────
    // If the agent calls a Discord API URL without an Authorization header,
    // automatically inject the bot token from the skill vault. This prevents
    // 401 errors when the LLM forgets to include the header (which happens
    // frequently after context truncation).
    let mut injected_headers: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    let has_auth_header = args["headers"]
        .as_object()
        .map(|h| h.keys().any(|k| k.eq_ignore_ascii_case("authorization")))
        .unwrap_or(false);

    if !has_auth_header && url.contains("discord.com/api") {
        if let Some(state) = app_handle.try_state::<crate::engine::state::EngineState>() {
            if let Ok(creds) = crate::engine::skills::get_skill_credentials(&state.store, "discord")
            {
                if let Some(token) = creds.get("DISCORD_BOT_TOKEN") {
                    if !token.is_empty() {
                        info!("[fetch] Auto-injecting Discord bot Authorization header");
                        injected_headers.insert("Authorization".into(), format!("Bot {}", token));
                    }
                }
            }
        }
    }

    // Auto-inject Content-Type for Discord API mutations when body is present
    if url.contains("discord.com/api") && args["body"].is_string() {
        let has_ct = args["headers"]
            .as_object()
            .map(|h| h.keys().any(|k| k.eq_ignore_ascii_case("content-type")))
            .unwrap_or(false);
        if !has_ct {
            injected_headers.insert("Content-Type".into(), "application/json".into());
        }
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    // ── Retry loop for transient errors ──────────────────────────────
    use crate::engine::http::{is_retryable_status, parse_retry_after, retry_delay, MAX_RETRIES};

    let mut last_err: Option<String> = None;
    let mut response_result: Option<(u16, String)> = None;

    for attempt in 0..=MAX_RETRIES {
        // Rebuild the request each attempt (RequestBuilder is not Clone)
        let mut req = match method.to_uppercase().as_str() {
            "POST" => client.post(url),
            "PUT" => client.put(url),
            "PATCH" => client.patch(url),
            "DELETE" => client.delete(url),
            "HEAD" => client.head(url),
            _ => client.get(url),
        };
        // Apply auto-injected credential headers first (so explicit headers override)
        for (key, value) in &injected_headers {
            req = req.header(key.as_str(), value.as_str());
        }
        if let Some(headers) = args["headers"].as_object() {
            for (key, value) in headers {
                if let Some(v) = value.as_str() {
                    req = req.header(key.as_str(), v);
                }
            }
        }
        // Accept body as either a JSON string or a JSON object/array.
        // When the model passes an object (e.g. {"name":"foo","type":0}),
        // we serialize it to a JSON string. This avoids the double-escaping
        // problem that causes MALFORMED_FUNCTION_CALL errors in Gemini.
        if let Some(body_str) = args["body"].as_str() {
            req = req.body(body_str.to_string());
        } else if args["body"].is_object() || args["body"].is_array() {
            req = req.body(serde_json::to_string(&args["body"]).unwrap_or_default());
        }

        match req.send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let retry_after = resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(parse_retry_after);

                if is_retryable_status(status) && attempt < MAX_RETRIES {
                    log::warn!(
                        "[fetch] Retryable status {} on attempt {}, backing off",
                        status,
                        attempt + 1
                    );
                    retry_delay(attempt, retry_after).await;
                    continue;
                }

                let body = resp
                    .text()
                    .await
                    .unwrap_or_else(|e| format!("(body read error: {})", e));
                response_result = Some((status, body));
                break;
            }
            Err(e) => {
                if attempt < MAX_RETRIES && (e.is_timeout() || e.is_connect()) {
                    log::warn!(
                        "[fetch] Transport error on attempt {}: {} — retrying",
                        attempt + 1,
                        e
                    );
                    retry_delay(attempt, None).await;
                    continue;
                }
                last_err = Some(e.to_string());
                break;
            }
        }
    }

    let (status, body) = match response_result {
        Some(r) => r,
        None => {
            return Err(format!(
                "fetch failed after retries: {}",
                last_err.unwrap_or_default()
            )
            .into())
        }
    };

    const MAX_BODY: usize = 50_000;
    let truncated = if body.len() > MAX_BODY {
        format!(
            "{}...\n[truncated, {} total bytes]",
            &body[..MAX_BODY],
            body.len()
        )
    } else {
        body
    };

    Ok(format!(
        "HTTP {} {}\n\n{}",
        status,
        if status < 400 { "OK" } else { "Error" },
        truncated
    ))
}
