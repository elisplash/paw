// Paw Agent Engine — fetch tool
// HTTP requests to any URL.

use crate::atoms::types::*;
use log::info;
use std::time::Duration;
use tauri::Manager;
use crate::atoms::error::EngineResult;

pub fn definitions() -> Vec<ToolDefinition> {
    vec![ToolDefinition {
        tool_type: "function".into(),
        function: FunctionDefinition {
            name: "fetch".into(),
            description: "Make an HTTP request to any URL. Returns the response body. Use for API calls, web scraping, downloading content.".into(),
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
                    "body": { "type": "string", "description": "Request body (for POST/PUT/PATCH)" }
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
        "fetch" => Some(execute_fetch(args, app_handle).await.map_err(|e| e.to_string())),
        _ => None,
    }
}

async fn execute_fetch(args: &serde_json::Value, app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let url = args["url"].as_str().ok_or("fetch: missing 'url' argument")?;
    let method = args["method"].as_str().unwrap_or("GET");

    info!("[engine] fetch: {} {}", method, url);

    // Network policy enforcement
    if let Some(state) = app_handle.try_state::<crate::engine::state::EngineState>() {
        if let Ok(Some(policy_json)) = state.store.get_config("network_policy") {
            if let Ok(policy) = serde_json::from_str::<crate::commands::browser::NetworkPolicy>(&policy_json) {
                let domain = crate::commands::browser::extract_domain_from_url(url);
                if policy.blocked_domains.iter().any(|d| crate::commands::browser::domain_matches_pub(&domain, d)) {
                    return Err(format!("Network policy: domain '{}' is blocked", domain).into());
                }
                if policy.enabled {
                    let allowed = policy.allowed_domains.iter().any(|d| crate::commands::browser::domain_matches_pub(&domain, d));
                    if !allowed {
                        return Err(format!("Network policy: domain '{}' is not in the allowlist", domain).into());
                    }
                }
            }
        }
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    // ── Retry loop for transient errors ──────────────────────────────
    use crate::engine::http::{MAX_RETRIES, is_retryable_status, retry_delay, parse_retry_after};

    let mut last_err: Option<String> = None;
    let mut response_result: Option<(u16, String)> = None;

    for attempt in 0..=MAX_RETRIES {
        // Rebuild the request each attempt (RequestBuilder is not Clone)
        let mut req = match method.to_uppercase().as_str() {
            "POST"   => client.post(url),
            "PUT"    => client.put(url),
            "PATCH"  => client.patch(url),
            "DELETE" => client.delete(url),
            "HEAD"   => client.head(url),
            _        => client.get(url),
        };
        if let Some(headers) = args["headers"].as_object() {
            for (key, value) in headers {
                if let Some(v) = value.as_str() {
                    req = req.header(key.as_str(), v);
                }
            }
        }
        if let Some(body) = args["body"].as_str() {
            req = req.body(body.to_string());
        }

        match req.send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let retry_after = resp.headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(parse_retry_after);

                if is_retryable_status(status) && attempt < MAX_RETRIES {
                    log::warn!("[fetch] Retryable status {} on attempt {}, backing off", status, attempt + 1);
                    retry_delay(attempt, retry_after).await;
                    continue;
                }

                let body = resp.text().await.unwrap_or_else(|e| format!("(body read error: {})", e));
                response_result = Some((status, body));
                break;
            }
            Err(e) => {
                if attempt < MAX_RETRIES && (e.is_timeout() || e.is_connect()) {
                    log::warn!("[fetch] Transport error on attempt {}: {} — retrying", attempt + 1, e);
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
        None => return Err(format!("fetch failed after retries: {}", last_err.unwrap_or_default()).into()),
    };

    const MAX_BODY: usize = 50_000;
    let truncated = if body.len() > MAX_BODY {
        format!("{}...\n[truncated, {} total bytes]", &body[..MAX_BODY], body.len())
    } else {
        body
    };

    Ok(format!("HTTP {} {}\n\n{}", status, if status < 400 { "OK" } else { "Error" }, truncated))
}
