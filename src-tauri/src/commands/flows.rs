// commands/flows.rs — Thin Tauri command wrappers for flow operations.
//
// Business logic lives in engine/sessions/flows.rs. This file only:
//   1. Extracts Tauri State<> from managed state
//   2. Delegates to the engine layer
//   3. Maps errors to String for the IPC boundary

use crate::engine::state::EngineState;
use crate::engine::types::{Flow, FlowRun};
use log::info;
use serde::{Deserialize, Serialize};
use tauri::State;

// ── Flow Commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn engine_flows_list(state: State<'_, EngineState>) -> Result<Vec<Flow>, String> {
    state.store.list_flows().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_flows_get(
    state: State<'_, EngineState>,
    flow_id: String,
) -> Result<Option<Flow>, String> {
    state.store.get_flow(&flow_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_flows_save(state: State<'_, EngineState>, flow: Flow) -> Result<(), String> {
    info!("[engine] Saving flow: {} ({})", flow.name, flow.id);
    state.store.save_flow(&flow).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_flows_delete(state: State<'_, EngineState>, flow_id: String) -> Result<(), String> {
    info!("[engine] Deleting flow: {}", flow_id);
    state.store.delete_flow(&flow_id).map_err(|e| e.to_string())
}

// ── Flow Run Commands ──────────────────────────────────────────────────────

#[tauri::command]
pub fn engine_flow_runs_list(
    state: State<'_, EngineState>,
    flow_id: String,
    limit: Option<u32>,
) -> Result<Vec<FlowRun>, String> {
    let limit = limit.unwrap_or(50);
    state
        .store
        .list_flow_runs(&flow_id, limit)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_flow_run_create(state: State<'_, EngineState>, run: FlowRun) -> Result<(), String> {
    info!(
        "[engine] Recording flow run: {} for flow {}",
        run.id, run.flow_id
    );
    state.store.create_flow_run(&run).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_flow_run_update(state: State<'_, EngineState>, run: FlowRun) -> Result<(), String> {
    state.store.update_flow_run(&run).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_flow_run_delete(state: State<'_, EngineState>, run_id: String) -> Result<(), String> {
    state
        .store
        .delete_flow_run(&run_id)
        .map_err(|e| e.to_string())
}

// ── Conductor Extract: Direct HTTP Request ─────────────────────────────────

/// Request payload for a direct HTTP call from the Conductor Extract primitive.
#[derive(Debug, Serialize, Deserialize)]
pub struct DirectHttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Option<std::collections::HashMap<String, String>>,
    pub body: Option<String>,
    pub timeout_ms: Option<u64>,
}

/// Response from a direct HTTP call.
#[derive(Debug, Serialize, Deserialize)]
pub struct DirectHttpResponse {
    pub status: u16,
    pub headers: std::collections::HashMap<String, String>,
    pub body: String,
    pub duration_ms: u64,
}

#[tauri::command]
pub async fn engine_flow_direct_http(
    request: DirectHttpRequest,
) -> Result<DirectHttpResponse, String> {
    info!(
        "[conductor-extract] Direct HTTP: {} {}",
        request.method, request.url
    );

    let client = reqwest::Client::new();
    let timeout = std::time::Duration::from_millis(request.timeout_ms.unwrap_or(30_000));
    let start = std::time::Instant::now();

    let method = request
        .method
        .to_uppercase()
        .parse::<reqwest::Method>()
        .map_err(|e| format!("Invalid HTTP method: {}", e))?;

    let mut builder = client.request(method, &request.url).timeout(timeout);

    // Apply headers
    if let Some(headers) = &request.headers {
        for (k, v) in headers {
            builder = builder.header(k.as_str(), v.as_str());
        }
    }

    // Apply body
    if let Some(body) = &request.body {
        builder = builder.body(body.clone());
    }

    let response = builder
        .send()
        .await
        .map_err(|e| format!("HTTP error: {}", e))?;
    let duration_ms = start.elapsed().as_millis() as u64;
    let status = response.status().as_u16();

    let resp_headers: std::collections::HashMap<String, String> = response
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();

    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    Ok(DirectHttpResponse {
        status,
        headers: resp_headers,
        body,
        duration_ms,
    })
}

// ── Conductor Extract: Direct MCP Tool Call ────────────────────────────────

/// Request payload for calling an MCP tool directly (no LLM).
#[derive(Debug, Serialize, Deserialize)]
pub struct DirectMcpRequest {
    pub tool_name: String,
    pub arguments: serde_json::Value,
}

/// Response from a direct MCP tool call.
#[derive(Debug, Serialize, Deserialize)]
pub struct DirectMcpResponse {
    pub output: String,
    pub success: bool,
    pub duration_ms: u64,
}

#[tauri::command]
pub async fn engine_flow_direct_mcp(
    state: State<'_, EngineState>,
    request: DirectMcpRequest,
) -> Result<DirectMcpResponse, String> {
    info!(
        "[conductor-extract] Direct MCP tool call: {}",
        request.tool_name
    );

    let start = std::time::Instant::now();
    let reg = state.mcp_registry.lock().await;

    let result = reg
        .execute_tool(&request.tool_name, &request.arguments)
        .await;

    let duration_ms = start.elapsed().as_millis() as u64;

    match result {
        Some(Ok(output)) => Ok(DirectMcpResponse {
            output,
            success: true,
            duration_ms,
        }),
        Some(Err(e)) => Ok(DirectMcpResponse {
            output: e.clone(),
            success: false,
            duration_ms,
        }),
        None => Err(format!(
            "MCP tool '{}' not found in any connected server",
            request.tool_name
        )),
    }
}
