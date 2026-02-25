// commands/n8n.rs — Tauri IPC commands for n8n integration

use crate::engine::channels;
use serde::{Deserialize, Serialize};

// ── Config ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct N8nConfig {
    pub url: String,
    pub api_key: String,
    pub enabled: bool,
    pub auto_discover: bool,
    pub mcp_mode: bool,
}

// ── Test-connection result ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct N8nTestResult {
    pub connected: bool,
    pub version: String,
    pub workflow_count: usize,
    pub error: Option<String>,
}

// ── Workflow summary (Phase 2 prep) ────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct N8nWorkflow {
    pub id: String,
    pub name: String,
    pub active: bool,
    pub tags: Vec<String>,
    pub nodes: Vec<String>,
    #[serde(rename = "triggerType", default)]
    pub trigger_type: String,
    #[serde(rename = "createdAt", default)]
    pub created_at: String,
    #[serde(rename = "updatedAt", default)]
    pub updated_at: String,
}

// ── Error classification ───────────────────────────────────────────────

/// Produce a user-friendly message from a reqwest error.
fn classify_reqwest_error(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        return "Connection timed out — verify the URL is reachable and n8n is running.".into();
    }
    if e.is_connect() {
        let inner = e.to_string().to_lowercase();
        if inner.contains("ssl") || inner.contains("tls") || inner.contains("certificate") {
            return "SSL/TLS certificate verification failed — if using a self-signed certificate, check your system trust store.".into();
        }
        if inner.contains("dns") || inner.contains("resolve") || inner.contains("no such host") {
            return "DNS resolution failed — could not resolve the hostname. Check the URL for typos.".to_string();
        }
        if inner.contains("refused") {
            return "Connection refused — is n8n running on this address and port?".into();
        }
        return format!("Connection failed: {}", e);
    }
    if e.is_request() {
        return format!("Invalid request — check the URL format: {}", e);
    }
    format!("Connection error: {}", e)
}

// ── Commands ───────────────────────────────────────────────────────────

/// Read n8n configuration.
#[tauri::command]
pub fn engine_n8n_get_config(app_handle: tauri::AppHandle) -> Result<N8nConfig, String> {
    channels::load_channel_config::<N8nConfig>(&app_handle, "n8n_config").map_err(|e| e.to_string())
}

/// Save n8n configuration.
#[tauri::command]
pub fn engine_n8n_set_config(
    app_handle: tauri::AppHandle,
    config: N8nConfig,
) -> Result<(), String> {
    channels::save_channel_config(&app_handle, "n8n_config", &config).map_err(|e| e.to_string())
}

/// Test the n8n connection by pinging /api/v1/workflows.
#[tauri::command]
pub async fn engine_n8n_test_connection(
    url: String,
    api_key: String,
) -> Result<N8nTestResult, String> {
    // Normalise URL (strip trailing slash)
    let base = url.trim_end_matches('/');

    // Validate URL format
    if !base.starts_with("http://") && !base.starts_with("https://") {
        return Ok(N8nTestResult {
            connected: false,
            version: String::new(),
            workflow_count: 0,
            error: Some("URL must start with http:// or https://".into()),
        });
    }

    let endpoint = format!("{}/api/v1/workflows?limit=1", base);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = match client
        .get(&endpoint)
        .header("X-N8N-API-KEY", &api_key)
        .header("Accept", "application/json")
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            let msg = classify_reqwest_error(&e);
            return Ok(N8nTestResult {
                connected: false,
                version: String::new(),
                workflow_count: 0,
                error: Some(msg),
            });
        }
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Ok(N8nTestResult {
            connected: false,
            version: String::new(),
            workflow_count: 0,
            error: Some(format!("HTTP {}: {}", status.as_u16(), body)),
        });
    }

    // Try to extract version from response headers (n8n sets X-N8N-Version)
    let version = resp
        .headers()
        .get("x-n8n-version")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    // n8n v1 REST: { data: [...], nextCursor: ... }
    let workflow_count = body
        .get("count")
        .or_else(|| body.get("data").and_then(|d| d.as_array().map(|_| d)))
        .map(|v| {
            if let Some(n) = v.as_u64() {
                n as usize
            } else if let Some(arr) = v.as_array() {
                arr.len()
            } else {
                0
            }
        })
        .unwrap_or(0);

    Ok(N8nTestResult {
        connected: true,
        version,
        workflow_count,
        error: None,
    })
}

/// List workflows from the connected n8n instance.
#[tauri::command]
pub async fn engine_n8n_list_workflows(
    app_handle: tauri::AppHandle,
) -> Result<Vec<N8nWorkflow>, String> {
    let config: N8nConfig =
        channels::load_channel_config(&app_handle, "n8n_config").map_err(|e| e.to_string())?;

    if config.url.is_empty() || config.api_key.is_empty() {
        return Err("n8n not configured — set URL and API key first".into());
    }

    let base = config.url.trim_end_matches('/');
    let endpoint = format!("{}/api/v1/workflows", base);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(&endpoint)
        .header("X-N8N-API-KEY", &config.api_key)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("n8n request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!(
            "n8n API error (HTTP {}): {}",
            status.as_u16(),
            body
        ));
    }

    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    let data = body
        .get("data")
        .and_then(|d| d.as_array())
        .cloned()
        .unwrap_or_default();

    let workflows: Vec<N8nWorkflow> = data
        .into_iter()
        .filter_map(|v| {
            let id = v.get("id")?.to_string().trim_matches('"').to_string();
            let name = v
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("Untitled")
                .to_string();
            let active = v.get("active").and_then(|a| a.as_bool()).unwrap_or(false);

            let tags = v
                .get("tags")
                .and_then(|t| t.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|t| {
                            t.get("name")
                                .or_else(|| t.get("id"))
                                .and_then(|n| n.as_str())
                                .map(String::from)
                        })
                        .collect()
                })
                .unwrap_or_default();

            let nodes = v
                .get("nodes")
                .and_then(|n| n.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|n| n.get("type").and_then(|t| t.as_str()).map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            let created_at = v
                .get("createdAt")
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .to_string();
            let updated_at = v
                .get("updatedAt")
                .and_then(|u| u.as_str())
                .unwrap_or("")
                .to_string();

            Some(N8nWorkflow {
                id,
                name,
                active,
                tags,
                nodes,
                trigger_type: String::new(),
                created_at,
                updated_at,
            })
        })
        .collect();

    Ok(workflows)
}

/// Trigger a specific n8n workflow by ID (webhook-trigger or test mode).
#[tauri::command]
pub async fn engine_n8n_trigger_workflow(
    app_handle: tauri::AppHandle,
    workflow_id: String,
    payload: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let config: N8nConfig =
        channels::load_channel_config(&app_handle, "n8n_config").map_err(|e| e.to_string())?;

    if config.url.is_empty() || config.api_key.is_empty() {
        return Err("n8n not configured".into());
    }

    let base = config.url.trim_end_matches('/');
    // Use the webhook-test endpoint for triggering
    let endpoint = format!("{}/api/v1/workflows/{}/execute", base, workflow_id);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let body = payload.unwrap_or(serde_json::json!({}));

    let resp = client
        .post(&endpoint)
        .header("X-N8N-API-KEY", &config.api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("n8n trigger failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let resp_body = resp.text().await.unwrap_or_default();
        return Err(format!(
            "n8n trigger error (HTTP {}): {}",
            status.as_u16(),
            resp_body
        ));
    }

    let result: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok(result)
}
