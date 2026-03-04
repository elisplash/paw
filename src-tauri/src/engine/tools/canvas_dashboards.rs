// Paw Agent Engine — Canvas Dashboard tools (Phase 2)
// Agent tools for saving, loading, listing, and deleting dashboards.

use crate::atoms::types::*;
use crate::engine::state::EngineState;
use log::info;
use tauri::{Emitter, Manager};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "canvas_save".into(),
                description: "Save the current canvas as a named dashboard. Components are cloned from the active session to standalone dashboard scope.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Dashboard display name"
                        },
                        "icon": {
                            "type": "string",
                            "description": "Material icon name (e.g. 'dashboard', 'trending_up'). Default: 'dashboard'"
                        },
                        "pinned": {
                            "type": "boolean",
                            "description": "Pin to sidebar for quick access. Default: false"
                        },
                        "refresh_interval": {
                            "type": "string",
                            "description": "Auto-refresh schedule: '5m', '15m', '30m', '1h', '6h', '1d'. Omit for manual-only."
                        },
                        "refresh_prompt": {
                            "type": "string",
                            "description": "Agent prompt to run on each refresh cycle"
                        }
                    },
                    "required": ["name"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "canvas_load".into(),
                description: "Load a saved dashboard by name or ID into the active canvas view.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "dashboard_id": {
                            "type": "string",
                            "description": "Dashboard ID to load"
                        },
                        "name": {
                            "type": "string",
                            "description": "Dashboard name to search for (if dashboard_id not provided)"
                        }
                    },
                    "required": []
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "canvas_list_dashboards".into(),
                description: "List all saved dashboards. Returns name, id, icon, pinned status, and last refresh time for each.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "canvas_delete_dashboard".into(),
                description: "Delete a saved dashboard and all its components.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "dashboard_id": {
                            "type": "string",
                            "description": "The dashboard ID to delete"
                        }
                    },
                    "required": ["dashboard_id"]
                }),
            },
        },
    ]
}

pub async fn execute(
    name: &str,
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    agent_id: &str,
) -> Option<Result<String, String>> {
    Some(match name {
        "canvas_save" => exec_save(args, app_handle, agent_id).map_err(|e| e.to_string()),
        "canvas_load" => exec_load(args, app_handle).map_err(|e| e.to_string()),
        "canvas_list_dashboards" => exec_list(app_handle).map_err(|e| e.to_string()),
        "canvas_delete_dashboard" => exec_delete(args, app_handle).map_err(|e| e.to_string()),
        _ => return None,
    })
}

fn exec_save(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    agent_id: &str,
) -> Result<String, String> {
    let name = args["name"]
        .as_str()
        .ok_or("Missing required parameter: name")?;
    let icon = args
        .get("icon")
        .and_then(|v| v.as_str())
        .unwrap_or("dashboard");
    let pinned = args
        .get("pinned")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let refresh_interval = args.get("refresh_interval").and_then(|v| v.as_str());
    let refresh_prompt = args.get("refresh_prompt").and_then(|v| v.as_str());

    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    // Get active session to clone components from.
    let session_id = get_active_session(app_handle, agent_id)
        .ok_or("No active session — cannot save dashboard")?;

    let dashboard_id = format!("dash-{}", uuid_v4());

    state.store.create_dashboard(
        &dashboard_id,
        name,
        icon,
        agent_id,
        Some(&session_id),
        None,
        pinned,
        refresh_interval,
        refresh_prompt,
    )?;

    // Clone components from session scope to dashboard scope.
    let cloned = state
        .store
        .clone_components_to_dashboard(&session_id, &dashboard_id)?;

    info!(
        "[canvas] Dashboard saved: id={} name={} components={}",
        dashboard_id, name, cloned
    );

    // Emit event so sidebar can update.
    let _ = app_handle.emit(
        "dashboard-saved",
        serde_json::json!({
            "dashboard_id": dashboard_id,
            "name": name,
            "icon": icon,
            "pinned": pinned,
        }),
    );

    Ok(format!(
        "Dashboard '{}' saved with {} components. ID: {}{}",
        name,
        cloned,
        dashboard_id,
        if pinned { " (pinned to sidebar)" } else { "" }
    ))
}

fn exec_load(args: &serde_json::Value, app_handle: &tauri::AppHandle) -> Result<String, String> {
    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    // Look up by ID first, then by name.
    let dashboard = if let Some(id) = args.get("dashboard_id").and_then(|v| v.as_str()) {
        state.store.get_dashboard(id)?
    } else if let Some(name) = args.get("name").and_then(|v| v.as_str()) {
        // Search by name (case-insensitive partial match).
        let all = state.store.list_dashboards()?;
        let lower = name.to_lowercase();
        all.into_iter()
            .find(|d| d.name.to_lowercase().contains(&lower))
    } else {
        return Err("Provide either 'dashboard_id' or 'name'".into());
    };

    let dash = dashboard.ok_or("Dashboard not found")?;

    // Load components.
    let components = state.store.list_canvas_by_dashboard(&dash.id)?;

    // Emit event so Canvas view switches to this dashboard.
    let _ = app_handle.emit(
        "dashboard-load",
        serde_json::json!({
            "dashboard_id": dash.id,
            "name": dash.name,
            "icon": dash.icon,
            "component_count": components.len(),
        }),
    );

    info!(
        "[canvas] Dashboard loaded: id={} name={} components={}",
        dash.id,
        dash.name,
        components.len()
    );

    Ok(format!(
        "Loaded dashboard '{}' with {} components.",
        dash.name,
        components.len()
    ))
}

fn exec_list(app_handle: &tauri::AppHandle) -> Result<String, String> {
    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    let dashboards = state.store.list_dashboards()?;

    if dashboards.is_empty() {
        return Ok("No saved dashboards.".into());
    }

    let lines: Vec<String> = dashboards
        .iter()
        .map(|d| {
            format!(
                "- {} (id: {}, icon: {}, pinned: {}, refresh: {})",
                d.name,
                d.id,
                d.icon,
                d.pinned,
                d.refresh_interval.as_deref().unwrap_or("manual"),
            )
        })
        .collect();

    Ok(format!(
        "{} saved dashboard(s):\n{}",
        dashboards.len(),
        lines.join("\n")
    ))
}

fn exec_delete(args: &serde_json::Value, app_handle: &tauri::AppHandle) -> Result<String, String> {
    let dashboard_id = args["dashboard_id"]
        .as_str()
        .ok_or("Missing required parameter: dashboard_id")?;

    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    let deleted = state.store.delete_dashboard(dashboard_id)?;
    if deleted {
        let _ = app_handle.emit(
            "dashboard-deleted",
            serde_json::json!({
                "dashboard_id": dashboard_id,
            }),
        );
        info!("[canvas] Dashboard deleted: id={}", dashboard_id);
        Ok(format!("Dashboard '{}' deleted.", dashboard_id))
    } else {
        Ok(format!("No dashboard found with id '{}'.", dashboard_id))
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:016x}{:016x}", t, t.wrapping_mul(6364136223846793005))
}

fn get_active_session(app_handle: &tauri::AppHandle, _agent_id: &str) -> Option<String> {
    let state = app_handle.try_state::<EngineState>()?;
    let runs = state.active_runs.lock();
    runs.keys().next().cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definitions_not_empty() {
        let defs = definitions();
        assert_eq!(defs.len(), 4);
        assert_eq!(defs[0].function.name, "canvas_save");
        assert_eq!(defs[1].function.name, "canvas_load");
        assert_eq!(defs[2].function.name, "canvas_list_dashboards");
        assert_eq!(defs[3].function.name, "canvas_delete_dashboard");
    }
}
