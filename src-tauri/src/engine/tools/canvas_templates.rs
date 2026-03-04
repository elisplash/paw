// Paw Agent Engine — Canvas Template tools (Phase 2)
// Agent tools for listing, instantiating, and creating dashboard templates.

use crate::atoms::types::*;
use crate::engine::state::EngineState;
use log::info;
use tauri::{Emitter, Manager};

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "canvas_list_templates".into(),
                description: "List available dashboard templates (built-in + user-created). Returns name, description, tags, and id for each.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "source": {
                            "type": "string",
                            "enum": ["builtin", "user", "community"],
                            "description": "Filter by source. Omit to list all."
                        }
                    },
                    "required": []
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "canvas_from_template".into(),
                description: "Instantiate a dashboard template. Creates a new dashboard with placeholder components from the template skeleton. Returns the new dashboard_id.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "template_id": {
                            "type": "string",
                            "description": "Template ID to instantiate"
                        },
                        "name": {
                            "type": "string",
                            "description": "Custom name for the new dashboard. Defaults to template name."
                        }
                    },
                    "required": ["template_id"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "canvas_create_template".into(),
                description: "Save the current dashboard's component structure as a reusable template. Strips live data and keeps component layout as a skeleton.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Template name"
                        },
                        "description": {
                            "type": "string",
                            "description": "Brief description of what this template is for"
                        },
                        "tags": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Searchable tags like ['ci','devops']"
                        },
                        "setup_prompt": {
                            "type": "string",
                            "description": "Agent prompt to populate the template with live data"
                        }
                    },
                    "required": ["name"]
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
        "canvas_list_templates" => exec_list(args, app_handle).map_err(|e| e.to_string()),
        "canvas_from_template" => exec_from(args, app_handle, agent_id).map_err(|e| e.to_string()),
        "canvas_create_template" => {
            exec_create(args, app_handle, agent_id).map_err(|e| e.to_string())
        }
        _ => return None,
    })
}

fn exec_list(args: &serde_json::Value, app_handle: &tauri::AppHandle) -> Result<String, String> {
    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    let source = args.get("source").and_then(|v| v.as_str());
    let templates = state.store.list_templates(source)?;

    if templates.is_empty() {
        return Ok("No templates available.".into());
    }

    let lines: Vec<String> = templates
        .iter()
        .map(|t| {
            format!(
                "- {} (id: {}, source: {}, tags: {}): {}",
                t.name, t.id, t.source, t.tags, t.description,
            )
        })
        .collect();

    Ok(format!(
        "{} template(s):\n{}",
        templates.len(),
        lines.join("\n")
    ))
}

fn exec_from(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    agent_id: &str,
) -> Result<String, String> {
    let template_id = args["template_id"]
        .as_str()
        .ok_or("Missing required parameter: template_id")?;

    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    let template = state
        .store
        .get_template(template_id)?
        .ok_or_else(|| format!("Template '{}' not found", template_id))?;

    let dashboard_name = args
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(&template.name);

    // Create the dashboard record.
    let dashboard_id = format!("dash-{}", uuid_v4());
    state.store.create_dashboard(
        &dashboard_id,
        dashboard_name,
        &template.icon,
        agent_id,
        None,
        Some(template_id),
        false,
        None,
        template.setup_prompt.as_deref(),
    )?;

    // Parse template components JSON and create skeleton components.
    let skeletons: Vec<serde_json::Value> = serde_json::from_str(&template.components)
        .map_err(|e| format!("Invalid template components JSON: {e}"))?;

    let mut created = 0u32;
    for skeleton in &skeletons {
        let comp_type = skeleton["type"].as_str().unwrap_or("card");
        let title = skeleton["title"].as_str().unwrap_or("Untitled");
        let data_hint = skeleton.get("data_hint").and_then(|v| v.as_str());
        let comp_id = format!("cc-{}", uuid_v4());

        // Build placeholder data from skeleton hints.
        let data = serde_json::json!({
            "placeholder": true,
            "data_hint": data_hint.unwrap_or(""),
        });
        let data_str = serde_json::to_string(&data).unwrap_or_default();

        state.store.upsert_canvas_component(
            &comp_id,
            None,
            Some(&dashboard_id),
            agent_id,
            comp_type,
            title,
            &data_str,
            None,
        )?;
        created += 1;
    }

    // Emit event so Canvas view can show the new dashboard.
    let _ = app_handle.emit(
        "dashboard-load",
        serde_json::json!({
            "dashboard_id": dashboard_id,
            "name": dashboard_name,
            "icon": template.icon,
            "component_count": created,
            "from_template": template_id,
        }),
    );

    info!(
        "[canvas] Dashboard created from template: dash={} tpl={} components={}",
        dashboard_id, template_id, created
    );

    Ok(format!(
        "Dashboard '{}' created from template '{}' with {} placeholder components. \
         ID: {}. Use canvas_update to populate each component with live data.",
        dashboard_name, template.name, created, dashboard_id,
    ))
}

fn exec_create(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    agent_id: &str,
) -> Result<String, String> {
    let name = args["name"]
        .as_str()
        .ok_or("Missing required parameter: name")?;
    let description = args
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let tags = args
        .get("tags")
        .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "[]".into()))
        .unwrap_or_else(|| "[]".into());
    let setup_prompt = args.get("setup_prompt").and_then(|v| v.as_str());

    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    // Get the active session's components to use as the skeleton.
    let session_id = get_active_session(app_handle, agent_id)
        .ok_or("No active session — cannot create template from current canvas")?;

    let components = state.store.list_canvas_by_session(&session_id)?;
    if components.is_empty() {
        return Err("Current canvas is empty — nothing to template.".into());
    }

    // Strip live data, keep structure as skeleton.
    let skeletons: Vec<serde_json::Value> = components
        .iter()
        .map(|c| {
            serde_json::json!({
                "type": c.component_type,
                "title": c.title,
                "data_hint": format!("Data for {}", c.title),
            })
        })
        .collect();
    let components_json = serde_json::to_string(&skeletons)
        .map_err(|e| format!("Failed to serialize skeleton: {e}"))?;

    let template_id = format!("tpl-{}", uuid_v4());
    state.store.create_template(
        &template_id,
        name,
        description,
        "dashboard_customize",
        &components_json,
        &tags,
        setup_prompt,
        "user",
    )?;

    info!(
        "[canvas] Template created: id={} name={} components={}",
        template_id,
        name,
        skeletons.len()
    );

    Ok(format!(
        "Template '{}' created with {} component skeleton(s). ID: {}",
        name,
        skeletons.len(),
        template_id,
    ))
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
        assert_eq!(defs.len(), 3);
        assert_eq!(defs[0].function.name, "canvas_list_templates");
        assert_eq!(defs[1].function.name, "canvas_from_template");
        assert_eq!(defs[2].function.name, "canvas_create_template");
    }
}
