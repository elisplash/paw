// commands/ollama.rs — Tauri IPC commands for Ollama model management
//
// Provides commands for listing, pulling, and creating Ollama models.
// Used by the Zero-Gap auto-setup flow to create the worker-qwen model.

use log::info;
use serde::{Deserialize, Serialize};

// ── Types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaModel {
    pub name: String,
    pub size: u64,
    pub modified_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaCreateResult {
    pub success: bool,
    pub model_name: String,
    pub message: String,
}

// ── Helpers ────────────────────────────────────────────────────────────

/// Get the Ollama base URL from the engine config, or default to localhost.
fn get_ollama_url(app_handle: &tauri::AppHandle) -> String {
    use crate::engine::state::EngineState;
    use crate::engine::types::ProviderKind;
    use tauri::Manager;

    if let Some(state) = app_handle.try_state::<EngineState>() {
        let cfg = state.config.lock();
        if let Some(provider) = cfg.providers.iter().find(|p| p.kind == ProviderKind::Ollama) {
            if let Some(ref url) = provider.base_url {
                if !url.is_empty() {
                    return url.trim_end_matches('/').to_string();
                }
            }
        }
    }
    "http://localhost:11434".to_string()
}

// ── Commands ───────────────────────────────────────────────────────────

/// List all models available in the local Ollama instance.
#[tauri::command]
pub async fn engine_ollama_list_models(
    app_handle: tauri::AppHandle,
) -> Result<Vec<OllamaModel>, String> {
    let base_url = get_ollama_url(&app_handle);
    let url = format!("{}/api/tags", base_url);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Cannot reach Ollama at {}: {}", base_url, e))?;

    if !resp.status().is_success() {
        return Err(format!("Ollama returned HTTP {}", resp.status().as_u16()));
    }

    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    let models = body["models"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    let name = m["name"].as_str()?.to_string();
                    Some(OllamaModel {
                        name,
                        size: m["size"].as_u64().unwrap_or(0),
                        modified_at: m["modified_at"]
                            .as_str()
                            .unwrap_or("")
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(models)
}

/// Check if a specific model exists in Ollama.
#[tauri::command]
pub async fn engine_ollama_has_model(
    app_handle: tauri::AppHandle,
    model_name: String,
) -> Result<bool, String> {
    let models = engine_ollama_list_models(app_handle).await?;
    let model_base = model_name.split(':').next().unwrap_or(&model_name);
    Ok(models.iter().any(|m| {
        let m_base = m.name.split(':').next().unwrap_or(&m.name);
        m_base == model_base || m.name == model_name
    }))
}

/// Pull a model from the Ollama registry.
///
/// This can take a long time (minutes) for large models.
/// The frontend should show a progress indicator.
#[tauri::command]
pub async fn engine_ollama_pull_model(
    app_handle: tauri::AppHandle,
    model_name: String,
) -> Result<String, String> {
    let base_url = get_ollama_url(&app_handle);

    info!("[ollama] Pulling model '{}' from {}", model_name, base_url);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(1800)) // 30 min timeout for large models
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .post(format!("{}/api/pull", base_url))
        .json(&serde_json::json!({
            "name": model_name,
            "stream": false
        }))
        .send()
        .await
        .map_err(|e| format!("Pull failed: {}", e))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Pull '{}' failed: {}", model_name, body));
    }

    info!("[ollama] Model '{}' pulled successfully", model_name);
    Ok(format!("Model '{}' is ready", model_name))
}

/// Create a custom model from a Modelfile.
///
/// This is used to create the worker-qwen model from the embedded Modelfile.
/// The `modelfile_content` is the raw text of the Modelfile.
#[tauri::command]
pub async fn engine_ollama_create_model(
    app_handle: tauri::AppHandle,
    model_name: String,
    modelfile_content: String,
) -> Result<OllamaCreateResult, String> {
    let base_url = get_ollama_url(&app_handle);

    info!(
        "[ollama] Creating model '{}' from Modelfile ({} bytes)",
        model_name,
        modelfile_content.len()
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600)) // 10 min timeout
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .post(format!("{}/api/create", base_url))
        .json(&serde_json::json!({
            "name": model_name,
            "modelfile": modelfile_content,
            "stream": false
        }))
        .send()
        .await
        .map_err(|e| format!("Create model failed: {}", e))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Create '{}' failed: {}", model_name, body));
    }

    info!("[ollama] Model '{}' created successfully", model_name);
    Ok(OllamaCreateResult {
        success: true,
        model_name: model_name.clone(),
        message: format!("Model '{}' created. Ready for use as a worker agent.", model_name),
    })
}

/// Set up the worker-qwen model for the Architect/Worker pattern.
///
/// This is the "one-click" setup that:
/// 1. Checks if the base model exists, pulls it if needed
/// 2. Creates the worker-qwen custom model from the embedded Modelfile
/// 3. Returns status
#[tauri::command]
pub async fn engine_ollama_setup_worker(
    app_handle: tauri::AppHandle,
    base_model: Option<String>,
) -> Result<OllamaCreateResult, String> {
    let base = base_model.unwrap_or_else(|| "qwen3.5:35b-a3b".to_string());
    let worker_name = "worker-qwen";

    info!(
        "[ollama] Setting up worker model '{}' from base '{}'",
        worker_name, base
    );

    // Step 1: Check if worker model already exists
    let has_worker = engine_ollama_has_model(app_handle.clone(), worker_name.to_string()).await?;
    if has_worker {
        return Ok(OllamaCreateResult {
            success: true,
            model_name: worker_name.to_string(),
            message: format!("Worker model '{}' already exists.", worker_name),
        });
    }

    // Step 2: Check if base model exists, pull if not
    let has_base = engine_ollama_has_model(app_handle.clone(), base.clone()).await?;
    if !has_base {
        info!("[ollama] Base model '{}' not found, pulling...", base);
        engine_ollama_pull_model(app_handle.clone(), base.clone()).await?;
    }

    // Step 3: Create worker model with embedded Modelfile
    let modelfile = format!(
        r#"FROM {}

PARAMETER temperature 0
PARAMETER num_ctx 16384
PARAMETER stop "<|im_end|>"

SYSTEM """
You are the LOCAL FOREMAN (Worker Agent) for OpenPawz.

Your job is to receive Task Orders from the Architect and translate them into
precise MCP tool calls. You are a silent execution unit — never engage in
conversation, never explain your reasoning, never add commentary.

## Execution Rules

1. Parse the Task Order. Identify the sequence of tool calls needed.
2. Execute tool calls one at a time in the correct order.
3. If a tool call fails, retry ONCE with corrected parameters.
4. After all steps complete, call `report_progress` with status "done".
5. If blocked (missing credentials, permission denied, unknown tool), call
   `report_progress` with status "blocked" and include the error message.

## Tool Prefixes

- `mcp_n8n_*` — Tools from the n8n MCP bridge (automation actions)
- `install_n8n_node` — Install a community node package from npm
- `search_ncnodes` — Search 25K+ community packages
- `mcp_refresh` — Refresh available tools after installing a package
- `report_progress` — Report completion or blockers to the Architect

## Installation Flow

When told to install a package:
1. `install_n8n_node(package_name)` — wait for result
2. `mcp_refresh()` — reload tool list
3. Proceed with the next tool call using the newly available tools

## Output Format

Only output tool calls. Never output plain text unless reporting an error
via `report_progress`. Your response should be a tool_calls array, nothing else.
"""
"#,
        base
    );

    engine_ollama_create_model(app_handle, worker_name.to_string(), modelfile).await
}
