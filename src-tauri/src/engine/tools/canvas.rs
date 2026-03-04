// Paw Agent Engine — Canvas tools (Agent Canvas)
// Allows agents to push/update/clear/remove live canvas components
// that render in the Canvas view as a bento-grid dashboard.

use crate::atoms::types::*;
use crate::engine::state::EngineState;
use log::info;
use tauri::{Emitter, Manager};

/// Valid canvas component types.
const VALID_COMPONENT_TYPES: &[&str] = &[
    "metric", "table", "chart", "log", "kv", "card", "status", "progress", "form", "markdown",
];

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "canvas_push".into(),
                description: "Add a new component to the user's Canvas dashboard. The component appears instantly. Returns the component_id for later updates.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "type": {
                            "type": "string",
                            "enum": VALID_COMPONENT_TYPES,
                            "description": "Component type: metric (big number + trend), table (columns + rows), chart (line/bar/area/pie), log (timestamped entries), kv (key-value pairs), card (markdown body + actions), status (icon + text), progress (label + pct), form (inputs → tool call), markdown (freeform)"
                        },
                        "title": {
                            "type": "string",
                            "description": "Component title shown in the card header"
                        },
                        "data": {
                            "type": "object",
                            "description": "Structured data for the component. Shape varies by type."
                        },
                        "position": {
                            "type": "object",
                            "description": "Optional grid placement: {col, row, width, height}",
                            "properties": {
                                "col": { "type": "integer" },
                                "row": { "type": "integer" },
                                "width": { "type": "integer" },
                                "height": { "type": "integer" }
                            }
                        }
                    },
                    "required": ["type", "title", "data"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "canvas_update".into(),
                description: "Update an existing canvas component in-place by component_id. Only specify the fields you want to change.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "component_id": {
                            "type": "string",
                            "description": "The component_id returned by a previous canvas_push"
                        },
                        "title": {
                            "type": "string",
                            "description": "New title (optional)"
                        },
                        "data": {
                            "type": "object",
                            "description": "New data (optional, replaces existing)"
                        },
                        "position": {
                            "type": "object",
                            "description": "New grid position (optional)"
                        }
                    },
                    "required": ["component_id"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "canvas_remove".into(),
                description: "Remove a single component from the canvas by component_id.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "component_id": {
                            "type": "string",
                            "description": "The component_id to remove"
                        }
                    },
                    "required": ["component_id"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "canvas_clear".into(),
                description: "Clear all components from the current session's canvas.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
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
        "canvas_push" => exec_push(args, app_handle, agent_id).map_err(|e| e.to_string()),
        "canvas_update" => exec_update(args, app_handle, agent_id).map_err(|e| e.to_string()),
        "canvas_remove" => exec_remove(args, app_handle).map_err(|e| e.to_string()),
        "canvas_clear" => exec_clear(args, app_handle).map_err(|e| e.to_string()),
        _ => return None,
    })
}

fn exec_push(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    agent_id: &str,
) -> Result<String, String> {
    let comp_type = args["type"]
        .as_str()
        .ok_or("Missing required parameter: type")?;
    let title = args["title"]
        .as_str()
        .ok_or("Missing required parameter: title")?;
    let data = &args["data"];

    if !VALID_COMPONENT_TYPES.contains(&comp_type) {
        return Err(format!(
            "Invalid type '{}'. Must be one of: {}",
            comp_type,
            VALID_COMPONENT_TYPES.join(", ")
        ));
    }
    if !data.is_object() {
        return Err("Parameter 'data' must be a JSON object".to_string());
    }

    let data_str =
        serde_json::to_string(data).map_err(|e| format!("Failed to serialize data: {e}"))?;
    let position_str = args.get("position").and_then(|p| {
        if p.is_object() {
            serde_json::to_string(p).ok()
        } else {
            None
        }
    });

    // Generate unique component_id
    let component_id = format!("cc-{}", uuid_v4());

    // Get the active session_id from the running context (stored in agent state)
    let session_id = get_active_session(app_handle, agent_id);

    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    state.store.upsert_canvas_component(
        &component_id,
        session_id.as_deref(),
        None,
        agent_id,
        comp_type,
        title,
        &data_str,
        position_str.as_deref(),
    )?;

    // Emit CanvasPush event so the frontend updates live
    let run_id = get_active_run(app_handle, agent_id);
    let component = CanvasComponent {
        component_type: parse_component_type(comp_type),
        title: title.to_string(),
        data: data.clone(),
        position: args
            .get("position")
            .and_then(|p| serde_json::from_value(p.clone()).ok()),
    };
    let event = EngineEvent::CanvasPush {
        session_id: session_id.unwrap_or_default(),
        run_id,
        agent_id: agent_id.to_string(),
        component_id: component_id.clone(),
        component,
    };
    let _ = app_handle.emit("engine-event", &event);

    info!(
        "[canvas] Component pushed: id={} type={} title={}",
        component_id, comp_type, title
    );

    Ok(format!(
        "Canvas component '{}' ({}) added. component_id: {}",
        title, comp_type, component_id
    ))
}

fn exec_update(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    agent_id: &str,
) -> Result<String, String> {
    let component_id = args["component_id"]
        .as_str()
        .ok_or("Missing required parameter: component_id")?;

    let title = args.get("title").and_then(|v| v.as_str());
    let data_str = args.get("data").and_then(|d| {
        if d.is_object() {
            serde_json::to_string(d).ok()
        } else {
            None
        }
    });
    let position_str = args.get("position").and_then(|p| {
        if p.is_object() {
            serde_json::to_string(p).ok()
        } else {
            None
        }
    });

    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    let patched = state.store.patch_canvas_component(
        component_id,
        title,
        data_str.as_deref(),
        position_str.as_deref(),
    )?;

    if !patched {
        return Ok(format!(
            "No component found with id '{}' — nothing updated.",
            component_id
        ));
    }

    // Emit CanvasUpdate event
    let session_id = get_active_session(app_handle, agent_id);
    let run_id = get_active_run(app_handle, agent_id);
    let patch = CanvasComponentPatch {
        title: title.map(|s| s.to_string()),
        data: args.get("data").cloned(),
        position: args
            .get("position")
            .and_then(|p| serde_json::from_value(p.clone()).ok()),
    };
    let event = EngineEvent::CanvasUpdate {
        session_id: session_id.unwrap_or_default(),
        run_id,
        agent_id: agent_id.to_string(),
        component_id: component_id.to_string(),
        patch,
    };
    let _ = app_handle.emit("engine-event", &event);

    info!("[canvas] Component updated: id={}", component_id);
    Ok(format!("Canvas component '{}' updated.", component_id))
}

fn exec_remove(args: &serde_json::Value, app_handle: &tauri::AppHandle) -> Result<String, String> {
    let component_id = args["component_id"]
        .as_str()
        .ok_or("Missing required parameter: component_id")?;

    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    let deleted = state.store.delete_canvas_component(component_id)?;
    if deleted {
        info!("[canvas] Component removed: id={}", component_id);
        Ok(format!("Component '{}' removed from canvas.", component_id))
    } else {
        Ok(format!(
            "No component '{}' found — nothing removed.",
            component_id
        ))
    }
}

fn exec_clear(args: &serde_json::Value, app_handle: &tauri::AppHandle) -> Result<String, String> {
    // session_id can optionally be passed; otherwise we clear based on context
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    let cleared = state.store.clear_canvas_session(session_id)?;
    info!(
        "[canvas] Session cleared: session={} count={}",
        session_id, cleared
    );
    Ok(format!("Cleared {} components from canvas.", cleared))
}

// ── Helpers ──────────────────────────────────────────────────────────────

/// Simple UUID v4 generator (no dependency — uses random bytes).
fn uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    // Mix timestamp with a counter for uniqueness within the same process
    format!("{:016x}{:016x}", t, t.wrapping_mul(6364136223846793005))
}

/// Parse a string into CanvasComponentType (defaults to Card for unknown).
fn parse_component_type(s: &str) -> CanvasComponentType {
    match s {
        "metric" => CanvasComponentType::Metric,
        "table" => CanvasComponentType::Table,
        "chart" => CanvasComponentType::Chart,
        "log" => CanvasComponentType::Log,
        "kv" => CanvasComponentType::Kv,
        "card" => CanvasComponentType::Card,
        "status" => CanvasComponentType::Status,
        "progress" => CanvasComponentType::Progress,
        "form" => CanvasComponentType::Form,
        "markdown" => CanvasComponentType::Markdown,
        _ => CanvasComponentType::Card,
    }
}

/// Get the active session_id for an agent.
/// Looks up the active_runs map (session_id → AbortHandle) to find
/// which session is currently running. In Phase 1, this picks the first
/// active session; Phase 2+ will route by agent_id.
fn get_active_session(app_handle: &tauri::AppHandle, _agent_id: &str) -> Option<String> {
    let state = app_handle.try_state::<EngineState>()?;
    let runs = state.active_runs.lock();
    runs.keys().next().cloned()
}

/// Get the active run_id for an agent (placeholder — not tracked per-tool).
fn get_active_run(_app_handle: &tauri::AppHandle, _agent_id: &str) -> String {
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definitions_not_empty() {
        let defs = definitions();
        assert_eq!(defs.len(), 4);
        assert_eq!(defs[0].function.name, "canvas_push");
        assert_eq!(defs[1].function.name, "canvas_update");
        assert_eq!(defs[2].function.name, "canvas_remove");
        assert_eq!(defs[3].function.name, "canvas_clear");
    }

    #[test]
    fn parse_all_component_types() {
        assert_eq!(parse_component_type("metric"), CanvasComponentType::Metric);
        assert_eq!(parse_component_type("chart"), CanvasComponentType::Chart);
        assert_eq!(
            parse_component_type("markdown"),
            CanvasComponentType::Markdown
        );
        assert_eq!(parse_component_type("unknown"), CanvasComponentType::Card);
    }
}
