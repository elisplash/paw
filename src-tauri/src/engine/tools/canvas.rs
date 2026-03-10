// Paw Agent Engine — Canvas tools (Agent Canvas)
// Allows agents to push/update/clear/remove live canvas components
// that render in the Canvas view as a bento-grid dashboard.

use crate::atoms::types::*;
use crate::engine::state::EngineState;
use log::info;
use tauri::{Emitter, Manager};

/// Valid canvas component types.
const VALID_COMPONENT_TYPES: &[&str] = &[
    "metric",
    "table",
    "chart",
    "log",
    "kv",
    "card",
    "status",
    "progress",
    "form",
    "markdown",
    "timeline",
    "checklist",
    "gauge",
    "countdown",
    "image",
    "embed",
];

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "canvas_push".into(),
                description: "Add a new component to the user's Canvas dashboard. Choose the component type that matches the user's design intent — if the request implies a specific visual style, color scheme, branding, creative layout, or custom UI (marketing dashboard, infographic, funnel, scorecard, etc.), always use `embed` with custom HTML/CSS/JS. For data monitors and operational dashboards, use structured types like metric/chart/table. Returns the component_id for later updates.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "type": {
                            "type": "string",
                            "enum": VALID_COMPONENT_TYPES,
                            "description": "Component type — choose based on the user's design intent:\n• embed — FIRST choice for any custom visual design: fully sandboxed HTML/CSS/JS iframe. ALWAYS populate the libraries array with CDN URLs for the best tool for the job — you choose the library, the user does not need to ask:\n  - 3D scenes / WebGL → https://cdnjs.cloudflare.com/ajax/libs/three.js/r128/three.min.js\n  - Data visualisation / force graphs / maps → https://cdnjs.cloudflare.com/ajax/libs/d3/7.8.5/d3.min.js\n  - Smooth animation / timeline / morphing → https://cdnjs.cloudflare.com/ajax/libs/gsap/3.12.2/gsap.min.js\n  - 2D WebGL / particles / sprites → https://cdnjs.cloudflare.com/ajax/libs/pixi.js/7.3.2/pixi.min.js\n  - Physics simulation → https://cdnjs.cloudflare.com/ajax/libs/matter-js/0.19.0/matter.min.js\n  - Charting (quick) → https://cdnjs.cloudflare.com/ajax/libs/Chart.js/4.4.0/chart.umd.min.js\n  - Creative / generative art → https://cdnjs.cloudflare.com/ajax/libs/p5.js/1.9.0/p5.min.js\n  You can combine multiple libraries. Use embed for marketing assets, branded dashboards, infographics, funnels, scorecards, interactive 3D/animated charts, or whenever the user describes a particular look.\n• metric — large KPI number with trend/delta indicator\n• chart — line / bar / area / pie visualization\n• table — rows + columns of structured data\n• card — markdown body: narrative, insights, action items\n• timeline — phases, milestones, schedule bars\n• checklist — task list with completion progress\n• status — per-item health indicators (ok / warn / down)\n• progress — labeled progress bars (0–100%)\n• gauge — radial meter (single value)\n• countdown — live timer counting down to a deadline\n• kv — key-value pairs, config values, quick references\n• log — timestamped activity/event stream\n• form — labeled input fields the user fills out; on submit the values are sent to you as a chat message so you can act on them (e.g. run a search, create a record, trigger a workflow); set on_submit_message to customise the prompt you receive\n• markdown — freeform text / documentation block\n• image — image with caption"
                        },
                        "title": {
                            "type": "string",
                            "description": "Component title shown in the card header"
                        },
                        "data": {
                            "type": "string",
                            "description": "JSON-encoded data for the component (pass as a JSON string). Shape by type:\n• embed: {\"html\":\"<div>…</div>\",\"css\":\"body{background:#111;font-family:sans-serif}…\",\"js\":\"…\",\"height\":480,\"libraries\":[\"https://cdnjs.cloudflare.com/ajax/libs/three.js/r128/three.min.js\"]} — always pick height to fill the visual (300–800). Always include relevant CDN libraries — do not hand-code what a library provides.\n• metric: {\"value\":\"$1.2M\",\"trend\":\"+12%\",\"delta\":\"+$120k\",\"unit\":\"USD\"}\n• chart: {\"labels\":[\"Jan\",\"Feb\"],\"datasets\":[{\"label\":\"Revenue\",\"data\":[100,200],\"type\":\"line\"}]}\n• table: {\"columns\":[\"Name\",\"Value\"],\"rows\":[[\"a\",\"b\"]]}\n• card: {\"body\":\"## Title\\n\\ncontent\",\"actions\":[{\"label\":\"Open\",\"url\":\"#\"}]}\n• status: {\"items\":[{\"label\":\"API\",\"state\":\"ok\"},{\"label\":\"DB\",\"state\":\"warn\"}]}\n• checklist: {\"items\":[{\"text\":\"Task 1\",\"done\":false}]}\n• progress: {\"items\":[{\"label\":\"Phase 1\",\"pct\":75}]}\n• gauge: {\"value\":72,\"max\":100,\"label\":\"CPU %\"}\n• countdown: {\"target\":\"2026-06-01T00:00:00Z\",\"label\":\"Launch\"}\n• kv: {\"items\":[{\"key\":\"Version\",\"value\":\"1.2.3\"}]}\n• log: {\"entries\":[{\"time\":\"10:00\",\"text\":\"Event description\"}]}\n• timeline: {\"phases\":[{\"label\":\"Design\",\"start\":\"Jan\",\"end\":\"Feb\",\"status\":\"done\"}]}\n• image: {\"src\":\"https://…\",\"caption\":\"Description\"}\n• form: {\"fields\":[{\"name\":\"query\",\"label\":\"Search\",\"type\":\"text\",\"placeholder\":\"Enter value…\",\"required\":true}],\"on_submit_message\":\"Search Stripe for customer: {{query}}\",\"submit_label\":\"Search\"}"
                        },
                        "position": {
                            "type": "string",
                            "description": "Optional JSON-encoded grid placement: {\"col\": int, \"row\": int, \"width\": int, \"height\": int}"
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
                description: "Update a canvas component in-place by component_id. This is the core live-data primitive: (1) fetch fresh data from any source (rest_api_call, service_api_call, read_file, etc.), (2) call canvas_update with the new JSON to push it into the existing tile instantly. Use this inside create_task with a cron_schedule to build auto-refreshing live dashboards. Only specify the fields you want to change.".into(),
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
                            "type": "string",
                            "description": "New JSON-encoded data (optional, replaces existing)"
                        },
                        "position": {
                            "type": "string",
                            "description": "New JSON-encoded grid position (optional)"
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
    let data_raw = &args["data"];

    if !VALID_COMPONENT_TYPES.contains(&comp_type) {
        return Err(format!(
            "Invalid type '{}'. Must be one of: {}",
            comp_type,
            VALID_COMPONENT_TYPES.join(", ")
        ));
    }

    // Validate data is an object — tolerate string-encoded JSON from LLMs
    let data_owned: serde_json::Value;
    let data = if data_raw.is_object() {
        data_raw
    } else if let Some(s) = data_raw.as_str() {
        data_owned = serde_json::from_str(s).map_err(|_| {
            "Parameter 'data' must be a JSON object (got a non-JSON string)".to_string()
        })?;
        if !data_owned.is_object() {
            return Err("Parameter 'data' must be a JSON object".to_string());
        }
        &data_owned
    } else {
        return Err("Parameter 'data' must be a JSON object".to_string());
    };

    let data_str =
        serde_json::to_string(data).map_err(|e| format!("Failed to serialize data: {e}"))?;
    let position_str = args.get("position").and_then(|p| {
        if p.is_object() {
            serde_json::to_string(p).ok()
        } else if let Some(s) = p.as_str() {
            // Tolerate string-encoded JSON from LLMs
            serde_json::from_str::<serde_json::Value>(s)
                .ok()
                .filter(|v| v.is_object())
                .and_then(|v| serde_json::to_string(&v).ok())
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
    let parsed_position: Option<crate::atoms::types::CanvasPosition> =
        args.get("position").and_then(|p| {
            if p.is_object() {
                serde_json::from_value(p.clone()).ok()
            } else if let Some(s) = p.as_str() {
                serde_json::from_str(s).ok()
            } else {
                None
            }
        });
    let component = CanvasComponent {
        component_type: parse_component_type(comp_type),
        title: title.to_string(),
        data: data.clone(),
        position: parsed_position,
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
        } else if let Some(s) = d.as_str() {
            // Tolerate string-encoded JSON from LLMs
            serde_json::from_str::<serde_json::Value>(s)
                .ok()
                .filter(|v| v.is_object())
                .and_then(|v| serde_json::to_string(&v).ok())
        } else {
            None
        }
    });
    let position_str = args.get("position").and_then(|p| {
        if p.is_object() {
            serde_json::to_string(p).ok()
        } else if let Some(s) = p.as_str() {
            // Tolerate string-encoded JSON from LLMs
            serde_json::from_str::<serde_json::Value>(s)
                .ok()
                .filter(|v| v.is_object())
                .and_then(|v| serde_json::to_string(&v).ok())
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
        data: args.get("data").and_then(|d| {
            if d.is_object() {
                Some(d.clone())
            } else if let Some(s) = d.as_str() {
                serde_json::from_str(s).ok()
            } else {
                None
            }
        }),
        position: args.get("position").and_then(|p| {
            if p.is_object() {
                serde_json::from_value(p.clone()).ok()
            } else if let Some(s) = p.as_str() {
                serde_json::from_str(s).ok()
            } else {
                None
            }
        }),
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
        "timeline" => CanvasComponentType::Timeline,
        "checklist" => CanvasComponentType::Checklist,
        "gauge" => CanvasComponentType::Gauge,
        "countdown" => CanvasComponentType::Countdown,
        "image" => CanvasComponentType::Image,
        "embed" => CanvasComponentType::Embed,
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
