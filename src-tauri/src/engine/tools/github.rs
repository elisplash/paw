// Paw Agent Engine — GitHub tool
// github_api

use crate::atoms::types::*;
use log::info;
use std::time::Duration;
use crate::atoms::error::EngineResult;

pub fn definitions() -> Vec<ToolDefinition> {
    vec![ToolDefinition {
        tool_type: "function".into(),
        function: FunctionDefinition {
            name: "github_api".into(),
            description: "Make a GitHub API call. Credentials are stored securely. Common operations: list repos, create issues, list PRs, read files.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "endpoint": { "type": "string", "description": "GitHub API endpoint path (e.g. /repos/owner/repo/issues)" },
                    "method": { "type": "string", "enum": ["GET", "POST", "PUT", "PATCH", "DELETE"], "description": "HTTP method (default: GET)" },
                    "body": { "type": "object", "description": "JSON body for POST/PUT/PATCH requests" }
                },
                "required": ["endpoint"]
            }),
        },
    }]
}

pub async fn execute(
    name: &str,
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
) -> Option<Result<String, String>> {
    if name != "github_api" { return None; }
    let creds = match super::get_skill_creds("github", app_handle) {
        Ok(c) => c,
        Err(e) => return Some(Err(e.to_string())),
    };
    Some(execute_github_api(args, &creds).await.map_err(|e| e.to_string()))
}

async fn execute_github_api(
    args: &serde_json::Value,
    creds: &std::collections::HashMap<String, String>,
) -> EngineResult<String> {
    let endpoint = args["endpoint"].as_str().ok_or("github_api: missing 'endpoint'")?;
    let method = args["method"].as_str().unwrap_or("GET");
    let token = creds.get("GITHUB_TOKEN").ok_or("Missing GITHUB_TOKEN")?;

    let url = if endpoint.starts_with("https://") {
        endpoint.to_string()
    } else {
        format!("https://api.github.com{}", if endpoint.starts_with('/') { endpoint.to_string() } else { format!("/{}", endpoint) })
    };

    info!("[skill:github] {} {}", method, url);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    let mut request = match method.to_uppercase().as_str() {
        "POST"   => client.post(&url),
        "PUT"    => client.put(&url),
        "PATCH"  => client.patch(&url),
        "DELETE" => client.delete(&url),
        _        => client.get(&url),
    };

    request = request
        .header("Authorization", format!("Bearer {}", token))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "Paw-Agent/1.0")
        .header("X-GitHub-Api-Version", "2022-11-28");

    if let Some(body) = args.get("body") {
        if !body.is_null() {
            request = request.json(body);
        }
    }

    let resp = request.send().await?;
    let status = resp.status().as_u16();
    let body = resp.text().await?;

    let truncated = if body.len() > 30_000 {
        format!("{}...\n[truncated, {} total bytes]", &body[..30_000], body.len())
    } else {
        body
    };

    Ok(format!("GitHub API {} {} → {}\n\n{}", method, endpoint, status, truncated))
}
