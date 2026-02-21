// Paw Agent Engine — fetch tool
// HTTP requests to any URL.

use crate::atoms::types::*;
use log::info;
use std::time::Duration;
use tauri::Manager;

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
        "fetch" => Some(execute_fetch(args, app_handle).await),
        _ => None,
    }
}

async fn execute_fetch(args: &serde_json::Value, app_handle: &tauri::AppHandle) -> Result<String, String> {
    let url = args["url"].as_str().ok_or("fetch: missing 'url' argument")?;
    let method = args["method"].as_str().unwrap_or("GET");

    info!("[engine] fetch: {} {}", method, url);

    // Network policy enforcement
    if let Some(state) = app_handle.try_state::<crate::engine::state::EngineState>() {
        if let Ok(Some(policy_json)) = state.store.get_config("network_policy") {
            if let Ok(policy) = serde_json::from_str::<crate::commands::browser::NetworkPolicy>(&policy_json) {
                let domain = crate::commands::browser::extract_domain_from_url(url);
                if policy.blocked_domains.iter().any(|d| crate::commands::browser::domain_matches_pub(&domain, d)) {
                    return Err(format!("Network policy: domain '{}' is blocked", domain));
                }
                if policy.enabled {
                    let allowed = policy.allowed_domains.iter().any(|d| crate::commands::browser::domain_matches_pub(&domain, d));
                    if !allowed {
                        return Err(format!("Network policy: domain '{}' is not in the allowlist", domain));
                    }
                }
            }
        }
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let mut request = match method.to_uppercase().as_str() {
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
                request = request.header(key.as_str(), v);
            }
        }
    }

    if let Some(body) = args["body"].as_str() {
        request = request.body(body.to_string());
    }

    let response = request.send().await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    let status = response.status().as_u16();
    let body = response.text().await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    const MAX_BODY: usize = 50_000;
    let truncated = if body.len() > MAX_BODY {
        format!("{}...\n[truncated, {} total bytes]", &body[..MAX_BODY], body.len())
    } else {
        body
    };

    Ok(format!("HTTP {} {}\n\n{}", status, if status < 400 { "OK" } else { "Error" }, truncated))
}
