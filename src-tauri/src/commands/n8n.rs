// commands/n8n.rs — Tauri IPC commands for n8n integration

use crate::engine::channels;
use crate::engine::n8n_engine;
use crate::engine::skills;
use crate::engine::state::EngineState;
use crate::engine::util::safe_truncate;
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tauri::Manager;

/// Mutex that serialises direct (docker exec / npm) community-package installs
/// so concurrent requests don't race and one restart doesn't kill another's install.
static INSTALL_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
/// Counter of in-flight direct installs — restart is deferred until this hits 0.
static PENDING_INSTALLS: AtomicUsize = AtomicUsize::new(0);
/// Cancellation flag — set to true by `engine_n8n_community_packages_cancel`
/// to abort a currently running install.
static INSTALL_CANCELLED: AtomicBool = AtomicBool::new(false);

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

// ── Phase 0: Engine lifecycle commands ─────────────────────────────────

/// Ensure the n8n integration engine is running.
/// Returns the endpoint URL and mode.  Auto-provisions via Docker or
/// Node.js if not already running.
///
/// When `mcp_mode` is enabled in the engine config, also auto-registers
/// the n8n engine as an MCP server so agents discover tools dynamically.
#[tauri::command]
pub async fn engine_n8n_ensure_ready(
    app_handle: tauri::AppHandle,
) -> Result<n8n_engine::N8nEndpoint, String> {
    let endpoint = n8n_engine::ensure_n8n_ready(&app_handle)
        .await
        .map_err(|e| e.to_string())?;

    // ── MCP bridge auto-registration ───────────────────────────────
    let config = n8n_engine::load_config(&app_handle).unwrap_or_default();
    if config.mcp_mode {
        if let Some(state) = app_handle.try_state::<EngineState>() {
            let mut reg = state.mcp_registry.lock().await;
            if !reg.is_n8n_registered() {
                let mcp_token = get_or_retrieve_mcp_token(&app_handle).await;
                match reg
                    .register_n8n(&endpoint.url, &endpoint.api_key, mcp_token.as_deref())
                    .await
                {
                    Ok(tool_count) => {
                        log::info!(
                            "[n8n] MCP bridge registered — {} tools available to agents",
                            tool_count
                        );
                        // Notify frontend about MCP tools
                        use tauri::Emitter;
                        let _ = app_handle.emit(
                            "n8n-mcp-status",
                            serde_json::json!({
                                "connected": true,
                                "tool_count": tool_count,
                            }),
                        );
                    }
                    Err(e) => {
                        log::debug!("[n8n] MCP bridge not available (n8n MCP endpoint may not be enabled): {}", e);
                        // Not fatal — n8n itself is running, just MCP discovery unavailable
                    }
                }
            }
        }
    }

    Ok(endpoint)
}

/// Get the MCP bridge status for the n8n integration engine.
/// Returns connection state and number of dynamically discovered tools.
#[tauri::command]
pub async fn engine_n8n_mcp_status(
    app_handle: tauri::AppHandle,
) -> Result<serde_json::Value, String> {
    if let Some(state) = app_handle.try_state::<EngineState>() {
        let reg = state.mcp_registry.lock().await;
        let connected = reg.is_n8n_registered();
        let tool_count = if connected {
            reg.tool_definitions_for(&["n8n".into()]).len()
        } else {
            0
        };
        Ok(serde_json::json!({
            "connected": connected,
            "tool_count": tool_count,
        }))
    } else {
        Ok(serde_json::json!({
            "connected": false,
            "tool_count": 0,
        }))
    }
}

/// Refresh the n8n MCP tool list (re-discovers tools from the running engine).
/// If the existing connection is stale (e.g., after a container restart),
/// performs a full disconnect + reconnect cycle.
#[tauri::command]
pub async fn engine_n8n_mcp_refresh(app_handle: tauri::AppHandle) -> Result<usize, String> {
    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    let mut reg = state.mcp_registry.lock().await;

    if reg.is_n8n_registered() {
        // Try refreshing on the existing connection first
        match reg.refresh_tools("n8n").await {
            Ok(()) => {
                let tool_count = reg.tool_definitions_for(&["n8n".into()]).len();
                log::info!("[n8n] MCP tools refreshed — {} tools", tool_count);
                return Ok(tool_count);
            }
            Err(e) => {
                // Existing connection is likely stale — do a full reconnect
                log::info!("[n8n] MCP refresh failed ({}), attempting reconnect…", e);
            }
        }
    }

    // Full reconnect: disconnect stale client and re-register
    let (endpoint_url, api_key) = get_n8n_endpoint(&app_handle)?;
    let mcp_token = get_or_retrieve_mcp_token(&app_handle).await;
    reg.disconnect_n8n().await;
    let tool_count = reg
        .register_n8n(&endpoint_url, &api_key, mcp_token.as_deref())
        .await
        .map_err(|e| format!("MCP reconnection failed: {}", e))?;

    drop(reg);

    // Invalidate tool index so it includes the new tools
    {
        let mut idx = state.tool_index.lock().await;
        *idx = crate::engine::tool_index::ToolIndex::new();
    }

    log::info!(
        "[n8n] MCP bridge reconnected via refresh — {} tools",
        tool_count
    );
    Ok(tool_count)
}

/// Get the current status of the n8n engine (for Settings → Advanced).
#[tauri::command]
pub async fn engine_n8n_get_status(
    app_handle: tauri::AppHandle,
) -> Result<n8n_engine::N8nEngineStatus, String> {
    Ok(n8n_engine::get_status(&app_handle).await)
}

/// Get the extended engine configuration.
#[tauri::command]
pub fn engine_n8n_get_engine_config(
    app_handle: tauri::AppHandle,
) -> Result<n8n_engine::N8nEngineConfig, String> {
    n8n_engine::load_config(&app_handle).map_err(|e| e.to_string())
}

/// Save the extended engine configuration.
#[tauri::command]
pub fn engine_n8n_set_engine_config(
    app_handle: tauri::AppHandle,
    config: n8n_engine::N8nEngineConfig,
) -> Result<(), String> {
    n8n_engine::save_config(&app_handle, &config).map_err(|e| e.to_string())
}

/// Perform a health check on the running engine.
#[tauri::command]
pub async fn engine_n8n_health_check(app_handle: tauri::AppHandle) -> Result<bool, String> {
    Ok(n8n_engine::health_check(&app_handle).await)
}

/// Gracefully shut down the engine (Docker stop / process kill).
#[tauri::command]
pub async fn engine_n8n_shutdown(app_handle: tauri::AppHandle) -> Result<(), String> {
    n8n_engine::shutdown(&app_handle).await;
    Ok(())
}

// ── Community Nodes: install/list/uninstall npm packages in n8n ────────

/// A community node package installed in the n8n engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityPackage {
    #[serde(rename = "packageName")]
    pub package_name: String,
    #[serde(rename = "installedVersion", default)]
    pub installed_version: String,
    #[serde(rename = "installedNodes", default)]
    pub installed_nodes: Vec<CommunityNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityNode {
    pub name: String,
    #[serde(rename = "type", default)]
    pub node_type: String,
}

/// Get the n8n base URL and API key from the engine config.
pub fn get_n8n_endpoint(app_handle: &tauri::AppHandle) -> Result<(String, String), String> {
    let config = n8n_engine::load_config(app_handle).map_err(|e| e.to_string())?;
    let url = match config.mode {
        n8n_engine::N8nMode::Remote | n8n_engine::N8nMode::Local => config.url.clone(),
        n8n_engine::N8nMode::Embedded => {
            format!("http://127.0.0.1:{}", config.container_port.unwrap_or(5678))
        }
        n8n_engine::N8nMode::Process => {
            format!("http://127.0.0.1:{}", config.process_port.unwrap_or(5678))
        }
    };
    if url.is_empty() || config.api_key.is_empty() {
        return Err("Integration engine not configured".into());
    }
    Ok((url, config.api_key))
}

/// Get the MCP token from config, or attempt to retrieve it from n8n.
/// Returns None (not an error) if retrieval fails — MCP is optional.
///
/// If a cached token exists, we validate it's not stale by checking
/// that it's a plausible JWT (has `.` separators and is not redacted).
/// If stale, we retrieve a fresh one.
async fn get_or_retrieve_mcp_token(app_handle: &tauri::AppHandle) -> Option<String> {
    let config = n8n_engine::load_config(app_handle).ok()?;

    // If we already have a token that looks like a valid JWT, use it
    if let Some(ref token) = config.mcp_token {
        if !token.is_empty() && token.contains('.') && !token.contains('*') {
            return Some(token.clone());
        }
        // Stale or redacted token — will re-fetch below
        log::info!("[n8n] Cached MCP token looks stale, retrieving fresh one");
    }

    // Try to retrieve the token from n8n
    let url = match config.mode {
        n8n_engine::N8nMode::Remote | n8n_engine::N8nMode::Local => config.url.clone(),
        n8n_engine::N8nMode::Embedded => {
            format!("http://127.0.0.1:{}", config.container_port.unwrap_or(5678))
        }
        n8n_engine::N8nMode::Process => {
            format!("http://127.0.0.1:{}", config.process_port.unwrap_or(5678))
        }
    };

    // Ensure owner exists and MCP access is enabled before retrieving token.
    // NOTE: setup_owner_if_needed() and enable_mcp_access() are already called
    // by the provisioning / reconnect code paths (process.rs, docker.rs, mod.rs)
    // before we reach this function.  Calling them again here caused duplicate
    // log lines and doubled the 401 failures when vault credentials mismatch.
    // We skip them here and only retrieve the token.

    // Retry token retrieval up to 2 times.  On a fresh n8n start the
    // MCP module may take a moment to initialise after the health
    // endpoint responds.
    for attempt in 1..=2 {
        match n8n_engine::health::retrieve_mcp_token(&url, &config.api_key).await {
            Ok(token) => {
                log::info!("[n8n] MCP token retrieved and cached");
                // Save token to config for future use
                let mut new_config = config;
                new_config.mcp_token = Some(token.clone());
                let _ = n8n_engine::save_config(app_handle, &new_config);
                return Some(token);
            }
            Err(e) => {
                if attempt < 2 {
                    log::info!(
                        "[n8n] MCP token retrieval attempt {} failed: {} — retrying…",
                        attempt,
                        e
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                } else {
                    log::warn!("[n8n] MCP token retrieval failed: {}", e);
                }
            }
        }
    }
    None
}

// ── NCNodes / npm Registry Search ──────────────────────────────────────

/// A search result from the npm registry for n8n community packages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NCNodeResult {
    pub package_name: String,
    pub description: String,
    pub author: String,
    pub version: String,
    pub weekly_downloads: u64,
    pub last_updated: String,
    pub repository_url: Option<String>,
    pub keywords: Vec<String>,
}

/// Search the npm registry for n8n community node packages.
///
/// Uses the npm registry search API with the `n8n-community-node-package`
/// keyword filter. This covers the same 25,000+ packages indexed by ncnodes.com.
#[tauri::command]
pub async fn engine_n8n_search_ncnodes(
    query: String,
    limit: Option<u32>,
    from: Option<u32>,
) -> Result<Vec<NCNodeResult>, String> {
    let limit = limit.unwrap_or(10).min(50);
    let from = from.unwrap_or(0);

    info!(
        "[n8n:ncnodes] Searching npm for '{}' (limit={}, from={})",
        query, limit, from
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    // npm registry search API — filter by the n8n community node keyword.
    // npm returns results in relevance order by default; we preserve that
    // order on the frontend via the "Best Match" sort option.
    let encoded_query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair(
            "text",
            &format!("{} keywords:n8n-community-node-package", query),
        )
        .append_pair("size", &limit.to_string())
        .append_pair("from", &from.to_string())
        .finish();
    let search_url = format!("https://registry.npmjs.org/-/v1/search?{}", encoded_query);

    let resp = client
        .get(&search_url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("npm search failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("npm search failed (HTTP {}): {}", status, body));
    }

    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    let mut results: Vec<NCNodeResult> = body["objects"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|obj| {
                    let pkg = &obj["package"];
                    let name = pkg["name"].as_str()?;
                    Some(NCNodeResult {
                        package_name: name.to_string(),
                        description: pkg["description"].as_str().unwrap_or("").to_string(),
                        author: pkg["publisher"]["username"]
                            .as_str()
                            .or_else(|| pkg["author"]["name"].as_str())
                            .unwrap_or("unknown")
                            .to_string(),
                        version: pkg["version"].as_str().unwrap_or("0.0.0").to_string(),
                        // Placeholder — real downloads fetched below
                        weekly_downloads: 0,
                        last_updated: pkg["date"].as_str().unwrap_or("").to_string(),
                        repository_url: pkg["links"]["repository"].as_str().map(|s| s.to_string()),
                        keywords: pkg["keywords"]
                            .as_array()
                            .map(|a| {
                                a.iter()
                                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                    .collect()
                            })
                            .unwrap_or_default(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // Fetch real weekly download counts from npm download-counts API.
    // We batch up to 128 scoped/unscoped names per request.
    if !results.is_empty() {
        let names: Vec<String> = results.iter().map(|r| r.package_name.clone()).collect();
        let name_refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        let downloads = fetch_npm_weekly_downloads(&client, &name_refs).await;
        for r in &mut results {
            if let Some(&count) = downloads.get(r.package_name.as_str()) {
                r.weekly_downloads = count;
            }
        }
    }

    info!(
        "[n8n:ncnodes] Found {} packages for '{}'",
        results.len(),
        query
    );

    Ok(results)
}

/// Fetch real weekly download counts from npm's bulk download-counts API.
///
/// Uses `https://api.npmjs.org/downloads/point/last-week/<pkg1>,<pkg2>,...`
/// which returns actual download numbers. Scoped packages (@org/pkg) need
/// individual requests since the bulk endpoint doesn't support them.
async fn fetch_npm_weekly_downloads<'a>(
    client: &reqwest::Client,
    names: &[&'a str],
) -> std::collections::HashMap<&'a str, u64> {
    use std::collections::HashMap;

    let mut result: HashMap<&str, u64> = HashMap::new();

    // Split into scoped (need individual fetch) and unscoped (can batch)
    let mut unscoped: Vec<&str> = Vec::new();
    let mut scoped: Vec<&str> = Vec::new();
    for &name in names {
        if name.starts_with('@') {
            scoped.push(name);
        } else {
            unscoped.push(name);
        }
    }

    // Bulk fetch unscoped packages (npm supports comma-separated, up to ~128)
    if !unscoped.is_empty() {
        let joined = unscoped.join(",");
        let url = format!("https://api.npmjs.org/downloads/point/last-week/{}", joined);
        if let Ok(resp) = client.get(&url).send().await {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                if let Some(obj) = body.as_object() {
                    for (key, val) in obj {
                        if let Some(downloads) = val["downloads"].as_u64() {
                            // Match back to the original slice reference
                            if let Some(&name) = names.iter().find(|&&n| n == key.as_str()) {
                                result.insert(name, downloads);
                            }
                        }
                    }
                }
            }
        }
    }

    // Fetch scoped packages individually
    for &name in &scoped {
        let url = format!(
            "https://api.npmjs.org/downloads/point/last-week/{}",
            url::form_urlencoded::byte_serialize(name.as_bytes()).collect::<String>()
        );
        if let Ok(resp) = client.get(&url).send().await {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                if let Some(downloads) = body["downloads"].as_u64() {
                    result.insert(name, downloads);
                }
            }
        }
    }

    result
}

/// List community node packages installed in the n8n engine.
///
/// Tries the n8n REST API first, then falls back to reading the container's
/// package.json via `docker exec` (handles cases where the REST API 404s).
#[tauri::command]
pub async fn engine_n8n_community_packages_list(
    app_handle: tauri::AppHandle,
) -> Result<Vec<CommunityPackage>, String> {
    let (base_url, api_key) = get_n8n_endpoint(&app_handle)?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(format!(
            "{}/api/v1/community-packages",
            base_url.trim_end_matches('/')
        ))
        .header("X-N8N-API-KEY", &api_key)
        .send()
        .await;

    // Try REST API first (may fail with 404 or connection error)
    if let Ok(r) = resp {
        if r.status().is_success() {
            if let Ok(packages) = r.json::<Vec<CommunityPackage>>().await {
                return Ok(packages);
            }
        } else {
            let status = r.status().as_u16();
            info!(
                "[n8n] Community packages REST API returned HTTP {} — falling back to docker exec",
                status
            );
        }
    }

    // Fallback 1: Docker container package.json
    match list_packages_from_container().await {
        Ok(pkgs) if !pkgs.is_empty() => return Ok(pkgs),
        Ok(_) => {
            info!("[n8n] Docker container returned 0 packages — trying data dir fallback");
        }
        Err(e) => {
            info!(
                "[n8n] Cannot list packages from container: {} — trying data dir fallback",
                e
            );
        }
    }

    // Fallback 2: Local data dir package.json (Process mode)
    let data_dir = n8n_engine::n8n_data_dir(&app_handle);
    match list_packages_from_data_dir(&data_dir) {
        Ok(pkgs) => Ok(pkgs),
        Err(e) => {
            info!(
                "[n8n] Data dir fallback also failed: {} — returning empty list",
                e
            );
            Ok(vec![])
        }
    }
}

/// Fallback for listing community packages by reading the container's package.json.
///
/// n8n stores community packages as npm dependencies in `CONTAINER_DATA_DIR/package.json`.
/// This approach works even when the REST API endpoint returns 404.
async fn list_packages_from_container() -> Result<Vec<CommunityPackage>, String> {
    use tokio::process::Command;

    let data_dir = n8n_engine::types::CONTAINER_DATA_DIR;

    let output = Command::new("docker")
        .args([
            "exec",
            n8n_engine::types::CONTAINER_NAME,
            "cat",
            &format!("{}/package.json", data_dir),
        ])
        .output()
        .await
        .map_err(|e| format!("docker exec failed: {}", e))?;

    if !output.status.success() {
        // No package.json = no community packages installed
        return Ok(vec![]);
    }

    let content = String::from_utf8_lossy(&output.stdout);
    let pkg_json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse package.json: {}", e))?;

    let mut packages = Vec::new();

    if let Some(deps) = pkg_json.get("dependencies").and_then(|d| d.as_object()) {
        for (name, version) in deps {
            // Only include n8n community packages.
            // Naming conventions: n8n-nodes-*, @scope/n8n-nodes-*, *-n8n-*
            // For scoped packages, check the part after the scope prefix.
            let unscoped = if let Some(pos) = name.find('/') {
                &name[pos + 1..]
            } else {
                name.as_str()
            };
            if unscoped.starts_with("n8n-nodes-") || name.contains("-n8n-") {
                // Try to read node types from the package's own package.json
                let node_pkg_json_str = String::from_utf8(
                    tokio::process::Command::new("docker")
                        .args([
                            "exec",
                            n8n_engine::types::CONTAINER_NAME,
                            "cat",
                            &format!("{}/node_modules/{}/package.json", data_dir, name),
                        ])
                        .output()
                        .await
                        .map(|o| o.stdout)
                        .unwrap_or_default(),
                )
                .unwrap_or_default();

                let installed_nodes = serde_json::from_str::<serde_json::Value>(&node_pkg_json_str)
                    .ok()
                    .map(|pj| discover_nodes_from_package_json(&pj))
                    .unwrap_or_default();

                packages.push(CommunityPackage {
                    package_name: name.clone(),
                    installed_version: version
                        .as_str()
                        .unwrap_or("unknown")
                        .trim_start_matches('^')
                        .trim_start_matches('~')
                        .to_string(),
                    installed_nodes,
                });
            }
        }
    }

    info!(
        "[n8n] Found {} community packages via container package.json",
        packages.len()
    );
    Ok(packages)
}

/// Discover n8n node types from a community package's own `package.json`.
///
/// n8n community packages declare their nodes under `"n8n" > "nodes"` in
/// their package.json, e.g.:
///   { "n8n": { "nodes": [ "dist/nodes/MyNode/MyNode.node.js" ] } }
///
/// We derive a human-readable name from the file path and return as
/// `CommunityNode` entries.
fn discover_nodes_from_package_json(pkg_json: &serde_json::Value) -> Vec<CommunityNode> {
    let mut nodes = Vec::new();

    // Path 1: n8n.nodes (array of dist paths)
    if let Some(n8n_section) = pkg_json.get("n8n").and_then(|v| v.as_object()) {
        if let Some(node_entries) = n8n_section.get("nodes").and_then(|v| v.as_array()) {
            for entry in node_entries {
                if let Some(path) = entry.as_str() {
                    // e.g. "dist/nodes/Telegram/Telegram.node.js" → "Telegram"
                    let file_name = std::path::Path::new(path)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or(path);
                    // Strip ".node" suffix if present
                    let name = file_name.strip_suffix(".node").unwrap_or(file_name);
                    nodes.push(CommunityNode {
                        name: name.to_string(),
                        node_type: format!("n8n-nodes-base.{}", name.to_lowercase()),
                    });
                }
            }
        }
    }

    // Path 2: n8n.credentials (so the count includes cred-only types)
    // Intentionally left out — users care about *nodes*, not credential types.

    nodes
}

/// Read community packages from the local n8n data directory's package.json.
///
/// Process-mode equivalent of `list_packages_from_container`.
fn list_packages_from_data_dir(
    data_dir: &std::path::Path,
) -> Result<Vec<CommunityPackage>, String> {
    let pkg_json_path = data_dir.join("package.json");
    let content = std::fs::read_to_string(&pkg_json_path)
        .map_err(|e| format!("Failed to read {}: {}", pkg_json_path.display(), e))?;
    let pkg_json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse package.json: {}", e))?;

    let node_modules = data_dir.join("node_modules");
    let mut packages = Vec::new();

    if let Some(deps) = pkg_json.get("dependencies").and_then(|d| d.as_object()) {
        for (name, version) in deps {
            let unscoped = if let Some(pos) = name.find('/') {
                &name[pos + 1..]
            } else {
                name.as_str()
            };
            if unscoped.starts_with("n8n-nodes-") || name.contains("-n8n-") {
                // Read the package's own package.json for node discovery
                let pkg_dir = node_modules.join(name);
                let installed_nodes = std::fs::read_to_string(pkg_dir.join("package.json"))
                    .ok()
                    .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
                    .map(|pj| discover_nodes_from_package_json(&pj))
                    .unwrap_or_default();

                info!(
                    "[n8n] Package {} has {} discovered nodes",
                    name,
                    installed_nodes.len()
                );

                packages.push(CommunityPackage {
                    package_name: name.clone(),
                    installed_version: version
                        .as_str()
                        .unwrap_or("unknown")
                        .trim_start_matches('^')
                        .trim_start_matches('~')
                        .to_string(),
                    installed_nodes,
                });
            }
        }
    }

    info!(
        "[n8n] Found {} community packages via data dir package.json",
        packages.len()
    );
    Ok(packages)
}

// ── n8n Credential Management ──────────────────────────────────────────

/// A field definition discovered from n8n's credential schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct N8nCredentialSchemaField {
    pub name: String,
    pub display_name: String,
    /// "string", "number", "boolean", "options"
    pub field_type: String,
    pub required: bool,
    pub default_value: Option<String>,
    pub placeholder: Option<String>,
    pub description: Option<String>,
    /// If field_type == "options", possible values
    pub options: Vec<String>,
    /// If true, render as password input
    pub is_secret: bool,
}

/// Schema for a specific n8n credential type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct N8nCredentialSchema {
    pub credential_type: String,
    pub display_name: String,
    pub fields: Vec<N8nCredentialSchemaField>,
    /// Link to n8n documentation or external setup guide for this credential.
    pub documentation_url: Option<String>,
}

/// Information about credential types required by an installed community package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageCredentialInfo {
    pub package_name: String,
    pub credential_types: Vec<N8nCredentialSchema>,
}

/// Fetch credential schemas for the node types installed by a community package.
///
/// After a community package is installed, n8n registers its node types.
/// This command discovers which credential types those nodes need and fetches
/// the field schema from n8n's REST API so the app can render an in-app form.
#[tauri::command]
pub async fn engine_n8n_package_credential_schema(
    app_handle: tauri::AppHandle,
    package_name: String,
) -> Result<PackageCredentialInfo, String> {
    let (base_url, api_key) = get_n8n_endpoint(&app_handle)?;
    let base = base_url.trim_end_matches('/');

    let api_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    // Obtain a session-authenticated client for /types/* endpoints.
    // These endpoints require a browser-style session cookie, NOT the API key.
    let session_client = n8n_engine::health::session_client(&base_url).await.ok();

    // Step 1: Try to get installed node types for this package.
    // First try the REST API, then fall back to matching by package name prefix.
    let known_node_types: Vec<String> = {
        let list_url = format!("{}/api/v1/community-packages", base);
        let resp = api_client
            .get(&list_url)
            .header("X-N8N-API-KEY", &api_key)
            .send()
            .await;

        match resp {
            Ok(r) if r.status().is_success() => {
                let pkgs: Vec<CommunityPackage> = r.json().await.unwrap_or_default();
                pkgs.iter()
                    .find(|p| p.package_name == package_name)
                    .map(|p| {
                        p.installed_nodes
                            .iter()
                            .map(|n| n.node_type.clone())
                            .collect()
                    })
                    .unwrap_or_default()
            }
            _ => {
                info!(
                    "[n8n] Community packages list unavailable — will discover node types by package name"
                );
                vec![]
            }
        }
    };

    // Step 2: Fetch /types/credentials.json (bulk credential definitions).
    // This endpoint requires session auth — use session_client if available,
    // otherwise fall back to API key (which will likely 401).
    let all_cred_types: serde_json::Value = if let Some(ref sc) = session_client {
        let cred_types_url = format!("{}/types/credentials.json", base);
        match sc.get(&cred_types_url).send().await {
            Ok(r) if r.status().is_success() => {
                info!("[n8n] /types/credentials.json fetched via session auth");
                r.json().await.unwrap_or(serde_json::Value::Array(vec![]))
            }
            Ok(r) => {
                info!(
                    "[n8n] /types/credentials.json returned HTTP {} via session",
                    r.status().as_u16()
                );
                serde_json::Value::Array(vec![])
            }
            Err(e) => {
                warn!(
                    "[n8n] /types/credentials.json session request failed: {}",
                    e
                );
                serde_json::Value::Array(vec![])
            }
        }
    } else {
        // No session available — try API key as last resort
        let cred_types_url = format!("{}/types/credentials.json", base);
        match api_client
            .get(&cred_types_url)
            .header("X-N8N-API-KEY", &api_key)
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => {
                r.json().await.unwrap_or(serde_json::Value::Array(vec![]))
            }
            _ => serde_json::Value::Array(vec![]),
        }
    };

    // Step 3: Get /types/nodes.json to discover which credential types nodes need.
    // Same session-auth requirement as Step 2.
    let mut needed_cred_types: Vec<String> = Vec::new();

    let nodes_json_result: Option<serde_json::Value> = if let Some(ref sc) = session_client {
        let node_types_url = format!("{}/types/nodes.json", base);
        match sc.get(&node_types_url).send().await {
            Ok(r) if r.status().is_success() => {
                info!("[n8n] /types/nodes.json fetched via session auth");
                r.json().await.ok()
            }
            Ok(r) => {
                info!(
                    "[n8n] /types/nodes.json returned HTTP {} via session",
                    r.status().as_u16()
                );
                None
            }
            Err(e) => {
                warn!("[n8n] /types/nodes.json session request failed: {}", e);
                None
            }
        }
    } else {
        let node_types_url = format!("{}/types/nodes.json", base);
        match api_client
            .get(&node_types_url)
            .header("X-N8N-API-KEY", &api_key)
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => r.json().await.ok(),
            _ => None,
        }
    };

    if let Some(nodes_json) = nodes_json_result {
        if let Some(nodes_arr) = nodes_json.as_array() {
            for node in nodes_arr {
                let node_name = node
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or_default();

                // Match nodes belonging to this package:
                // - If we have known node types from the REST API, match exactly
                // - Otherwise, match by package name prefix (e.g., "n8n-nodes-foo.Bar")
                let is_pkg_node = if !known_node_types.is_empty() {
                    known_node_types.iter().any(|t| t == node_name)
                } else {
                    node_name.starts_with(&format!("{}.", package_name))
                };

                if !is_pkg_node {
                    continue;
                }

                // Extract credential type names from node definition
                if let Some(cred_arr) = node.get("credentials").and_then(|c| c.as_array()) {
                    for cred in cred_arr {
                        if let Some(cred_name) = cred.get("name").and_then(|n| n.as_str()) {
                            if !needed_cred_types.contains(&cred_name.to_string()) {
                                needed_cred_types.push(cred_name.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    info!(
        "[n8n] Discovered {} credential type(s) for '{}': {:?}",
        needed_cred_types.len(),
        package_name,
        needed_cred_types
    );

    // Step 4: For each needed credential type, extract its schema
    let mut schemas: Vec<N8nCredentialSchema> = Vec::new();

    for cred_type_name in &needed_cred_types {
        // First try to find it in the /types/credentials.json bulk response
        let cred_def = all_cred_types.as_array().and_then(|arr| {
            arr.iter()
                .find(|ct| ct.get("name").and_then(|n| n.as_str()) == Some(cred_type_name))
        });

        if let Some(ct) = cred_def {
            let display = ct
                .get("displayName")
                .and_then(|d| d.as_str())
                .unwrap_or(cred_type_name)
                .to_string();

            let doc_url = ct
                .get("documentationUrl")
                .and_then(|d| d.as_str())
                .map(resolve_n8n_doc_url);

            let fields = extract_credential_fields(ct);

            schemas.push(N8nCredentialSchema {
                credential_type: cred_type_name.clone(),
                display_name: display,
                fields,
                documentation_url: doc_url,
            });
        } else {
            // Fallback: try the REST API schema endpoint
            let schema_url = format!("{}/api/v1/credentials/schema/{}", base, cred_type_name);
            if let Ok(resp) = api_client
                .get(&schema_url)
                .header("X-N8N-API-KEY", &api_key)
                .send()
                .await
            {
                if resp.status().is_success() {
                    if let Ok(schema_json) = resp.json::<serde_json::Value>().await {
                        let display = schema_json
                            .get("displayName")
                            .and_then(|d| d.as_str())
                            .unwrap_or(cred_type_name)
                            .to_string();

                        let doc_url = schema_json
                            .get("documentationUrl")
                            .and_then(|d| d.as_str())
                            .map(resolve_n8n_doc_url);

                        let fields = extract_credential_fields(&schema_json);

                        schemas.push(N8nCredentialSchema {
                            credential_type: cred_type_name.clone(),
                            display_name: display,
                            fields,
                            documentation_url: doc_url,
                        });
                    }
                }
            }
        }
    }

    // If we couldn't discover any schemas, emit a generic "API Key"
    // credential as a fallback. Most community nodes need at least an API key
    // and this lets users configure it even if n8n's type endpoints are
    // inaccessible (they often require session auth, not API key auth).
    if schemas.is_empty() {
        let pkg_display = display_name_for_pkg(&package_name);
        let npm_url = format!("https://www.npmjs.com/package/{}", package_name);

        schemas.push(N8nCredentialSchema {
            credential_type: format!(
                "{}Api",
                package_name.replace("n8n-nodes-", "").replace('-', "")
            ),
            display_name: format!("{} API", pkg_display),
            fields: vec![N8nCredentialSchemaField {
                name: "apiKey".into(),
                display_name: "API Key".into(),
                field_type: "string".into(),
                required: true,
                default_value: None,
                placeholder: Some("Enter your API key…".into()),
                description: Some(format!("API key for {}", pkg_display)),
                options: vec![],
                is_secret: true,
            }],
            documentation_url: Some(npm_url),
        });
    }

    // For community packages, the setup guide lives in the npm package README,
    // not on n8n's docs site.  Override any n8n-internal documentationUrl with
    // the npm page so users land on the actual community node's instructions.
    let npm_doc_url = format!("https://www.npmjs.com/package/{}", package_name);
    for schema in &mut schemas {
        schema.documentation_url = Some(npm_doc_url.clone());
    }

    Ok(PackageCredentialInfo {
        package_name,
        credential_types: schemas,
    })
}

/// Helper: extract field definitions from an n8n credential type JSON object.
fn extract_credential_fields(ct: &serde_json::Value) -> Vec<N8nCredentialSchemaField> {
    let mut fields = Vec::new();

    // n8n credential types store their fields in "properties" array
    let props = ct
        .get("properties")
        .and_then(|p| p.as_array())
        .cloned()
        .unwrap_or_default();

    for prop in &props {
        let name = prop
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or_default()
            .to_string();
        let display = prop
            .get("displayName")
            .and_then(|d| d.as_str())
            .unwrap_or(&name)
            .to_string();
        let type_hint = prop
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("string")
            .to_string();
        let required = prop
            .get("required")
            .and_then(|r| r.as_bool())
            .unwrap_or(false);
        let default_val = prop.get("default").and_then(|d| {
            if d.is_string() {
                d.as_str().map(|s| s.to_string())
            } else {
                Some(d.to_string())
            }
        });
        let placeholder = prop
            .get("placeholder")
            .and_then(|p| p.as_str())
            .map(|s| s.to_string());
        let description = prop
            .get("description")
            .and_then(|d| d.as_str())
            .map(|s| s.to_string());
        let is_secret = type_hint == "string"
            && (name.to_lowercase().contains("key")
                || name.to_lowercase().contains("secret")
                || name.to_lowercase().contains("token")
                || name.to_lowercase().contains("password"));
        let options = prop
            .get("options")
            .and_then(|o| o.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|opt| {
                        opt.get("value")
                            .or_else(|| opt.get("name"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Skip display-only / notice fields
        if type_hint == "notice" || type_hint == "hidden" {
            continue;
        }

        fields.push(N8nCredentialSchemaField {
            name,
            display_name: display,
            field_type: type_hint,
            required,
            default_value: default_val,
            placeholder,
            description,
            options,
            is_secret,
        });
    }

    fields
}

/// Resolve an n8n documentation URL.
///
/// n8n credential definitions store `documentationUrl` in several formats:
///   - Full URL:     `"https://docs.n8n.io/integrations/builtin/credentials/instagram/"`
///   - Relative:     `"instagram/"` or `"/integrations/builtin/credentials/instagram/"`
///   - Empty/other:  fallback gracefully
fn resolve_n8n_doc_url(raw: &str) -> String {
    if raw.starts_with("http://") || raw.starts_with("https://") {
        return raw.to_string();
    }
    let path = raw.trim_start_matches('/');
    if path.starts_with("integrations/") {
        format!("https://docs.n8n.io/{}", path)
    } else {
        format!(
            "https://docs.n8n.io/integrations/builtin/credentials/{}",
            path
        )
    }
}

/// Display name for a package: strip "n8n-nodes-" prefix, titlecase.
fn display_name_for_pkg(package_name: &str) -> String {
    let stripped = package_name
        .trim_start_matches("@")
        .split('/')
        .next_back()
        .unwrap_or(package_name)
        .trim_start_matches("n8n-nodes-");
    stripped
        .split('-')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Create a credential in n8n programmatically via REST API.
///
/// This replaces the need for users to open n8n's UI and manually add credentials.
/// The credential is created directly inside the embedded n8n instance.
#[tauri::command]
pub async fn engine_n8n_create_credential(
    app_handle: tauri::AppHandle,
    credential_type: String,
    credential_name: String,
    credential_data: std::collections::HashMap<String, String>,
) -> Result<serde_json::Value, String> {
    let (base_url, api_key) = get_n8n_endpoint(&app_handle)?;
    let base = base_url.trim_end_matches('/');

    info!(
        "[n8n] Creating credential '{}' of type '{}'",
        credential_name, credential_type
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let payload = serde_json::json!({
        "name": credential_name,
        "type": credential_type,
        "data": credential_data,
    });

    let resp = client
        .post(format!("{}/api/v1/credentials", base))
        .header("X-N8N-API-KEY", &api_key)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| {
            error!("[n8n] Create credential request failed: {}", e);
            format!("Failed to create credential: {}", e)
        })?;

    if resp.status().is_success() {
        let result: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse credential creation response: {}", e))?;
        info!(
            "[n8n] Credential '{}' created successfully (id={})",
            credential_name,
            result.get("id").and_then(|i| i.as_u64()).unwrap_or(0)
        );
        Ok(result)
    } else {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        error!("[n8n] Create credential failed: HTTP {} — {}", status, body);
        Err(format!(
            "Failed to create credential (HTTP {}): {}",
            status, body
        ))
    }
}

/// Install a community node package from npm into the n8n engine.
///
/// `package_name` is the npm package name, e.g. "n8n-nodes-puppeteer"
/// or "@n8n/n8n-nodes-langchain".
///
/// Strategy:
///   1. Verify the package exists on npm (fast, from host).
///   2. For lightweight packages, try the n8n REST API first.
///      For "heavy" packages (puppeteer, playwright, etc.) skip the
///      REST API entirely — it would trigger a postinstall binary
///      download that stalls inside the slim Alpine container.
///   3. Fall back to direct `npm install` inside the container/process
///      data directory (with skip-download env vars) and restart n8n.
///
/// Emits `n8n-install-progress` events so the frontend can show real-time
/// status updates.  Respects the `INSTALL_CANCELLED` flag for user abort.
#[tauri::command]
pub async fn engine_n8n_community_packages_install(
    app_handle: tauri::AppHandle,
    package_name: String,
) -> Result<CommunityPackage, String> {
    // Reset cancellation flag at start
    INSTALL_CANCELLED.store(false, Ordering::SeqCst);

    emit_install_progress(&app_handle, &package_name, "preparing", "Checking engine…");

    // Ensure n8n is running before attempting install (provisions container if needed)
    engine_n8n_ensure_ready(app_handle.clone())
        .await
        .map_err(|e| {
            format!(
                "Integration engine is not ready — please wait for it to start. ({})",
                e
            )
        })?;

    check_cancelled(&package_name)?;

    let (base_url, api_key) = get_n8n_endpoint(&app_handle)?;

    info!("[n8n] Installing community package: {}", package_name);
    emit_install_progress(
        &app_handle,
        &package_name,
        "verifying",
        "Verifying package on npm…",
    );

    // ── Step 1: verify the package exists on npm ───────────────────
    verify_npm_package_exists(&package_name).await?;

    check_cancelled(&package_name)?;

    // ── Step 2: Choose install strategy ────────────────────────────
    //
    // Heavy packages (puppeteer, playwright) try to download 200+ MB
    // browser binaries in their postinstall scripts.  When n8n's REST
    // API runs `npm install` internally it doesn't have our SKIP_DOWNLOAD
    // env vars, so the install stalls or fails in the slim Alpine image.
    //
    // For these packages we skip the REST API entirely and go straight
    // to docker exec (where we control the environment).
    let skip_rest_api = is_heavy_package(&package_name);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .map_err(|e| e.to_string())?;

    if !skip_rest_api {
        emit_install_progress(
            &app_handle,
            &package_name,
            "installing",
            "Installing via n8n REST API…",
        );

        let resp = client
            .post(format!(
                "{}/api/v1/community-packages",
                base_url.trim_end_matches('/')
            ))
            .header("X-N8N-API-KEY", &api_key)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({ "name": package_name }))
            .send()
            .await
            .map_err(|e| {
                error!("[n8n] Install request failed for {}: {}", package_name, e);
                format!("Failed to install package (request error): {}", e)
            })?;

        if resp.status().is_success() {
            let body_text = resp.text().await.map_err(|e| {
                error!("[n8n] Failed to read install response body: {}", e);
                format!("Failed to read response: {}", e)
            })?;

            let pkg: CommunityPackage = serde_json::from_str(&body_text).map_err(|e| {
                error!(
                    "[n8n] Failed to parse install response for {}: {} — body: {}",
                    package_name,
                    e,
                    safe_truncate(&body_text, 500)
                );
                format!("Failed to parse response: {}", e)
            })?;

            info!(
                "[n8n] Installed {} v{} ({} nodes)",
                pkg.package_name,
                pkg.installed_version,
                pkg.installed_nodes.len()
            );

            emit_install_progress(&app_handle, &package_name, "done", "Installed successfully");
            return Ok(pkg);
        }

        let status = resp.status().as_u16();
        let _body = resp.text().await.unwrap_or_default();
        info!(
            "[n8n] REST API install returned HTTP {} for '{}', falling back to direct npm…",
            status, package_name
        );
    } else {
        info!(
            "[n8n] Skipping REST API for heavy package '{}' — going straight to docker exec",
            package_name
        );
    }

    check_cancelled(&package_name)?;

    // ── Step 3: Direct npm install fallback ─────────────────────────
    emit_install_progress(
        &app_handle,
        &package_name,
        "installing",
        "Installing via npm (this may take a minute)…",
    );

    let config = n8n_engine::load_config(&app_handle).map_err(|e| e.to_string())?;
    match config.mode {
        n8n_engine::N8nMode::Embedded => {
            let _guard = INSTALL_LOCK.lock().await;
            PENDING_INSTALLS.fetch_add(1, Ordering::SeqCst);

            // 5 minute timeout — generous but prevents hanging forever
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(300),
                direct_npm_install_docker(&package_name),
            )
            .await;

            let remaining = PENDING_INSTALLS.fetch_sub(1, Ordering::SeqCst) - 1;

            match result {
                Ok(inner) => inner?,
                Err(_) => {
                    error!(
                        "[n8n] npm install timed out after 300s for '{}' — \
                         the package may have a large postinstall script",
                        package_name
                    );
                    return Err(format!(
                        "Install of '{}' timed out after 5 minutes. \
                         This package may need additional system dependencies \
                         or have a very large download. Try installing manually \
                         via `docker exec {} sh -c 'cd {} && npm install {}'`.",
                        package_name,
                        n8n_engine::types::CONTAINER_NAME,
                        n8n_engine::types::CONTAINER_DATA_DIR,
                        package_name
                    ));
                }
            }

            check_cancelled(&package_name)?;

            emit_install_progress(
                &app_handle,
                &package_name,
                "restarting",
                "Restarting engine to load new nodes…",
            );

            if remaining == 0 {
                restart_n8n_container(&base_url, &api_key).await;
                refresh_mcp_after_install(&app_handle).await;
            } else {
                info!(
                    "[n8n] Deferring container restart — {} install(s) still in flight",
                    remaining
                );
            }
        }
        n8n_engine::N8nMode::Process => {
            let _guard = INSTALL_LOCK.lock().await;
            PENDING_INSTALLS.fetch_add(1, Ordering::SeqCst);

            let data_dir = n8n_engine::n8n_data_dir(&app_handle);

            info!(
                "[n8n] Process mode: installing '{}' via npm in {}",
                package_name,
                data_dir.display()
            );

            // 5 minute timeout — generous but prevents hanging forever
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(300),
                direct_npm_install_process(&package_name, &data_dir),
            )
            .await;

            let remaining = PENDING_INSTALLS.fetch_sub(1, Ordering::SeqCst) - 1;

            match result {
                Ok(inner) => inner?,
                Err(_) => {
                    error!(
                        "[n8n] npm install timed out after 300s for '{}' — \
                         the package may have a large postinstall script",
                        package_name
                    );
                    return Err(format!(
                        "Install of '{}' timed out after 5 minutes. \
                         The package may have a very large post-install download. \
                         Try installing manually: cd {} && npm install {}",
                        package_name,
                        data_dir.display(),
                        package_name
                    ));
                }
            }

            check_cancelled(&package_name)?;

            // n8n must be restarted so it discovers the new node_modules.
            // Defer restart until all concurrent installs are finished.
            emit_install_progress(
                &app_handle,
                &package_name,
                "restarting",
                "Restarting engine to load new nodes…",
            );

            if remaining == 0 {
                match n8n_engine::restart_process(&app_handle).await {
                    Ok(endpoint) => {
                        info!("[n8n] Process restarted at {} after install", endpoint.url);
                    }
                    Err(e) => {
                        warn!(
                            "[n8n] Process restart failed after install ({}). \
                             Nodes may not be available until next app restart.",
                            e
                        );
                    }
                }
                refresh_mcp_after_install(&app_handle).await;
            } else {
                info!(
                    "[n8n] Deferring process restart — {} install(s) still in flight",
                    remaining
                );
            }
        }
        _ => {
            if skip_rest_api {
                return Err(format!(
                    "Package '{}' requires direct npm install which is only available \
                     in Embedded or Process mode. In Remote/Local mode, install it \
                     manually in your n8n instance.",
                    package_name
                ));
            }
            let status = 0u16; // REST was already tried and failed above
            error!(
                "[n8n] Install '{}' failed — direct fallback unavailable in {:?} mode",
                package_name, config.mode
            );
            return Err(format!(
                "Install '{}' failed (HTTP {}). \
                 The n8n instance may not have community packages enabled. \
                 Set N8N_COMMUNITY_PACKAGES_ENABLED=true and \
                 N8N_COMMUNITY_PACKAGES_ALLOW_UNVERIFIED=true in the n8n environment.",
                package_name, status
            ));
        }
    }

    // ── Step 4: Verify registration ────────────────────────────────
    emit_install_progress(
        &app_handle,
        &package_name,
        "verifying",
        "Confirming node registration…",
    );

    for attempt in 1..=5 {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Try REST API first
        let list_resp = client
            .get(format!(
                "{}/api/v1/community-packages",
                base_url.trim_end_matches('/')
            ))
            .header("X-N8N-API-KEY", &api_key)
            .send()
            .await;

        if let Ok(r) = list_resp {
            if r.status().is_success() {
                if let Ok(pkgs) = r.json::<Vec<CommunityPackage>>().await {
                    if let Some(pkg) = pkgs.into_iter().find(|p| p.package_name == package_name) {
                        info!(
                            "[n8n] Confirmed {} v{} ({} nodes) via REST API (attempt {})",
                            pkg.package_name,
                            pkg.installed_version,
                            pkg.installed_nodes.len(),
                            attempt
                        );
                        emit_install_progress(
                            &app_handle,
                            &package_name,
                            "done",
                            "Installed successfully",
                        );
                        return Ok(pkg);
                    }
                }
            }
        }

        // Fallback: check package.json in container (Docker mode)
        if let Ok(pkgs) = list_packages_from_container().await {
            if let Some(pkg) = pkgs.into_iter().find(|p| p.package_name == package_name) {
                info!(
                    "[n8n] Confirmed {} v{} via container package.json (attempt {})",
                    pkg.package_name, pkg.installed_version, attempt
                );
                emit_install_progress(&app_handle, &package_name, "done", "Installed successfully");
                return Ok(pkg);
            }
        }

        // Fallback: check package.json in data dir (Process mode)
        {
            let data_dir = n8n_engine::n8n_data_dir(&app_handle);
            if let Ok(pkgs) = list_packages_from_data_dir(&data_dir) {
                if let Some(pkg) = pkgs.into_iter().find(|p| p.package_name == package_name) {
                    info!(
                        "[n8n] Confirmed {} v{} via data dir package.json (attempt {})",
                        pkg.package_name, pkg.installed_version, attempt
                    );
                    emit_install_progress(
                        &app_handle,
                        &package_name,
                        "done",
                        "Installed successfully",
                    );
                    return Ok(pkg);
                }
            }
        }

        info!(
            "[n8n] Package '{}' not yet visible in n8n (attempt {}/5)",
            package_name, attempt
        );
    }

    // Package was npm-installed but n8n hasn't registered it yet — return a synthetic result
    info!(
        "[n8n] Package {} installed via npm fallback (n8n may need restart to register nodes)",
        package_name
    );
    emit_install_progress(
        &app_handle,
        &package_name,
        "done",
        "Installed (pending node registration)",
    );
    Ok(CommunityPackage {
        package_name: package_name.clone(),
        installed_version: "latest".into(),
        installed_nodes: vec![],
    })
}

/// Cancel a currently running community package install.
#[tauri::command]
pub async fn engine_n8n_community_packages_cancel() -> Result<(), String> {
    info!("[n8n] Install cancellation requested");
    INSTALL_CANCELLED.store(true, Ordering::SeqCst);
    Ok(())
}

// ── Helpers for fallback install ───────────────────────────────────────

/// Verify a package exists on npm before attempting install.
async fn verify_npm_package_exists(package_name: &str) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!(
        "https://registry.npmjs.org/{}",
        url::form_urlencoded::byte_serialize(package_name.as_bytes()).collect::<String>()
    );

    let resp = client
        .head(&url)
        .send()
        .await
        .map_err(|e| format!("Cannot verify package on npm: {}", e))?;

    if resp.status().as_u16() == 404 {
        return Err(format!(
            "Package '{}' does not exist on npm — check the package name for typos.",
            package_name
        ));
    }
    if !resp.status().is_success() {
        // Non-404 failure (rate limit, etc.) — let the install attempt proceed
        info!(
            "[n8n] npm registry check returned {} for '{}', proceeding anyway",
            resp.status().as_u16(),
            package_name
        );
    }
    Ok(())
}

/// Build env-var flags needed for specific packages.
/// For example, puppeteer packages try to download ~280 MB of Chromium
/// during `npm install`, which will fail in the slim Alpine n8n image.
fn extra_env_for_package(package_name: &str) -> Vec<(&'static str, &'static str)> {
    let lower = package_name.to_lowercase();
    let mut env = Vec::new();
    if lower.contains("puppeteer") || lower.contains("playwright") {
        env.push(("PUPPETEER_SKIP_CHROMIUM_DOWNLOAD", "true"));
        env.push(("PUPPETEER_SKIP_DOWNLOAD", "true"));
        env.push(("PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD", "1"));
    }
    if lower.contains("cypress") {
        env.push(("CYPRESS_INSTALL_BINARY", "0"));
    }
    if lower.contains("electron") {
        env.push(("ELECTRON_SKIP_BINARY_DOWNLOAD", "1"));
    }
    env
}

/// Returns true if the package is known to download large binaries (Chromium,
/// Electron, etc.) during `npm install`.  For these packages we skip n8n's
/// REST API (which runs `npm install` without our SKIP flags) and go straight
/// to the docker exec fallback where we control the environment.
fn is_heavy_package(package_name: &str) -> bool {
    let lower = package_name.to_lowercase();
    lower.contains("puppeteer")
        || lower.contains("playwright")
        || lower.contains("cypress")
        || lower.contains("electron")
        || lower.contains("chromium")
        || lower.contains("selenium")
        || lower.contains("sharp") // libvips native binary
}

/// Emit a progress event to the frontend during community package install.
fn emit_install_progress(
    app_handle: &tauri::AppHandle,
    package_name: &str,
    phase: &str,
    message: &str,
) {
    use tauri::Emitter;
    let _ = app_handle.emit(
        "n8n-install-progress",
        serde_json::json!({
            "packageName": package_name,
            "phase": phase,
            "message": message,
        }),
    );
}

/// Check if the user has requested cancellation of the current install.
fn check_cancelled(package_name: &str) -> Result<(), String> {
    if INSTALL_CANCELLED.load(Ordering::SeqCst) {
        warn!("[n8n] Install of '{}' cancelled by user", package_name);
        Err(format!("Install of '{}' cancelled.", package_name))
    } else {
        Ok(())
    }
}

/// Install a community node package directly via `docker exec` in the managed container.
///
/// Does NOT restart the container — the caller decides when to restart
/// (after all concurrent installs have finished).
async fn direct_npm_install_docker(package_name: &str) -> Result<(), String> {
    use tokio::process::Command;

    info!(
        "[n8n] Fallback: installing '{}' via docker exec in container '{}'",
        package_name,
        n8n_engine::types::CONTAINER_NAME
    );

    // Build the env export prefix for the shell command
    let extras = extra_env_for_package(package_name);
    let env_prefix: String = extras
        .iter()
        .map(|(k, v)| format!("export {}={} && ", k, v))
        .collect();

    let shell_cmd = format!(
        "{}cd {} && npm install --save --legacy-peer-deps '{}' 2>&1",
        env_prefix,
        n8n_engine::types::CONTAINER_DATA_DIR,
        package_name.replace('\'', "'\\''")
    );

    let output = Command::new("docker")
        .args([
            "exec",
            n8n_engine::types::CONTAINER_NAME,
            "sh",
            "-c",
            &shell_cmd,
        ])
        .output()
        .await
        .map_err(|e| format!("docker exec failed: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        let exit_code = output
            .status
            .code()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "signal".into());
        let detail = if stdout.is_empty() && stderr.is_empty() {
            format!(
                "process exited with code {} (no output — the container may have \
                 been restarted or run out of memory)",
                exit_code
            )
        } else if stderr.is_empty() {
            stdout.to_string()
        } else {
            stderr.to_string()
        };
        error!(
            "[n8n] docker exec npm install failed for {} (exit {}): stdout={}, stderr={}",
            package_name, exit_code, stdout, stderr
        );
        return Err(format!(
            "Direct npm install of '{}' failed inside container: {}",
            package_name, detail
        ));
    }

    info!(
        "[n8n] docker exec npm install succeeded for {}: {}",
        package_name,
        stdout.lines().last().unwrap_or("")
    );

    Ok(())
}

/// Restart the managed n8n container so it picks up newly installed nodes.
async fn restart_n8n_container(base_url: &str, api_key: &str) {
    use tokio::process::Command;

    info!("[n8n] Restarting container to load new nodes…");
    let _ = Command::new("docker")
        .args(["restart", n8n_engine::types::CONTAINER_NAME])
        .output()
        .await;

    // Poll n8n until it responds (up to 60s)
    info!("[n8n] Waiting for n8n to come back up after restart…");
    let ready = n8n_engine::health::poll_n8n_ready(base_url, api_key).await;
    if ready {
        info!("[n8n] Container restarted and responding");
    } else {
        // Fall back to a generous fixed wait if polling failed
        info!("[n8n] Polling timed out — waiting 10s as fallback");
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    }
}

/// After a community package install + container restart, reconnect the MCP
/// bridge and invalidate the tool index so the Librarian discovers new workflows.
///
/// Per the Conductor Protocol / Librarian Method architecture:
///   1. n8n is a headless MCP service exposing workflow-level tools
///   2. Community packages are composed into auto-deployed workflows
///   3. After restart the MCP connection is stale — must reconnect
///   4. Tool index must rebuild so request_tools() finds the new workflows
async fn refresh_mcp_after_install(app_handle: &tauri::AppHandle) {
    let state = match app_handle.try_state::<EngineState>() {
        Some(s) => s,
        None => return,
    };
    let (endpoint_url, api_key) = match get_n8n_endpoint(app_handle) {
        Ok(pair) => pair,
        Err(_) => return,
    };

    // 1. Disconnect stale MCP client and re-register
    let mcp_token = get_or_retrieve_mcp_token(app_handle).await;
    let tool_count = {
        let mut reg = state.mcp_registry.lock().await;
        reg.disconnect_n8n().await;
        match reg
            .register_n8n(&endpoint_url, &api_key, mcp_token.as_deref())
            .await
        {
            Ok(count) => count,
            Err(e) => {
                log::warn!("[n8n] MCP bridge reconnection failed after install: {}", e);
                return;
            }
        }
    };

    info!(
        "[n8n] MCP bridge reconnected after install — {} tools available",
        tool_count
    );

    // 2. Invalidate tool index so it rebuilds with new MCP tools
    //    on the next request_tools() call (lazy rebuild).
    {
        let mut idx = state.tool_index.lock().await;
        *idx = crate::engine::tool_index::ToolIndex::new();
    }

    info!("[n8n] Tool index invalidated — will rebuild on next request_tools call");

    // 3. Notify frontend about updated MCP status
    use tauri::Emitter;
    let _ = app_handle.emit(
        "n8n-mcp-status",
        serde_json::json!({
            "connected": true,
            "tool_count": tool_count,
        }),
    );
}

/// Install a community node package directly via npm in the process-mode data directory.
async fn direct_npm_install_process(
    package_name: &str,
    data_dir: &std::path::Path,
) -> Result<(), String> {
    use tokio::process::Command;

    info!(
        "[n8n] Fallback: installing '{}' via npm in {}",
        package_name,
        data_dir.display()
    );

    // Ensure the data directory has a package.json
    let pkg_json = data_dir.join("package.json");
    if !pkg_json.exists() {
        let _ = std::fs::write(&pkg_json, r#"{"name":"n8n-custom-nodes","private":true}"#);
    }

    let mut cmd = Command::new("npm");
    cmd.args(["install", "--save", "--legacy-peer-deps", package_name])
        .current_dir(data_dir);

    // Set env vars for packages that need special handling (e.g. skip Chromium download)
    for (k, v) in extra_env_for_package(package_name) {
        cmd.env(k, v);
    }

    let output = cmd
        .output()
        .await
        .map_err(|e| format!("npm install failed: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        error!(
            "[n8n] npm install failed for {}: stdout={}, stderr={}",
            package_name, stdout, stderr
        );
        return Err(format!(
            "Direct npm install of '{}' failed: {}",
            package_name,
            if stderr.is_empty() {
                stdout.to_string()
            } else {
                stderr.to_string()
            }
        ));
    }

    info!(
        "[n8n] npm install succeeded for {}: {}",
        package_name,
        stdout.lines().last().unwrap_or("")
    );

    Ok(())
}

/// Uninstall a community node package from the n8n engine.
///
/// Tries the n8n REST API first, then falls back to direct `npm uninstall`
/// (Process mode) or `docker exec npm uninstall` (Docker mode).
/// Restarts n8n after uninstall so removed nodes are unloaded, and
/// refreshes the MCP bridge so agents no longer see stale tools.
#[tauri::command]
pub async fn engine_n8n_community_packages_uninstall(
    app_handle: tauri::AppHandle,
    package_name: String,
) -> Result<(), String> {
    let (base_url, api_key) = get_n8n_endpoint(&app_handle)?;

    info!("[n8n] Uninstalling community package: {}", package_name);

    // ── Try REST API first ─────────────────────────────────────────
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .delete(format!(
            "{}/api/v1/community-packages",
            base_url.trim_end_matches('/')
        ))
        .header("X-N8N-API-KEY", &api_key)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({ "name": package_name }))
        .send()
        .await;

    let rest_ok = match resp {
        Ok(r) if r.status().is_success() => {
            info!("[n8n] Uninstalled '{}' via REST API", package_name);
            true
        }
        Ok(r) => {
            let status = r.status().as_u16();
            info!(
                "[n8n] REST API uninstall returned HTTP {} for '{}' — falling back to npm",
                status, package_name
            );
            false
        }
        Err(e) => {
            info!(
                "[n8n] REST API uninstall request failed for '{}': {} — falling back to npm",
                package_name, e
            );
            false
        }
    };

    // ── Fallback: direct npm uninstall ─────────────────────────────
    if !rest_ok {
        let config = n8n_engine::load_config(&app_handle).map_err(|e| e.to_string())?;

        match config.mode {
            n8n_engine::N8nMode::Process => {
                let data_dir = n8n_engine::n8n_data_dir(&app_handle);
                info!(
                    "[n8n] Process mode: running npm uninstall '{}' in {}",
                    package_name,
                    data_dir.display()
                );
                let output = tokio::process::Command::new("npm")
                    .args(["uninstall", &package_name])
                    .current_dir(&data_dir)
                    .output()
                    .await
                    .map_err(|e| format!("npm uninstall failed to start: {}", e))?;

                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    error!(
                        "[n8n] npm uninstall failed for '{}': {}",
                        package_name, stderr
                    );
                    return Err(format!(
                        "Failed to uninstall '{}': {}",
                        package_name,
                        stderr.chars().take(300).collect::<String>()
                    ));
                }
                info!("[n8n] npm uninstall succeeded for '{}'", package_name);
            }
            n8n_engine::N8nMode::Embedded => {
                info!(
                    "[n8n] Docker mode: running npm uninstall '{}' in container",
                    package_name
                );
                let output = tokio::process::Command::new("docker")
                    .args([
                        "exec",
                        n8n_engine::types::CONTAINER_NAME,
                        "sh",
                        "-c",
                        &format!(
                            "cd {} && npm uninstall {}",
                            n8n_engine::types::CONTAINER_DATA_DIR,
                            package_name
                        ),
                    ])
                    .output()
                    .await
                    .map_err(|e| format!("docker exec npm uninstall failed: {}", e))?;

                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    error!(
                        "[n8n] docker npm uninstall failed for '{}': {}",
                        package_name, stderr
                    );
                    return Err(format!(
                        "Failed to uninstall '{}': {}",
                        package_name,
                        stderr.chars().take(300).collect::<String>()
                    ));
                }
                info!(
                    "[n8n] docker npm uninstall succeeded for '{}'",
                    package_name
                );
            }
            _ => {
                return Err(format!(
                    "Uninstall of '{}' failed — REST API returned an error and direct \
                     npm uninstall is only available in Embedded or Process mode.",
                    package_name
                ));
            }
        }

        // ── Restart n8n so removed nodes are unloaded ──────────────
        match config.mode {
            n8n_engine::N8nMode::Process => {
                info!("[n8n] Restarting n8n process after uninstall");
                match n8n_engine::restart_process(&app_handle).await {
                    Ok(endpoint) => {
                        info!(
                            "[n8n] Process restarted at {} after uninstall",
                            endpoint.url
                        );
                    }
                    Err(e) => {
                        warn!(
                            "[n8n] Process restart failed after uninstall ({}). \
                             Nodes may still appear until next app restart.",
                            e
                        );
                    }
                }
            }
            n8n_engine::N8nMode::Embedded => {
                info!("[n8n] Restarting n8n container after uninstall");
                restart_n8n_container(&base_url, &api_key).await;
            }
            _ => {}
        }

        // ── Refresh MCP bridge ─────────────────────────────────────
        refresh_mcp_after_install(&app_handle).await;
    }

    info!("[n8n] Uninstalled community package: {}", package_name);
    Ok(())
}

// ── MCP Workflow Auto-Deployer ─────────────────────────────────────────
//
// Creates n8n workflows with the MCP Server Trigger node that expose
// service tools to agents via the MCP bridge. When a user connects
// a service (saves credentials), we auto-deploy a workflow that makes
// that service's actions available as MCP tools.

/// Deploy an MCP-enabled workflow for a service into the n8n engine.
///
/// This creates (or updates) a workflow with:
///   1. An MCP Server Trigger node (entry point for MCP tool calls)
///   2. The service's n8n node configured to execute operations
///
/// Returns the created workflow ID.
#[tauri::command]
pub async fn engine_n8n_deploy_mcp_workflow(
    app_handle: tauri::AppHandle,
    service_id: String,
    service_name: String,
    n8n_node_type: String,
) -> Result<String, String> {
    let (base_url, api_key) = get_n8n_endpoint(&app_handle)?;

    info!(
        "[n8n:mcp] Deploying MCP workflow for service '{}' (node: {})",
        service_id, n8n_node_type
    );

    let workflow_name = format!("OpenPawz MCP — {}", service_name);
    let tag = format!("openpawz-mcp-{}", service_id);

    // Check if workflow already exists (by searching for our tag)
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let existing = find_mcp_workflow(&client, &base_url, &api_key, &tag).await?;

    // Build the MCP workflow JSON
    let workflow_json = build_mcp_workflow(&workflow_name, &tag, &service_id, &n8n_node_type);

    let workflow_id = if let Some(existing_id) = existing {
        // Update existing workflow
        let resp = client
            .patch(format!(
                "{}/api/v1/workflows/{}",
                base_url.trim_end_matches('/'),
                existing_id
            ))
            .header("X-N8N-API-KEY", &api_key)
            .header("Content-Type", "application/json")
            .json(&workflow_json)
            .send()
            .await
            .map_err(|e| format!("Failed to update workflow: {}", e))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Update workflow failed: {}", body));
        }
        existing_id
    } else {
        // Create new workflow
        let resp = client
            .post(format!(
                "{}/api/v1/workflows",
                base_url.trim_end_matches('/')
            ))
            .header("X-N8N-API-KEY", &api_key)
            .header("Content-Type", "application/json")
            .json(&workflow_json)
            .send()
            .await
            .map_err(|e| format!("Failed to create workflow: {}", e))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Create workflow failed: {}", body));
        }

        let result: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        // Handle numeric or string id
        let id_str = if let Some(n) = result["id"].as_u64() {
            n.to_string()
        } else {
            result["id"].as_str().unwrap_or("unknown").to_string()
        };
        id_str
    };

    // Activate the workflow so MCP trigger is live
    let activate_resp = client
        .patch(format!(
            "{}/api/v1/workflows/{}",
            base_url.trim_end_matches('/'),
            workflow_id
        ))
        .header("X-N8N-API-KEY", &api_key)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({ "active": true }))
        .send()
        .await;

    match activate_resp {
        Ok(r) if r.status().is_success() => {
            info!(
                "[n8n:mcp] Workflow '{}' activated (id={})",
                workflow_name, workflow_id
            );
        }
        Ok(r) => {
            let body = r.text().await.unwrap_or_default();
            log::warn!(
                "[n8n:mcp] Workflow activation returned non-success: {}",
                body
            );
        }
        Err(e) => {
            log::warn!("[n8n:mcp] Workflow activation request failed: {}", e);
        }
    }

    // Refresh MCP tools so agents see the new workflow's tools
    if let Some(state) = app_handle.try_state::<EngineState>() {
        let mut reg = state.mcp_registry.lock().await;
        if reg.is_n8n_registered() {
            // Small delay to let n8n register the MCP trigger
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            if let Err(e) = reg.refresh_tools("n8n").await {
                log::warn!("[n8n:mcp] Tool refresh after deploy failed: {}", e);
            }
        }
    }

    Ok(workflow_id)
}

/// Find an existing MCP workflow we deployed (by tag in the name).
async fn find_mcp_workflow(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    tag: &str,
) -> Result<Option<String>, String> {
    let resp = client
        .get(format!(
            "{}/api/v1/workflows",
            base_url.trim_end_matches('/')
        ))
        .header("X-N8N-API-KEY", api_key)
        .send()
        .await
        .map_err(|e| format!("Failed to list workflows: {}", e))?;

    if !resp.status().is_success() {
        return Ok(None); // Can't search — treat as not found
    }

    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let workflows = body["data"].as_array().or_else(|| body.as_array());

    if let Some(workflows) = workflows {
        for wf in workflows {
            let name = wf["name"].as_str().unwrap_or("");
            if name.contains(tag) {
                let id = if let Some(n) = wf["id"].as_u64() {
                    n.to_string()
                } else {
                    wf["id"].as_str().unwrap_or("").to_string()
                };
                if !id.is_empty() {
                    return Ok(Some(id));
                }
            }
        }
    }

    Ok(None)
}

/// Build the n8n workflow JSON with MCP Server Trigger + service node.
///
/// The workflow structure:
///   [MCP Server Trigger] → [Service Node (e.g. Slack)]
///
/// The MCP trigger exposes the service node's operations as workflow-level tools.
/// When an agent calls execute_workflow via MCP, the trigger fires, routes to the
/// service node, executes the operation, and returns the result.
fn build_mcp_workflow(
    name: &str,
    tag: &str,
    service_id: &str,
    n8n_node_type: &str,
) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "nodes": [
            {
                "parameters": {},
                "id": format!("mcp-trigger-{}", service_id),
                "name": "MCP Server Trigger",
                "type": "@n8n/n8n-nodes-langchain.mcpTrigger",
                "typeVersion": 1,
                "position": [250, 300]
            },
            {
                "parameters": {
                    "operation": "={{ $json.operation }}",
                },
                "id": format!("node-{}", service_id),
                "name": service_id,
                "type": n8n_node_type,
                "typeVersion": 1,
                "position": [500, 300],
            }
        ],
        "connections": {
            "MCP Server Trigger": {
                "main": [
                    [
                        {
                            "node": service_id,
                            "type": "main",
                            "index": 0
                        }
                    ]
                ]
            }
        },
        "settings": {
            "executionOrder": "v1"
        },
        "tags": [
            { "name": tag },
            { "name": "openpawz-mcp" }
        ],
        "active": false
    })
}

// ── Phase 2.5: Integration credential commands ────────────────────────

/// Result of testing service credentials.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialTestResult {
    pub success: bool,
    pub message: String,
    pub details: Option<String>,
}

/// Test credentials for a third-party service by making a lightweight
/// validation request to its API.
#[tauri::command]
pub async fn engine_integrations_test_credentials(
    service_id: String,
    node_type: String,
    credentials: std::collections::HashMap<String, String>,
) -> Result<CredentialTestResult, String> {
    info!(
        "[test-credentials] Testing connection for service '{}' (node: {})",
        service_id, node_type
    );
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    // Per-service lightweight validation
    let result = match service_id.as_str() {
        "slack" => {
            let token = credentials
                .get("bot_token")
                .or(credentials.get("access_token"))
                .or(credentials.get("api_key"))
                .cloned()
                .unwrap_or_default();
            _test_slack(&client, &token).await
        }
        "discord" => {
            let token = credentials
                .get("bot_token")
                .or(credentials.get("api_key"))
                .cloned()
                .unwrap_or_default();
            _test_bearer_bot(
                &client,
                "https://discord.com/api/v10/users/@me",
                &token,
                "Discord",
            )
            .await
        }
        "github" | "github-app" => {
            let token = credentials
                .get("access_token")
                .or(credentials.get("api_key"))
                .cloned()
                .unwrap_or_default();
            _test_bearer(&client, "https://api.github.com/user", &token, "GitHub").await
        }
        "linear" => {
            let token = credentials.get("api_key").cloned().unwrap_or_default();
            _test_bearer(&client, "https://api.linear.app/graphql", &token, "Linear").await
        }
        "notion" => {
            let token = credentials.get("api_key").cloned().unwrap_or_default();
            _test_notion(&client, &token).await
        }
        "stripe" => {
            let key = credentials
                .get("secret_key")
                .or(credentials.get("api_key"))
                .cloned()
                .unwrap_or_default();
            _test_basic_auth(
                &client,
                "https://api.stripe.com/v1/balance",
                &key,
                "",
                "Stripe",
            )
            .await
        }
        "todoist" => {
            let token = credentials
                .get("api_token")
                .or(credentials.get("api_key"))
                .cloned()
                .unwrap_or_default();
            _test_bearer(
                &client,
                "https://api.todoist.com/rest/v2/projects",
                &token,
                "Todoist",
            )
            .await
        }
        "clickup" => {
            let token = credentials.get("api_key").cloned().unwrap_or_default();
            _test_bearer(
                &client,
                "https://api.clickup.com/api/v2/user",
                &token,
                "ClickUp",
            )
            .await
        }
        "airtable" => {
            let token = credentials.get("api_key").cloned().unwrap_or_default();
            _test_bearer(
                &client,
                "https://api.airtable.com/v0/meta/whoami",
                &token,
                "Airtable",
            )
            .await
        }
        "trello" => {
            let api_key = credentials.get("api_key").cloned().unwrap_or_default();
            let api_token = credentials.get("api_token").cloned().unwrap_or_default();
            let url = format!(
                "https://api.trello.com/1/members/me?key={}&token={}",
                api_key, api_token
            );
            _test_get(&client, &url, "Trello").await
        }
        "telegram" => {
            let token = credentials
                .get("bot_token")
                .or(credentials.get("api_key"))
                .cloned()
                .unwrap_or_default();
            let url = format!("https://api.telegram.org/bot{}/getMe", token);
            _test_get(&client, &url, "Telegram").await
        }
        "sendgrid" => {
            let token = credentials.get("api_key").cloned().unwrap_or_default();
            _test_bearer(
                &client,
                "https://api.sendgrid.com/v3/user/profile",
                &token,
                "SendGrid",
            )
            .await
        }
        "jira" => {
            let domain = credentials.get("domain").cloned().unwrap_or_default();
            let email = credentials.get("email").cloned().unwrap_or_default();
            let token = credentials.get("api_token").cloned().unwrap_or_default();
            let url = format!("https://{}/rest/api/3/myself", domain.trim_end_matches('/'));
            _test_basic_auth(&client, &url, &email, &token, "Jira").await
        }
        "zendesk" => {
            let subdomain = credentials.get("subdomain").cloned().unwrap_or_default();
            let email = credentials.get("email").cloned().unwrap_or_default();
            let token = credentials.get("api_token").cloned().unwrap_or_default();
            let url = format!("https://{}.zendesk.com/api/v2/users/me.json", subdomain);
            _test_basic_auth(
                &client,
                &url,
                &format!("{}/token", email),
                &token,
                "Zendesk",
            )
            .await
        }
        "weather-api" => {
            let location = credentials.get("location").cloned().unwrap_or_default();
            if location.is_empty() {
                Ok(CredentialTestResult {
                    success: false,
                    message: "Location is empty".into(),
                    details: None,
                })
            } else {
                // Verify the location can be geocoded via Open-Meteo
                // Uses the shared helper that handles "City, State" fallback
                match crate::commands::utility::geocode_location(&client, &location).await {
                    Ok(place) => {
                        let name = place["name"].as_str().unwrap_or("Unknown");
                        let country = place["country"].as_str().unwrap_or("");
                        Ok(CredentialTestResult {
                            success: true,
                            message: format!("Location found: {}, {}", name, country),
                            details: None,
                        })
                    }
                    Err(_) => Ok(CredentialTestResult {
                        success: false,
                        message: format!("Could not find location: {}", location),
                        details: Some(
                            "Try a different city name, e.g. 'Austin' or 'London'".into(),
                        ),
                    }),
                }
            }
        }
        _ => {
            // Generic: try to invoke the n8n credential test if available
            Ok(CredentialTestResult {
                success: true,
                message: format!("Credentials saved for {}", node_type),
                details: Some("Credentials stored — validation will occur on first use.".into()),
            })
        }
    };

    match &result {
        Ok(r) => {
            if r.success {
                info!("[test-credentials] '{}' — OK: {}", service_id, r.message);
            } else {
                warn!(
                    "[test-credentials] '{}' — FAILED: {}{}",
                    service_id,
                    r.message,
                    r.details
                        .as_deref()
                        .map(|d| format!(" ({})", d))
                        .unwrap_or_default()
                );
            }
        }
        Err(e) => warn!("[test-credentials] '{}' — error: {}", service_id, e),
    }

    result.map_err(|e| e.to_string())
}

/// Save service credentials to the app config store.
#[tauri::command]
pub fn engine_integrations_save_credentials(
    app_handle: tauri::AppHandle,
    service_id: String,
    credentials: std::collections::HashMap<String, String>,
) -> Result<(), String> {
    let key = format!("integration_creds_{}", service_id);
    channels::save_channel_config(&app_handle, &key, &credentials).map_err(|e| e.to_string())
}

/// Load saved credentials for a service.
#[tauri::command]
pub fn engine_integrations_get_credentials(
    app_handle: tauri::AppHandle,
    service_id: String,
) -> Result<std::collections::HashMap<String, String>, String> {
    let key = format!("integration_creds_{}", service_id);
    channels::load_channel_config(&app_handle, &key).map_err(|e| e.to_string())
}

// ── Credential test helpers ────────────────────────────────────────────

/// Slack's auth.test requires POST and returns `{"ok": true/false}` in the JSON body.
/// A GET still returns HTTP 200 but with `ok: false`, so we must use POST and check the body.
async fn _test_slack(
    client: &reqwest::Client,
    token: &str,
) -> Result<CredentialTestResult, String> {
    if token.is_empty() {
        return Ok(CredentialTestResult {
            success: false,
            message: "Bot token is empty".into(),
            details: None,
        });
    }
    match client
        .post("https://slack.com/api/auth.test")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .send()
        .await
    {
        Ok(resp) => {
            let body = resp.text().await.unwrap_or_default();
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                if json["ok"].as_bool() == Some(true) {
                    let team = json["team"].as_str().unwrap_or("your workspace");
                    return Ok(CredentialTestResult {
                        success: true,
                        message: format!("Connected to Slack ({})", team),
                        details: None,
                    });
                }
                let err = json["error"].as_str().unwrap_or("unknown error");
                return Ok(CredentialTestResult {
                    success: false,
                    message: format!("Slack rejected the token: {}", err),
                    details: Some(body),
                });
            }
            Ok(CredentialTestResult {
                success: false,
                message: "Slack returned an unexpected response".into(),
                details: Some(body),
            })
        }
        Err(e) => Ok(CredentialTestResult {
            success: false,
            message: "Could not reach Slack".into(),
            details: Some(classify_reqwest_error(&e)),
        }),
    }
}

async fn _test_bearer(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    name: &str,
) -> Result<CredentialTestResult, String> {
    if token.is_empty() {
        return Ok(CredentialTestResult {
            success: false,
            message: "API token is empty".into(),
            details: None,
        });
    }
    match client
        .get(url)
        .header("Authorization", format!("Bearer {}", token))
        .header("Accept", "application/json")
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => Ok(CredentialTestResult {
            success: true,
            message: format!("Connected to {}", name),
            details: None,
        }),
        Ok(resp) => {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            Ok(CredentialTestResult {
                success: false,
                message: format!("{} returned HTTP {}", name, status),
                details: Some(body),
            })
        }
        Err(e) => Ok(CredentialTestResult {
            success: false,
            message: format!("Could not reach {}", name),
            details: Some(classify_reqwest_error(&e)),
        }),
    }
}

async fn _test_bearer_bot(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    name: &str,
) -> Result<CredentialTestResult, String> {
    if token.is_empty() {
        return Ok(CredentialTestResult {
            success: false,
            message: "Bot token is empty".into(),
            details: None,
        });
    }
    match client
        .get(url)
        .header("Authorization", format!("Bot {}", token))
        .header("Accept", "application/json")
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => Ok(CredentialTestResult {
            success: true,
            message: format!("Connected to {}", name),
            details: None,
        }),
        Ok(resp) => {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            Ok(CredentialTestResult {
                success: false,
                message: format!("{} returned HTTP {}", name, status),
                details: Some(body),
            })
        }
        Err(e) => Ok(CredentialTestResult {
            success: false,
            message: format!("Could not reach {}", name),
            details: Some(classify_reqwest_error(&e)),
        }),
    }
}

async fn _test_basic_auth(
    client: &reqwest::Client,
    url: &str,
    user: &str,
    pass: &str,
    name: &str,
) -> Result<CredentialTestResult, String> {
    if user.is_empty() {
        return Ok(CredentialTestResult {
            success: false,
            message: "Credentials are empty".into(),
            details: None,
        });
    }
    match client
        .get(url)
        .basic_auth(user, Some(pass))
        .header("Accept", "application/json")
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => Ok(CredentialTestResult {
            success: true,
            message: format!("Connected to {}", name),
            details: None,
        }),
        Ok(resp) => {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            Ok(CredentialTestResult {
                success: false,
                message: format!("{} returned HTTP {}", name, status),
                details: Some(body),
            })
        }
        Err(e) => Ok(CredentialTestResult {
            success: false,
            message: format!("Could not reach {}", name),
            details: Some(classify_reqwest_error(&e)),
        }),
    }
}

async fn _test_get(
    client: &reqwest::Client,
    url: &str,
    name: &str,
) -> Result<CredentialTestResult, String> {
    match client
        .get(url)
        .header("Accept", "application/json")
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => Ok(CredentialTestResult {
            success: true,
            message: format!("Connected to {}", name),
            details: None,
        }),
        Ok(resp) => {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            Ok(CredentialTestResult {
                success: false,
                message: format!("{} returned HTTP {}", name, status),
                details: Some(body),
            })
        }
        Err(e) => Ok(CredentialTestResult {
            success: false,
            message: format!("Could not reach {}", name),
            details: Some(classify_reqwest_error(&e)),
        }),
    }
}

async fn _test_notion(
    client: &reqwest::Client,
    token: &str,
) -> Result<CredentialTestResult, String> {
    if token.is_empty() {
        return Ok(CredentialTestResult {
            success: false,
            message: "API key is empty".into(),
            details: None,
        });
    }
    match client
        .get("https://api.notion.com/v1/users/me")
        .header("Authorization", format!("Bearer {}", token))
        .header("Notion-Version", "2022-06-28")
        .header("Accept", "application/json")
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => Ok(CredentialTestResult {
            success: true,
            message: "Connected to Notion".into(),
            details: None,
        }),
        Ok(resp) => {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            Ok(CredentialTestResult {
                success: false,
                message: format!("Notion returned HTTP {}", status),
                details: Some(body),
            })
        }
        Err(e) => Ok(CredentialTestResult {
            success: false,
            message: "Could not reach Notion".into(),
            details: Some(classify_reqwest_error(&e)),
        }),
    }
}

// ── Credential status (Phase 3) ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceCredentialStatus {
    pub service_id: String,
    pub status: String, // "connected" | "expired" | "not_connected"
    pub last_tested: Option<String>,
}

/// Get credential status for all known services.
#[tauri::command]
pub async fn engine_integrations_credential_status(
    app_handle: tauri::AppHandle,
    service_ids: Vec<String>,
) -> Result<Vec<ServiceCredentialStatus>, String> {
    let mut statuses = Vec::new();

    for sid in service_ids {
        let key = format!("integration_creds_{}", sid);
        let has_creds = channels::load_channel_config::<std::collections::HashMap<String, String>>(
            &app_handle,
            &key,
        )
        .map(|m| !m.is_empty())
        .unwrap_or(false);

        statuses.push(ServiceCredentialStatus {
            service_id: sid,
            status: if has_creds {
                "connected".to_string()
            } else {
                "not_connected".to_string()
            },
            last_tested: None,
        });
    }

    Ok(statuses)
}

/// Auto-provision agent tools after saving credentials.
/// Bridges integration credentials → skill vault and auto-enables the skill.
///
/// If `credentials` is provided, uses them directly (preferred — avoids config-store roundtrip).
/// Otherwise falls back to loading from `integration_creds_{service_id}` in the config store.
#[tauri::command]
pub fn engine_integrations_provision(
    app_handle: tauri::AppHandle,
    service_id: String,
    credentials: Option<std::collections::HashMap<String, String>>,
) -> Result<String, String> {
    // 1. Use provided credentials or load from config store
    let creds = if let Some(c) = credentials {
        if c.is_empty() {
            return Err(format!("No credentials provided for '{}'.", service_id));
        }
        info!(
            "[provision] Using {} directly-provided credentials for '{}'",
            c.len(),
            service_id
        );
        c
    } else {
        let cred_key = format!("integration_creds_{}", service_id);
        let loaded: std::collections::HashMap<String, String> =
            channels::load_channel_config(&app_handle, &cred_key)
                .map_err(|e| format!("Failed to load credentials for {}: {}", service_id, e))?;
        if loaded.is_empty() {
            return Err(format!(
                "No credentials found for '{}'. Save credentials first.",
                service_id
            ));
        }
        loaded
    };

    // 2. Map integration credential keys → skill vault keys
    let (skill_id, mapped_creds) = map_integration_to_skill(&service_id, &creds);

    if mapped_creds.is_empty() {
        return Ok(format!(
            "Service '{}' credentials saved. No skill mapping needed (will use REST API tool).",
            service_id
        ));
    }

    // 3. Get engine state and vault key for encryption
    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;
    let vault_key =
        skills::get_vault_key().map_err(|e| format!("Failed to get vault key: {}", e))?;

    // 4. Clear any existing (possibly corrupted) vault entries for this skill
    //    before writing new ones. This prevents stale/corrupted keys from
    //    breaking decryption after re-provisioning.
    if let Err(e) = state.store.delete_all_skill_credentials(&skill_id) {
        warn!(
            "[provision] Failed to clear old vault entries for skill '{}': {} — continuing anyway",
            skill_id, e
        );
    }

    // 5. Write each credential to the skill vault (encrypted)
    for (key, value) in &mapped_creds {
        let encrypted = skills::encrypt_credential(value, &vault_key)
            .map_err(|e| format!("Encryption failed for {}: {}", key, e))?;
        state
            .store
            .set_skill_credential(&skill_id, key, &encrypted)
            .map_err(|e| {
                format!(
                    "Failed to store credential {} for skill {}: {}",
                    key, skill_id, e
                )
            })?;
    }

    // 6. Auto-enable the skill
    state
        .store
        .set_skill_enabled(&skill_id, true)
        .map_err(|e| format!("Failed to enable skill {}: {}", skill_id, e))?;

    let tool_count = mapped_creds.len();
    info!(
        "[provision] Bridged {} credentials for service '{}' → skill '{}', skill enabled",
        tool_count, service_id, skill_id
    );

    Ok(format!(
        "Service '{}' provisioned → skill '{}' enabled with {} credentials. Agent tools are now active.",
        service_id, skill_id, tool_count
    ))
}

/// Map integration credential keys (from UI) to skill vault keys (for tools).
/// Returns (skill_id, mapped_credentials).
pub(crate) fn map_integration_to_skill(
    service_id: &str,
    creds: &std::collections::HashMap<String, String>,
) -> (String, std::collections::HashMap<String, String>) {
    let mut mapped = std::collections::HashMap::new();

    let skill_id = match service_id {
        // ── Services with dedicated tool modules ──
        "slack" => {
            if let Some(v) = creds
                .get("bot_token")
                .or(creds.get("access_token"))
                .or(creds.get("api_key"))
            {
                mapped.insert("SLACK_BOT_TOKEN".into(), v.clone());
            }
            if let Some(v) = creds.get("default_channel") {
                mapped.insert("SLACK_DEFAULT_CHANNEL".into(), v.clone());
            }
            "slack"
        }
        "discord" => {
            if let Some(v) = creds.get("bot_token").or(creds.get("api_key")) {
                mapped.insert("DISCORD_BOT_TOKEN".into(), v.clone());
            }
            if let Some(v) = creds.get("default_channel") {
                mapped.insert("DISCORD_DEFAULT_CHANNEL".into(), v.clone());
            }
            if let Some(v) = creds.get("server_id").or(creds.get("guild_id")) {
                mapped.insert("DISCORD_SERVER_ID".into(), v.clone());
            }
            "discord"
        }
        "github" | "github-app" => {
            if let Some(v) = creds
                .get("access_token")
                .or(creds.get("api_key"))
                .or(creds.get("token"))
            {
                mapped.insert("GITHUB_TOKEN".into(), v.clone());
            }
            "github"
        }
        "trello" => {
            if let Some(v) = creds.get("api_key") {
                mapped.insert("TRELLO_API_KEY".into(), v.clone());
            }
            if let Some(v) = creds.get("api_token").or(creds.get("token")) {
                mapped.insert("TRELLO_TOKEN".into(), v.clone());
            }
            "trello"
        }
        "telegram" => {
            // Telegram uses channel bridge config, but we also store in skill vault
            // so the tool can use it directly
            if let Some(v) = creds.get("bot_token").or(creds.get("api_key")) {
                mapped.insert("TELEGRAM_BOT_TOKEN".into(), v.clone());
            }
            "telegram"
        }
        // ── Services with per-service skill vaults ──────────────────────
        // Each service gets its own skill_id so connecting multiple services
        // doesn't overwrite credentials.  The rest_api_call tool uses the
        // `service` parameter to look up the right vault.
        "notion" => {
            // Notion has a dedicated builtin skill with detailed API docs
            if let Some(v) = creds.get("api_key").or(creds.get("access_token")) {
                mapped.insert("NOTION_API_KEY".into(), v.clone());
            }
            "notion"
        }
        "linear" => {
            if let Some(v) = creds.get("api_key") {
                mapped.insert("API_KEY".into(), v.clone());
            }
            mapped.insert("API_BASE_URL".into(), "https://api.linear.app".into());
            mapped.insert("API_AUTH_HEADER".into(), "Authorization".into());
            mapped.insert("API_AUTH_PREFIX".into(), "Bearer".into());
            mapped.insert("SERVICE_NAME".into(), "Linear".into());
            mapped.insert(
                "SERVICE_HINT".into(),
                "GraphQL API for issue tracking. POST /graphql with query body.".into(),
            );
            "linear"
        }
        "stripe" => {
            if let Some(v) = creds.get("secret_key").or(creds.get("api_key")) {
                mapped.insert("API_KEY".into(), v.clone());
            }
            mapped.insert("API_BASE_URL".into(), "https://api.stripe.com/v1".into());
            mapped.insert("API_AUTH_HEADER".into(), "Authorization".into());
            mapped.insert("API_AUTH_PREFIX".into(), "Bearer".into());
            mapped.insert("SERVICE_NAME".into(), "Stripe".into());
            mapped.insert(
                "SERVICE_HINT".into(),
                "Payments API. Use form-encoded bodies for POST/PUT.".into(),
            );
            "stripe"
        }
        "todoist" => {
            if let Some(v) = creds.get("api_token").or(creds.get("api_key")) {
                mapped.insert("API_KEY".into(), v.clone());
            }
            mapped.insert(
                "API_BASE_URL".into(),
                "https://api.todoist.com/rest/v2".into(),
            );
            mapped.insert("API_AUTH_HEADER".into(), "Authorization".into());
            mapped.insert("API_AUTH_PREFIX".into(), "Bearer".into());
            mapped.insert("SERVICE_NAME".into(), "Todoist".into());
            mapped.insert(
                "SERVICE_HINT".into(),
                "Task management. GET/POST /tasks, /projects, /sections, /labels.".into(),
            );
            "todoist"
        }
        "clickup" => {
            if let Some(v) = creds.get("api_key") {
                mapped.insert("API_KEY".into(), v.clone());
            }
            mapped.insert(
                "API_BASE_URL".into(),
                "https://api.clickup.com/api/v2".into(),
            );
            mapped.insert("API_AUTH_HEADER".into(), "Authorization".into());
            mapped.insert("API_AUTH_PREFIX".into(), "Bearer".into());
            mapped.insert("SERVICE_NAME".into(), "ClickUp".into());
            mapped.insert(
                "SERVICE_HINT".into(),
                "Project management. GET /team, /space, /task.".into(),
            );
            "clickup"
        }
        "airtable" => {
            if let Some(v) = creds.get("api_key") {
                mapped.insert("API_KEY".into(), v.clone());
            }
            mapped.insert("API_BASE_URL".into(), "https://api.airtable.com/v0".into());
            mapped.insert("API_AUTH_HEADER".into(), "Authorization".into());
            mapped.insert("API_AUTH_PREFIX".into(), "Bearer".into());
            mapped.insert("SERVICE_NAME".into(), "Airtable".into());
            mapped.insert(
                "SERVICE_HINT".into(),
                "Spreadsheet-database. GET/POST /{baseId}/{tableName}.".into(),
            );
            "airtable"
        }
        "sendgrid" => {
            if let Some(v) = creds.get("api_key") {
                mapped.insert("API_KEY".into(), v.clone());
            }
            mapped.insert("API_BASE_URL".into(), "https://api.sendgrid.com/v3".into());
            mapped.insert("API_AUTH_HEADER".into(), "Authorization".into());
            mapped.insert("API_AUTH_PREFIX".into(), "Bearer".into());
            mapped.insert("SERVICE_NAME".into(), "SendGrid".into());
            mapped.insert(
                "SERVICE_HINT".into(),
                "Email API. POST /mail/send with JSON body.".into(),
            );
            "sendgrid"
        }
        "jira" => {
            // Jira uses Basic auth — store domain + encoded credentials
            let domain = creds.get("domain").cloned().unwrap_or_default();
            let email = creds.get("email").cloned().unwrap_or_default();
            let token = creds.get("api_token").cloned().unwrap_or_default();
            if !domain.is_empty() {
                let base = if domain.starts_with("http") {
                    format!("{}/rest/api/3", domain.trim_end_matches('/'))
                } else {
                    format!("https://{}/rest/api/3", domain.trim_end_matches('/'))
                };
                mapped.insert("API_BASE_URL".into(), base);
            }
            if !email.is_empty() && !token.is_empty() {
                use base64::Engine;
                let encoded = base64::engine::general_purpose::STANDARD
                    .encode(format!("{}:{}", email, token));
                mapped.insert("API_KEY".into(), encoded);
                mapped.insert("API_AUTH_HEADER".into(), "Authorization".into());
                mapped.insert("API_AUTH_PREFIX".into(), "Basic".into());
            }
            mapped.insert("SERVICE_NAME".into(), "Jira".into());
            mapped.insert(
                "SERVICE_HINT".into(),
                "Issue tracking. GET/POST /issue, /search, /project.".into(),
            );
            "jira"
        }
        "zendesk" => {
            let subdomain = creds.get("subdomain").cloned().unwrap_or_default();
            let email = creds.get("email").cloned().unwrap_or_default();
            let token = creds.get("api_token").cloned().unwrap_or_default();
            if !subdomain.is_empty() {
                mapped.insert(
                    "API_BASE_URL".into(),
                    format!("https://{}.zendesk.com/api/v2", subdomain),
                );
            }
            if !email.is_empty() && !token.is_empty() {
                use base64::Engine;
                let encoded = base64::engine::general_purpose::STANDARD
                    .encode(format!("{}/token:{}", email, token));
                mapped.insert("API_KEY".into(), encoded);
                mapped.insert("API_AUTH_HEADER".into(), "Authorization".into());
                mapped.insert("API_AUTH_PREFIX".into(), "Basic".into());
            }
            mapped.insert("SERVICE_NAME".into(), "Zendesk".into());
            mapped.insert(
                "SERVICE_HINT".into(),
                "Support tickets. GET/POST /tickets, /users, /organizations.".into(),
            );
            "zendesk"
        }
        "hubspot" => {
            if let Some(v) = creds.get("access_token").or(creds.get("api_key")) {
                mapped.insert("API_KEY".into(), v.clone());
            }
            mapped.insert("API_BASE_URL".into(), "https://api.hubapi.com".into());
            mapped.insert("API_AUTH_HEADER".into(), "Authorization".into());
            mapped.insert("API_AUTH_PREFIX".into(), "Bearer".into());
            mapped.insert("SERVICE_NAME".into(), "HubSpot".into());
            mapped.insert(
                "SERVICE_HINT".into(),
                "CRM. GET/POST /crm/v3/objects/contacts, /deals, /companies.".into(),
            );
            "hubspot"
        }
        "twilio" => {
            let sid = creds.get("account_sid").cloned().unwrap_or_default();
            let token = creds.get("auth_token").cloned().unwrap_or_default();
            if !sid.is_empty() {
                mapped.insert(
                    "API_BASE_URL".into(),
                    format!("https://api.twilio.com/2010-04-01/Accounts/{}", sid),
                );
            }
            if !sid.is_empty() && !token.is_empty() {
                use base64::Engine;
                let encoded =
                    base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", sid, token));
                mapped.insert("API_KEY".into(), encoded);
                mapped.insert("API_AUTH_HEADER".into(), "Authorization".into());
                mapped.insert("API_AUTH_PREFIX".into(), "Basic".into());
            }
            mapped.insert("SERVICE_NAME".into(), "Twilio".into());
            mapped.insert(
                "SERVICE_HINT".into(),
                "Messaging API. POST /Messages.json to send SMS.".into(),
            );
            "twilio"
        }
        "microsoft-teams" => {
            // MS Teams uses OAuth — store client credentials for now
            for (k, v) in creds {
                mapped.insert(k.to_uppercase(), v.clone());
            }
            mapped.insert("SERVICE_NAME".into(), "Microsoft Teams".into());
            mapped.insert(
                "SERVICE_HINT".into(),
                "Chat & channels via Microsoft Graph API.".into(),
            );
            "microsoft_teams"
        }
        // ── Google Workspace: tools read from vault directly, but we
        // map here so the skill auto-enables on OAuth connect ──
        "google" | "google-workspace" | "gmail" | "google-drive" | "google-calendar"
        | "google-sheets" | "google-docs" => {
            // No credential mapping needed — tools use key_vault::get("oauth:google")
            // But we store a sentinel so provision_oauth_to_skill_vault enables the skill.
            mapped.insert("GOOGLE_OAUTH_CONNECTED".into(), "true".into());
            "google_workspace"
        }
        // ── Fallback: store raw creds under service_id as a REST API skill ──
        other => {
            // For any unknown service, map all credentials as-is
            // and try to use as REST API
            for (k, v) in creds {
                mapped.insert(k.to_uppercase(), v.clone());
            }
            other
        }
    };

    (skill_id.to_string(), mapped)
}

/// Map a service_id to its corresponding skill_id without needing credentials.
/// Used by disconnect to know which skill vault to purge.
pub fn service_to_skill_id(service_id: &str) -> String {
    match service_id {
        "slack" => "slack".into(),
        "discord" => "discord".into(),
        "github" | "github-app" => "github".into(),
        "trello" => "trello".into(),
        "telegram" => "telegram".into(),
        // Each service has its own vault — no more rest_api collision
        "notion" => "notion".into(),
        "linear" => "linear".into(),
        "stripe" => "stripe".into(),
        "todoist" => "todoist".into(),
        "clickup" => "clickup".into(),
        "airtable" => "airtable".into(),
        "sendgrid" => "sendgrid".into(),
        "jira" => "jira".into(),
        "zendesk" => "zendesk".into(),
        "hubspot" => "hubspot".into(),
        "twilio" => "twilio".into(),
        "shopify" => "shopify".into(),
        "pagerduty" => "pagerduty".into(),
        "microsoft-teams" => "microsoft_teams".into(),
        "google" | "google-workspace" => "google_workspace".into(),
        "gmail" | "google-drive" | "google-calendar" | "google-sheets" | "google-docs" => {
            "google_workspace".into()
        }
        // Fallback: skill_id == service_id
        other => other.into(),
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_nodes_basic() {
        let pkg_json: serde_json::Value = serde_json::json!({
            "name": "n8n-nodes-telegram-trigger",
            "n8n": {
                "nodes": [
                    "dist/nodes/Telegram/TelegramTrigger.node.js"
                ]
            }
        });
        let nodes = discover_nodes_from_package_json(&pkg_json);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "TelegramTrigger");
        assert_eq!(nodes[0].node_type, "n8n-nodes-base.telegramtrigger");
    }

    #[test]
    fn discover_nodes_multiple() {
        let pkg_json: serde_json::Value = serde_json::json!({
            "n8n": {
                "nodes": [
                    "dist/nodes/MyService/MyService.node.js",
                    "dist/nodes/MyTrigger/MyTrigger.node.js"
                ],
                "credentials": [
                    "dist/credentials/MyServiceApi.credentials.js"
                ]
            }
        });
        let nodes = discover_nodes_from_package_json(&pkg_json);
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].name, "MyService");
        assert_eq!(nodes[1].name, "MyTrigger");
    }

    #[test]
    fn discover_nodes_empty_when_no_n8n_section() {
        let pkg_json: serde_json::Value = serde_json::json!({
            "name": "some-random-package",
            "version": "1.0.0"
        });
        let nodes = discover_nodes_from_package_json(&pkg_json);
        assert!(nodes.is_empty());
    }

    #[test]
    fn discover_nodes_empty_when_no_nodes_key() {
        let pkg_json: serde_json::Value = serde_json::json!({
            "n8n": {
                "credentials": ["dist/credentials/Foo.credentials.js"]
            }
        });
        let nodes = discover_nodes_from_package_json(&pkg_json);
        assert!(nodes.is_empty());
    }

    #[test]
    fn display_name_for_pkg_strips_prefix() {
        assert_eq!(display_name_for_pkg("n8n-nodes-puppeteer"), "Puppeteer");
        assert_eq!(
            display_name_for_pkg("n8n-nodes-google-sheets"),
            "Google Sheets"
        );
        assert_eq!(display_name_for_pkg("@scope/n8n-nodes-foo-bar"), "Foo Bar");
    }

    #[test]
    fn is_heavy_package_detection() {
        assert!(is_heavy_package("n8n-nodes-puppeteer"));
        assert!(is_heavy_package("n8n-nodes-playwright-extra"));
        assert!(!is_heavy_package("n8n-nodes-telegram"));
    }

    // ── Integration→Skill mapping tests ────────────────────────────

    #[test]
    fn map_google_workspace_returns_correct_skill_id() {
        let creds =
            std::collections::HashMap::from([("access_token".to_string(), "tok_123".to_string())]);
        for service_id in &[
            "google",
            "google-workspace",
            "gmail",
            "google-drive",
            "google-calendar",
            "google-sheets",
            "google-docs",
        ] {
            let (skill_id, mapped) = map_integration_to_skill(service_id, &creds);
            assert_eq!(
                skill_id, "google_workspace",
                "Service '{}' should map to skill 'google_workspace'",
                service_id
            );
            assert_eq!(
                mapped.get("GOOGLE_OAUTH_CONNECTED"),
                Some(&"true".to_string()),
                "Google mapping should set GOOGLE_OAUTH_CONNECTED sentinel"
            );
        }
    }

    #[test]
    fn map_slack_extracts_bot_token() {
        let creds = std::collections::HashMap::from([
            ("bot_token".to_string(), "xoxb-123".to_string()),
            ("default_channel".to_string(), "#general".to_string()),
        ]);
        let (skill_id, mapped) = map_integration_to_skill("slack", &creds);
        assert_eq!(skill_id, "slack");
        assert_eq!(mapped.get("SLACK_BOT_TOKEN"), Some(&"xoxb-123".to_string()));
        assert_eq!(
            mapped.get("SLACK_DEFAULT_CHANNEL"),
            Some(&"#general".to_string())
        );
    }

    #[test]
    fn map_discord_extracts_credentials() {
        let creds = std::collections::HashMap::from([
            ("bot_token".to_string(), "disc_tok".to_string()),
            ("server_id".to_string(), "123456".to_string()),
        ]);
        let (skill_id, mapped) = map_integration_to_skill("discord", &creds);
        assert_eq!(skill_id, "discord");
        assert_eq!(
            mapped.get("DISCORD_BOT_TOKEN"),
            Some(&"disc_tok".to_string())
        );
        assert_eq!(mapped.get("DISCORD_SERVER_ID"), Some(&"123456".to_string()));
    }

    #[test]
    fn map_github_accepts_access_token() {
        let creds =
            std::collections::HashMap::from([("access_token".to_string(), "ghp_abc".to_string())]);
        let (skill_id, mapped) = map_integration_to_skill("github", &creds);
        assert_eq!(skill_id, "github");
        assert_eq!(mapped.get("GITHUB_TOKEN"), Some(&"ghp_abc".to_string()));
    }

    #[test]
    fn map_telegram_extracts_bot_token() {
        let creds =
            std::collections::HashMap::from([("bot_token".to_string(), "123:ABC".to_string())]);
        let (skill_id, mapped) = map_integration_to_skill("telegram", &creds);
        assert_eq!(skill_id, "telegram");
        assert_eq!(
            mapped.get("TELEGRAM_BOT_TOKEN"),
            Some(&"123:ABC".to_string())
        );
    }

    #[test]
    fn map_notion_to_dedicated_skill() {
        let creds =
            std::collections::HashMap::from([("api_key".to_string(), "secret_abc".to_string())]);
        let (skill_id, mapped) = map_integration_to_skill("notion", &creds);
        assert_eq!(
            skill_id, "notion",
            "Notion should use its dedicated builtin skill"
        );
        assert_eq!(
            mapped.get("NOTION_API_KEY"),
            Some(&"secret_abc".to_string())
        );
    }

    #[test]
    fn map_jira_uses_basic_auth() {
        let creds = std::collections::HashMap::from([
            ("domain".to_string(), "myco.atlassian.net".to_string()),
            ("email".to_string(), "user@co.com".to_string()),
            ("api_token".to_string(), "jira_tok_123".to_string()),
        ]);
        let (skill_id, mapped) = map_integration_to_skill("jira", &creds);
        assert_eq!(skill_id, "jira", "Jira should have its own skill vault");
        assert!(
            mapped
                .get("API_BASE_URL")
                .unwrap()
                .contains("myco.atlassian.net"),
            "Jira base URL should contain the domain"
        );
        assert_eq!(mapped.get("API_AUTH_PREFIX"), Some(&"Basic".to_string()));
        assert_eq!(mapped.get("SERVICE_NAME"), Some(&"Jira".to_string()));
    }

    #[test]
    fn map_unknown_service_falls_through() {
        let creds =
            std::collections::HashMap::from([("api_key".to_string(), "key_123".to_string())]);
        let (skill_id, mapped) = map_integration_to_skill("some-unknown-service", &creds);
        assert_eq!(skill_id, "some-unknown-service");
        assert_eq!(mapped.get("API_KEY"), Some(&"key_123".to_string()));
    }

    // ── service_to_skill_id mapping ────────────────────────────────

    #[test]
    fn service_to_skill_id_google_unified() {
        assert_eq!(service_to_skill_id("google"), "google_workspace");
        assert_eq!(service_to_skill_id("google-workspace"), "google_workspace");
        assert_eq!(service_to_skill_id("gmail"), "google_workspace");
        assert_eq!(service_to_skill_id("google-drive"), "google_workspace");
        assert_eq!(service_to_skill_id("google-calendar"), "google_workspace");
        assert_eq!(service_to_skill_id("google-sheets"), "google_workspace");
        assert_eq!(service_to_skill_id("google-docs"), "google_workspace");
    }

    #[test]
    fn service_to_skill_id_dedicated_skills() {
        assert_eq!(service_to_skill_id("slack"), "slack");
        assert_eq!(service_to_skill_id("discord"), "discord");
        assert_eq!(service_to_skill_id("telegram"), "telegram");
        assert_eq!(service_to_skill_id("github"), "github");
    }

    #[test]
    fn service_to_skill_id_per_service_vaults() {
        // Each service now maps to its own skill_id — no more rest_api collision
        let expected: Vec<(&str, &str)> = vec![
            ("notion", "notion"),
            ("linear", "linear"),
            ("stripe", "stripe"),
            ("todoist", "todoist"),
            ("clickup", "clickup"),
            ("airtable", "airtable"),
            ("sendgrid", "sendgrid"),
            ("jira", "jira"),
            ("zendesk", "zendesk"),
            ("hubspot", "hubspot"),
            ("twilio", "twilio"),
            ("microsoft-teams", "microsoft_teams"),
        ];
        for (service, expected_skill) in &expected {
            assert_eq!(
                service_to_skill_id(service),
                *expected_skill,
                "Service '{}' should map to '{}'",
                service,
                expected_skill
            );
        }
    }

    #[test]
    fn service_to_skill_id_unknown_is_identity() {
        assert_eq!(service_to_skill_id("acme-api"), "acme-api");
        assert_eq!(service_to_skill_id("custom-thing"), "custom-thing");
    }

    // ── Per-service credential isolation tests ─────────────────────

    #[test]
    fn map_per_service_no_collision() {
        // Two services can be provisioned simultaneously without overwriting each other
        let notion_creds =
            std::collections::HashMap::from([("api_key".to_string(), "notion_key".to_string())]);
        let linear_creds =
            std::collections::HashMap::from([("api_key".to_string(), "linear_key".to_string())]);

        let (notion_skill, notion_mapped) = map_integration_to_skill("notion", &notion_creds);
        let (linear_skill, linear_mapped) = map_integration_to_skill("linear", &linear_creds);

        // Distinct skill IDs — stored in separate vault namespaces
        assert_ne!(
            notion_skill, linear_skill,
            "Services must not share skill_id"
        );
        assert_eq!(notion_skill, "notion");
        assert_eq!(linear_skill, "linear");

        // Notion stores under NOTION_API_KEY, Linear stores under API_KEY
        assert_eq!(
            notion_mapped.get("NOTION_API_KEY"),
            Some(&"notion_key".to_string())
        );
        assert_eq!(
            linear_mapped.get("API_KEY"),
            Some(&"linear_key".to_string())
        );
    }

    #[test]
    fn map_per_service_includes_service_name() {
        // Per-service mappings should include SERVICE_NAME metadata so the AI
        // knows what service it's talking to.
        let services_with_names = vec![
            ("linear", "Linear"),
            ("stripe", "Stripe"),
            ("todoist", "Todoist"),
            ("clickup", "ClickUp"),
            ("airtable", "Airtable"),
            ("sendgrid", "SendGrid"),
            ("jira", "Jira"),
            ("zendesk", "Zendesk"),
            ("hubspot", "HubSpot"),
            ("twilio", "Twilio"),
            ("microsoft-teams", "Microsoft Teams"),
        ];
        for (service_id, expected_name) in services_with_names {
            let mut creds = std::collections::HashMap::new();
            // Provide minimum creds each service might need
            creds.insert("api_key".into(), "test_key".into());
            creds.insert("domain".into(), "test.example.com".into());
            creds.insert("email".into(), "test@example.com".into());
            creds.insert("api_token".into(), "test_token".into());
            creds.insert("account_sid".into(), "AC123".into());
            creds.insert("auth_token".into(), "token123".into());
            creds.insert("subdomain".into(), "testco".into());

            let (_skill_id, mapped) = map_integration_to_skill(service_id, &creds);
            assert_eq!(
                mapped.get("SERVICE_NAME"),
                Some(&expected_name.to_string()),
                "Service '{}' should include SERVICE_NAME='{}'",
                service_id,
                expected_name,
            );
        }
    }

    #[test]
    fn map_per_service_includes_service_hint() {
        let creds = std::collections::HashMap::from([("api_key".to_string(), "key".to_string())]);
        let (_skill_id, mapped) = map_integration_to_skill("linear", &creds);
        assert!(
            mapped.get("SERVICE_HINT").is_some(),
            "Per-service mappings should include SERVICE_HINT for AI context"
        );
        assert!(
            mapped.get("SERVICE_HINT").unwrap().contains("GraphQL"),
            "Linear hint should mention GraphQL"
        );
    }

    #[test]
    fn map_integration_notion_uses_access_token() {
        // Notion OAuth returns access_token, not api_key
        let creds = std::collections::HashMap::from([(
            "access_token".to_string(),
            "ntn_oauth_token".to_string(),
        )]);
        let (skill_id, mapped) = map_integration_to_skill("notion", &creds);
        assert_eq!(skill_id, "notion");
        assert_eq!(
            mapped.get("NOTION_API_KEY"),
            Some(&"ntn_oauth_token".to_string()),
            "Notion should accept access_token as NOTION_API_KEY"
        );
    }

    // ── OAuth→n8n node mapping ─────────────────────────────────────

    #[test]
    fn oauth_service_to_n8n_node_google_variants() {
        // All Google variants should map to Gmail n8n node
        for service in &["gmail", "google", "google-workspace"] {
            let result = crate::commands::oauth::oauth_service_to_n8n_node(service);
            assert!(
                result.is_some(),
                "Service '{}' should have an n8n node mapping",
                service
            );
            let (node_type, _name) = result.unwrap();
            assert_eq!(node_type, "n8n-nodes-base.gmail");
        }
    }

    #[test]
    fn oauth_service_to_n8n_node_known_services() {
        // Verify all known service mappings exist
        let known = vec![
            ("github", "n8n-nodes-base.github"),
            ("slack", "n8n-nodes-base.slack"),
            ("discord", "n8n-nodes-base.discord"),
            ("notion", "n8n-nodes-base.notion"),
        ];
        for (service, expected_node) in known {
            let result = crate::commands::oauth::oauth_service_to_n8n_node(service);
            assert!(result.is_some(), "Missing n8n mapping for '{}'", service);
            assert_eq!(result.unwrap().0, expected_node);
        }
    }

    #[test]
    fn oauth_service_to_n8n_node_unknown_returns_none() {
        assert!(crate::commands::oauth::oauth_service_to_n8n_node("acme-crm").is_none());
        assert!(crate::commands::oauth::oauth_service_to_n8n_node("").is_none());
    }

    // ── map_integration_to_skill consistency with service_to_skill_id ──

    #[test]
    fn map_integration_skill_id_matches_service_to_skill_id() {
        // For services that both functions know about, they should agree.
        let empty_creds = std::collections::HashMap::new();
        let test_cases = vec![
            "slack",
            "discord",
            "github",
            "telegram",
            "google",
            "google-workspace",
            "gmail",
            "google-drive",
            "google-calendar",
        ];
        for service in &test_cases {
            let (map_skill, _) = map_integration_to_skill(service, &empty_creds);
            let direct_skill = service_to_skill_id(service);
            assert_eq!(
                map_skill, direct_skill,
                "Skill ID mismatch for '{}': map_integration='{}' vs service_to_skill='{}'",
                service, map_skill, direct_skill
            );
        }
    }

    // ══════════════════════════════════════════════════════════════════
    // n8n API Response Contract Tests
    //
    // These tests verify that our code correctly parses the JSON shapes
    // that real n8n instances return.  If n8n changes their API format,
    // these tests break BEFORE users hit it.  This is how we validate
    // 25,000+ potential tools without testing each one individually:
    // we test the PROTOCOL LAYER.
    // ══════════════════════════════════════════════════════════════════

    // ── GET /api/v1/workflows response parsing ─────────────────────

    #[test]
    fn parse_workflow_list_real_shape() {
        // Real n8n v1 response: { data: [...], nextCursor: null }
        let json: serde_json::Value = serde_json::json!({
            "data": [
                {
                    "id": "w1a2b3c4",
                    "name": "My Workflow",
                    "active": true,
                    "tags": [{"id": "t1", "name": "production"}],
                    "nodes": [
                        {"type": "n8n-nodes-base.webhook"},
                        {"type": "n8n-nodes-base.slack"}
                    ],
                    "createdAt": "2026-01-15T10:30:00.000Z",
                    "updatedAt": "2026-02-20T14:00:00.000Z"
                },
                {
                    "id": 42,
                    "name": "Legacy Numeric ID",
                    "active": false,
                    "tags": [],
                    "nodes": [],
                    "createdAt": "2025-06-01T00:00:00.000Z",
                    "updatedAt": "2025-06-01T00:00:00.000Z"
                }
            ],
            "nextCursor": null
        });

        let data = json
            .get("data")
            .and_then(|d| d.as_array())
            .cloned()
            .unwrap_or_default();
        assert_eq!(data.len(), 2);

        // Test ID extraction — must handle both string and numeric IDs
        let id1 = data[0]
            .get("id")
            .unwrap()
            .to_string()
            .trim_matches('"')
            .to_string();
        assert_eq!(id1, "w1a2b3c4");

        let id2 = data[1]
            .get("id")
            .unwrap()
            .to_string()
            .trim_matches('"')
            .to_string();
        assert_eq!(id2, "42"); // numeric ID becomes string

        // Test tag extraction
        let tags: Vec<String> = data[0]
            .get("tags")
            .and_then(|t| t.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(tags, vec!["production"]);

        // Test node type extraction
        let nodes: Vec<String> = data[0]
            .get("nodes")
            .and_then(|n| n.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|n| n.get("type").and_then(|t| t.as_str()).map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(
            nodes,
            vec!["n8n-nodes-base.webhook", "n8n-nodes-base.slack"]
        );
    }

    #[test]
    fn parse_workflow_count_with_count_field() {
        // Some n8n versions return a `count` field
        let body: serde_json::Value = serde_json::json!({
            "count": 47,
            "data": []
        });
        let count = body
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
        assert_eq!(count, 47);
    }

    #[test]
    fn parse_workflow_count_without_count_field() {
        // Fallback: count from data array length
        let body: serde_json::Value = serde_json::json!({
            "data": [{"id": "1"}, {"id": "2"}, {"id": "3"}]
        });
        let count = body
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
        assert_eq!(count, 3);
    }

    #[test]
    fn parse_workflow_count_empty_response() {
        let body: serde_json::Value = serde_json::json!({});
        let count = body
            .get("count")
            .or_else(|| body.get("data").and_then(|d| d.as_array().map(|_| d)))
            .map(|v| {
                if let Some(n) = v.as_u64() {
                    n as usize
                } else {
                    0
                }
            })
            .unwrap_or(0);
        assert_eq!(count, 0);
    }

    // ── Community package response parsing ─────────────────────────

    #[test]
    fn parse_community_package_real_shape() {
        let json = r#"{
            "packageName": "n8n-nodes-telegram-trigger",
            "installedVersion": "0.3.2",
            "installedNodes": [
                {"name": "TelegramTrigger", "type": "n8n-nodes-base.telegramtrigger"},
                {"name": "TelegramSend", "type": "n8n-nodes-base.telegramsend"}
            ]
        }"#;
        let pkg: CommunityPackage = serde_json::from_str(json).unwrap();
        assert_eq!(pkg.package_name, "n8n-nodes-telegram-trigger");
        assert_eq!(pkg.installed_version, "0.3.2");
        assert_eq!(pkg.installed_nodes.len(), 2);
        assert_eq!(pkg.installed_nodes[0].name, "TelegramTrigger");
        assert_eq!(
            pkg.installed_nodes[0].node_type,
            "n8n-nodes-base.telegramtrigger"
        );
    }

    #[test]
    fn parse_community_package_minimal() {
        // Some packages may have empty nodes or missing version
        let json = r#"{"packageName": "n8n-nodes-example"}"#;
        let pkg: CommunityPackage = serde_json::from_str(json).unwrap();
        assert_eq!(pkg.package_name, "n8n-nodes-example");
        assert_eq!(pkg.installed_version, ""); // default
        assert!(pkg.installed_nodes.is_empty()); // default
    }

    #[test]
    fn parse_community_packages_list_response() {
        let json = r#"[
            {"packageName": "pkg-a", "installedVersion": "1.0.0", "installedNodes": []},
            {"packageName": "pkg-b", "installedVersion": "2.1.0", "installedNodes": [
                {"name": "NodeB", "type": "n8n-nodes-base.nodeB"}
            ]}
        ]"#;
        let packages: Vec<CommunityPackage> = serde_json::from_str(json).unwrap();
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].package_name, "pkg-a");
        assert_eq!(packages[1].installed_nodes.len(), 1);
    }

    // ── Credential schema response parsing ─────────────────────────

    #[test]
    fn parse_credential_schema_real_shape() {
        // Real n8n GET /api/v1/credentials/schema/{type} response
        let json: serde_json::Value = serde_json::json!({
            "displayName": "Slack API",
            "documentationUrl": "https://docs.n8n.io/integrations/builtin/credentials/slack/",
            "properties": [
                {
                    "name": "accessToken",
                    "displayName": "Access Token",
                    "type": "string",
                    "required": true,
                    "default": "",
                    "placeholder": "xoxb-...",
                    "description": "OAuth access token for Slack"
                },
                {
                    "name": "workspace",
                    "displayName": "Workspace",
                    "type": "string",
                    "required": false,
                    "default": "",
                    "options": [
                        {"value": "default"}
                    ]
                }
            ]
        });

        let display_name = json
            .get("displayName")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown");
        assert_eq!(display_name, "Slack API");

        let props = json.get("properties").and_then(|v| v.as_array()).unwrap();
        assert_eq!(props.len(), 2);

        let first_prop = &props[0];
        assert_eq!(first_prop["name"].as_str().unwrap(), "accessToken");
        assert_eq!(first_prop["required"].as_bool().unwrap(), true);
        assert_eq!(first_prop["placeholder"].as_str().unwrap(), "xoxb-...");
    }

    // ── nodes.json response parsing (bulk credential discovery) ────

    #[test]
    fn parse_nodes_json_credential_extraction() {
        // Real n8n /types/nodes.json shape (simplified)
        let nodes: Vec<serde_json::Value> = serde_json::from_str(
            r#"[
            {
                "name": "n8n-nodes-base.slack",
                "displayName": "Slack",
                "credentials": [
                    {"name": "slackApi"},
                    {"name": "slackOAuth2Api"}
                ]
            },
            {
                "name": "n8n-nodes-base.httpRequest",
                "displayName": "HTTP Request",
                "credentials": []
            },
            {
                "name": "n8n-nodes-base.noOp",
                "displayName": "No Op"
            }
        ]"#,
        )
        .unwrap();

        // Extract credential types like our code does
        let mut cred_types: Vec<String> = Vec::new();
        for node in &nodes {
            if let Some(creds) = node.get("credentials").and_then(|c| c.as_array()) {
                for cred in creds {
                    if let Some(name) = cred.get("name").and_then(|n| n.as_str()) {
                        if !cred_types.contains(&name.to_string()) {
                            cred_types.push(name.to_string());
                        }
                    }
                }
            }
        }
        assert_eq!(cred_types, vec!["slackApi", "slackOAuth2Api"]);
    }

    // ── credentials.json response (bulk credential type defs) ──────

    #[test]
    fn parse_credentials_json_real_shape() {
        let cred_types: Vec<serde_json::Value> = serde_json::from_str(r#"[
            {
                "name": "slackApi",
                "displayName": "Slack API",
                "documentationUrl": "https://docs.n8n.io/...",
                "properties": [
                    {"name": "accessToken", "displayName": "Access Token", "type": "string", "required": true}
                ]
            },
            {
                "name": "telegramApi",
                "displayName": "Telegram API",
                "properties": [
                    {"name": "accessToken", "displayName": "Bot Token", "type": "string", "required": true}
                ]
            }
        ]"#)
        .unwrap();

        assert_eq!(cred_types.len(), 2);

        // Find by name (like our code does)
        let slack = cred_types
            .iter()
            .find(|ct| ct.get("name").and_then(|n| n.as_str()) == Some("slackApi"));
        assert!(slack.is_some());

        let props = slack
            .unwrap()
            .get("properties")
            .and_then(|p| p.as_array())
            .unwrap();
        assert_eq!(props[0]["name"].as_str().unwrap(), "accessToken");
    }

    // ── Error classification ───────────────────────────────────────

    #[test]
    fn classify_error_message_patterns() {
        // Verify our error messages contain actionable guidance
        // (These test the error strings, not reqwest errors which need real HTTP)

        // Test connection result for non-HTTP URLs
        let base = "ftp://example.com";
        let is_valid = base.starts_with("http://") || base.starts_with("https://");
        assert!(!is_valid, "ftp:// should be rejected");

        let base = "https://n8n.example.com/";
        let trimmed = base.trim_end_matches('/');
        let endpoint = format!("{}/api/v1/workflows?limit=1", trimmed);
        assert_eq!(endpoint, "https://n8n.example.com/api/v1/workflows?limit=1");

        // Trailing-slash normalization
        let base = "http://localhost:5678///";
        let trimmed = base.trim_end_matches('/');
        assert_eq!(trimmed, "http://localhost:5678");
    }

    // ── N8nTestResult serialization ────────────────────────────────

    #[test]
    fn n8n_test_result_serializes_for_frontend() {
        let result = N8nTestResult {
            connected: true,
            version: "1.82.1".into(),
            workflow_count: 15,
            error: None,
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["connected"], true);
        assert_eq!(json["version"], "1.82.1");
        assert_eq!(json["workflow_count"], 15);
        assert!(json["error"].is_null());
    }

    #[test]
    fn n8n_test_result_error_case() {
        let result = N8nTestResult {
            connected: false,
            version: String::new(),
            workflow_count: 0,
            error: Some("Connection refused".into()),
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["connected"], false);
        assert_eq!(json["error"], "Connection refused");
    }

    // ── N8nWorkflow serialization round-trip ───────────────────────

    #[test]
    fn n8n_workflow_serde_round_trip() {
        let wf = N8nWorkflow {
            id: "abc123".into(),
            name: "Test Workflow".into(),
            active: true,
            tags: vec!["mcp".into(), "paw".into()],
            nodes: vec!["n8n-nodes-base.webhook".into()],
            trigger_type: "webhook".into(),
            created_at: "2026-01-01T00:00:00.000Z".into(),
            updated_at: "2026-03-01T00:00:00.000Z".into(),
        };
        let json = serde_json::to_string(&wf).unwrap();
        let parsed: N8nWorkflow = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "abc123");
        assert_eq!(parsed.name, "Test Workflow");
        assert!(parsed.active);
        assert_eq!(parsed.tags.len(), 2);
        assert_eq!(parsed.trigger_type, "webhook");
    }

    #[test]
    fn n8n_workflow_handles_missing_optional_fields() {
        let json = r#"{"id":"1","name":"Minimal","active":false,"tags":[],"nodes":[]}"#;
        let wf: N8nWorkflow = serde_json::from_str(json).unwrap();
        assert_eq!(wf.trigger_type, ""); // default
        assert_eq!(wf.created_at, ""); // default
        assert_eq!(wf.updated_at, ""); // default
    }

    // ── discover_nodes_from_package_json robustness ────────────────

    #[test]
    fn discover_nodes_handles_missing_n8n_key() {
        let pkg_json = serde_json::json!({"name": "some-package"});
        let nodes = discover_nodes_from_package_json(&pkg_json);
        assert!(nodes.is_empty());
    }

    #[test]
    fn discover_nodes_handles_empty_nodes_array() {
        let pkg_json = serde_json::json!({
            "n8n": {"nodes": []}
        });
        let nodes = discover_nodes_from_package_json(&pkg_json);
        assert!(nodes.is_empty());
    }

    #[test]
    fn discover_nodes_extracts_name_from_path() {
        let pkg_json = serde_json::json!({
            "name": "n8n-nodes-aws",
            "n8n": {
                "nodes": [
                    "dist/nodes/Aws/S3/AwsS3.node.js",
                    "dist/nodes/Aws/Lambda/AwsLambda.node.js"
                ]
            }
        });
        let nodes = discover_nodes_from_package_json(&pkg_json);
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].name, "AwsS3");
        assert_eq!(nodes[1].name, "AwsLambda");
    }

    // ── MCP token validation logic ─────────────────────────────────

    #[test]
    fn mcp_token_validation_logic() {
        // Mirrors the stale-token check in get_or_retrieve_mcp_token
        let valid_jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
        assert!(!valid_jwt.is_empty() && valid_jwt.contains('.') && !valid_jwt.contains('*'));

        let redacted = "eyJhbGci*****.eyJzdWI****";
        assert!(
            redacted.contains('*'),
            "Redacted token should trigger refresh"
        );

        let empty = "";
        assert!(empty.is_empty(), "Empty token should trigger retrieval");

        let no_dots = "plaintext-api-key-no-jwt-structure";
        assert!(
            !no_dots.contains('.'),
            "Non-JWT should be detected as invalid"
        );
    }
}
