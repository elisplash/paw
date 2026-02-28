// commands/n8n.rs — Tauri IPC commands for n8n integration

use crate::engine::channels;
use crate::engine::n8n_engine;
use crate::engine::skills;
use crate::engine::state::EngineState;
use log::{error, info};
use serde::{Deserialize, Serialize};
use tauri::Manager;

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
                match reg.register_n8n(&endpoint.url, &endpoint.api_key).await {
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
#[tauri::command]
pub async fn engine_n8n_mcp_refresh(app_handle: tauri::AppHandle) -> Result<usize, String> {
    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;
    let mut reg = state.mcp_registry.lock().await;
    if reg.is_n8n_registered() {
        reg.refresh_tools("n8n").await?;
        let tool_count = reg.tool_definitions_for(&["n8n".into()]).len();
        log::info!("[n8n] MCP tools refreshed — {} tools", tool_count);
        Ok(tool_count)
    } else {
        Err("n8n MCP bridge is not connected".to_string())
    }
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
fn get_n8n_endpoint(app_handle: &tauri::AppHandle) -> Result<(String, String), String> {
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
) -> Result<Vec<NCNodeResult>, String> {
    let limit = limit.unwrap_or(10).min(50);

    info!(
        "[n8n:ncnodes] Searching npm for '{}' (limit={})",
        query, limit
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    // npm registry search API — filter by the n8n community node keyword
    let encoded_query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair(
            "text",
            &format!("{} keywords:n8n-community-node-package", query),
        )
        .append_pair("size", &limit.to_string())
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

    let results: Vec<NCNodeResult> = body["objects"]
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
                        weekly_downloads: obj["score"]["detail"]["popularity"]
                            .as_f64()
                            .map(|p| (p * 100_000.0) as u64)
                            .unwrap_or(0),
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

    info!(
        "[n8n:ncnodes] Found {} packages for '{}'",
        results.len(),
        query
    );

    Ok(results)
}

/// List community node packages installed in the n8n engine.
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
        .await
        .map_err(|e| format!("Failed to list community packages: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("List packages failed (HTTP {}): {}", status, body));
    }

    let packages: Vec<CommunityPackage> = resp.json().await.map_err(|e| e.to_string())?;
    Ok(packages)
}

/// Install a community node package from npm into the n8n engine.
///
/// `package_name` is the npm package name, e.g. "n8n-nodes-puppeteer"
/// or "@n8n/n8n-nodes-langchain".
#[tauri::command]
pub async fn engine_n8n_community_packages_install(
    app_handle: tauri::AppHandle,
    package_name: String,
) -> Result<CommunityPackage, String> {
    let (base_url, api_key) = get_n8n_endpoint(&app_handle)?;

    info!("[n8n] Installing community package: {}", package_name);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300)) // npm install can be very slow in Docker
        .build()
        .map_err(|e| e.to_string())?;

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

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        error!(
            "[n8n] Install '{}' failed (HTTP {}): {}",
            package_name, status, body
        );
        return Err(format!(
            "Install '{}' failed (HTTP {}): {}",
            package_name, status, body
        ));
    }

    let body_text = resp.text().await.map_err(|e| {
        error!("[n8n] Failed to read install response body: {}", e);
        format!("Failed to read response: {}", e)
    })?;

    let pkg: CommunityPackage = serde_json::from_str(&body_text).map_err(|e| {
        error!(
            "[n8n] Failed to parse install response for {}: {} — body: {}",
            package_name,
            e,
            &body_text[..body_text.len().min(500)]
        );
        format!("Failed to parse response: {}", e)
    })?;

    info!(
        "[n8n] Installed {} v{} ({} nodes)",
        pkg.package_name,
        pkg.installed_version,
        pkg.installed_nodes.len()
    );

    Ok(pkg)
}

/// Uninstall a community node package from the n8n engine.
#[tauri::command]
pub async fn engine_n8n_community_packages_uninstall(
    app_handle: tauri::AppHandle,
    package_name: String,
) -> Result<(), String> {
    let (base_url, api_key) = get_n8n_endpoint(&app_handle)?;

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
        .await
        .map_err(|e| format!("Failed to uninstall package: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!(
            "Uninstall '{}' failed (HTTP {}): {}",
            package_name, status, body
        ));
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
/// The MCP trigger exposes the service node's operations as MCP tools.
/// When an agent calls a tool via MCP, the trigger fires, routes to the
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

    // 4. Write each credential to the skill vault (encrypted)
    for (key, value) in &mapped_creds {
        let encrypted = skills::encrypt_credential(value, &vault_key);
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

    // 5. Auto-enable the skill
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
fn map_integration_to_skill(
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
        // ── Services that map to the generic REST API skill ──
        "notion" => {
            if let Some(v) = creds.get("api_key").or(creds.get("access_token")) {
                mapped.insert("API_KEY".into(), v.clone());
            }
            mapped.insert("API_BASE_URL".into(), "https://api.notion.com/v1".into());
            mapped.insert("API_AUTH_HEADER".into(), "Authorization".into());
            mapped.insert("API_AUTH_PREFIX".into(), "Bearer".into());
            "rest_api"
        }
        "linear" => {
            if let Some(v) = creds.get("api_key") {
                mapped.insert("API_KEY".into(), v.clone());
            }
            mapped.insert("API_BASE_URL".into(), "https://api.linear.app".into());
            mapped.insert("API_AUTH_HEADER".into(), "Authorization".into());
            mapped.insert("API_AUTH_PREFIX".into(), "Bearer".into());
            "rest_api"
        }
        "stripe" => {
            if let Some(v) = creds.get("secret_key").or(creds.get("api_key")) {
                mapped.insert("API_KEY".into(), v.clone());
            }
            mapped.insert("API_BASE_URL".into(), "https://api.stripe.com/v1".into());
            mapped.insert("API_AUTH_HEADER".into(), "Authorization".into());
            mapped.insert("API_AUTH_PREFIX".into(), "Bearer".into());
            "rest_api"
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
            "rest_api"
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
            "rest_api"
        }
        "airtable" => {
            if let Some(v) = creds.get("api_key") {
                mapped.insert("API_KEY".into(), v.clone());
            }
            mapped.insert("API_BASE_URL".into(), "https://api.airtable.com/v0".into());
            mapped.insert("API_AUTH_HEADER".into(), "Authorization".into());
            mapped.insert("API_AUTH_PREFIX".into(), "Bearer".into());
            "rest_api"
        }
        "sendgrid" => {
            if let Some(v) = creds.get("api_key") {
                mapped.insert("API_KEY".into(), v.clone());
            }
            mapped.insert("API_BASE_URL".into(), "https://api.sendgrid.com/v3".into());
            mapped.insert("API_AUTH_HEADER".into(), "Authorization".into());
            mapped.insert("API_AUTH_PREFIX".into(), "Bearer".into());
            "rest_api"
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
            "rest_api"
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
            "rest_api"
        }
        "hubspot" => {
            if let Some(v) = creds.get("access_token").or(creds.get("api_key")) {
                mapped.insert("API_KEY".into(), v.clone());
            }
            mapped.insert("API_BASE_URL".into(), "https://api.hubapi.com".into());
            mapped.insert("API_AUTH_HEADER".into(), "Authorization".into());
            mapped.insert("API_AUTH_PREFIX".into(), "Bearer".into());
            "rest_api"
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
            "rest_api"
        }
        "microsoft-teams" => {
            // MS Teams uses OAuth — store client credentials for now
            for (k, v) in creds {
                mapped.insert(k.to_uppercase(), v.clone());
            }
            "rest_api"
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
