// commands/mcp.rs — Tauri IPC commands for MCP server management (Phase E)

use crate::commands::state::EngineState;
use crate::engine::channels;
use crate::engine::mcp::types::{McpServerConfig, McpServerStatus};
use tauri::State;

// ── Config key for persisting the server list ──────────────────────────

const CONFIG_KEY: &str = "mcp_servers";

// ── Read all configured servers ────────────────────────────────────────

#[tauri::command]
pub fn engine_mcp_list_servers(
    app_handle: tauri::AppHandle,
) -> Result<Vec<McpServerConfig>, String> {
    channels::load_channel_config::<Vec<McpServerConfig>>(&app_handle, CONFIG_KEY)
        .or_else(|_| Ok(vec![]))
}

// ── Add or update a server config ──────────────────────────────────────

#[tauri::command]
pub fn engine_mcp_save_server(
    app_handle: tauri::AppHandle,
    server: McpServerConfig,
) -> Result<(), String> {
    let mut servers: Vec<McpServerConfig> =
        channels::load_channel_config(&app_handle, CONFIG_KEY).unwrap_or_default();

    // Replace existing or append new
    if let Some(pos) = servers.iter().position(|s| s.id == server.id) {
        servers[pos] = server;
    } else {
        servers.push(server);
    }

    channels::save_channel_config(&app_handle, CONFIG_KEY, &servers)
        .map_err(|e| e.to_string())
}

// ── Remove a server config ─────────────────────────────────────────────

#[tauri::command]
pub async fn engine_mcp_remove_server(
    app_handle: tauri::AppHandle,
    state: State<'_, EngineState>,
    id: String,
) -> Result<(), String> {
    // Disconnect if running
    {
        let mut reg = state.mcp_registry.lock().await;
        reg.disconnect(&id).await;
    }

    // Remove from persisted config
    let mut servers: Vec<McpServerConfig> =
        channels::load_channel_config(&app_handle, CONFIG_KEY).unwrap_or_default();
    servers.retain(|s| s.id != id);
    channels::save_channel_config(&app_handle, CONFIG_KEY, &servers)
        .map_err(|e| e.to_string())
}

// ── Connect to a server ────────────────────────────────────────────────

#[tauri::command]
pub async fn engine_mcp_connect(
    app_handle: tauri::AppHandle,
    state: State<'_, EngineState>,
    id: String,
) -> Result<(), String> {
    let servers: Vec<McpServerConfig> =
        channels::load_channel_config(&app_handle, CONFIG_KEY).unwrap_or_default();

    let config = servers
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| format!("Server '{}' not found", id))?;

    if !config.enabled {
        return Err(format!("Server '{}' is disabled", id));
    }

    let mut reg = state.mcp_registry.lock().await;
    reg.connect(config).await
}

// ── Disconnect a server ────────────────────────────────────────────────

#[tauri::command]
pub async fn engine_mcp_disconnect(
    state: State<'_, EngineState>,
    id: String,
) -> Result<(), String> {
    let mut reg = state.mcp_registry.lock().await;
    reg.disconnect(&id).await;
    Ok(())
}

// ── Get status of all connected servers ────────────────────────────────

#[tauri::command]
pub async fn engine_mcp_status(
    state: State<'_, EngineState>,
) -> Result<Vec<McpServerStatus>, String> {
    let reg = state.mcp_registry.lock().await;
    Ok(reg.status_list())
}

// ── Refresh tool list for a server ─────────────────────────────────────

#[tauri::command]
pub async fn engine_mcp_refresh_tools(
    state: State<'_, EngineState>,
    id: String,
) -> Result<(), String> {
    let mut reg = state.mcp_registry.lock().await;
    reg.refresh_tools(&id).await
}

// ── Connect all enabled servers (called on app startup) ────────────────

#[tauri::command]
pub async fn engine_mcp_connect_all(
    app_handle: tauri::AppHandle,
    state: State<'_, EngineState>,
) -> Result<(), String> {
    let servers: Vec<McpServerConfig> =
        channels::load_channel_config(&app_handle, CONFIG_KEY).unwrap_or_default();

    let mut errors = Vec::new();
    let mut reg = state.mcp_registry.lock().await;

    for server in servers {
        if !server.enabled {
            continue;
        }
        let name = server.name.clone();
        if let Err(e) = reg.connect(server).await {
            log::warn!("[mcp] Failed to connect '{}': {}", name, e);
            errors.push(format!("{}: {}", name, e));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Some MCP servers failed to connect: {}",
            errors.join("; ")
        ))
    }
}
