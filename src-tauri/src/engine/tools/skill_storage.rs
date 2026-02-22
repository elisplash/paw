// tools/skill_storage.rs — Persistent KV store tools for extensions (Phase F.6).
// Gives agents read/write access to namespaced skill storage.

use crate::atoms::types::*;
use crate::engine::state::EngineState;
use log::info;
use tauri::Manager;

// ── Tool definitions ───────────────────────────────────────────────────

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "skill_store_set".into(),
                description: "Store a key-value pair in a skill's persistent storage. \
                    Each skill has its own isolated namespace. Values persist across sessions."
                    .into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "skill_id": {
                            "type": "string",
                            "description": "The skill ID to store data for"
                        },
                        "key": {
                            "type": "string",
                            "description": "Storage key (e.g. 'last_sync', 'config')"
                        },
                        "value": {
                            "type": "string",
                            "description": "Value to store (plain text or JSON string)"
                        }
                    },
                    "required": ["skill_id", "key", "value"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "skill_store_get".into(),
                description:
                    "Retrieve a value from a skill's persistent storage by key.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "skill_id": {
                            "type": "string",
                            "description": "The skill ID"
                        },
                        "key": {
                            "type": "string",
                            "description": "Storage key to retrieve"
                        }
                    },
                    "required": ["skill_id", "key"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "skill_store_list".into(),
                description:
                    "List all key-value pairs stored for a skill.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "skill_id": {
                            "type": "string",
                            "description": "The skill ID"
                        }
                    },
                    "required": ["skill_id"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "skill_store_delete".into(),
                description:
                    "Delete a key from a skill's persistent storage.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "skill_id": {
                            "type": "string",
                            "description": "The skill ID"
                        },
                        "key": {
                            "type": "string",
                            "description": "Storage key to delete"
                        }
                    },
                    "required": ["skill_id", "key"]
                }),
            },
        },
    ]
}

// ── Execute dispatcher ─────────────────────────────────────────────────

pub async fn execute(
    name: &str,
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    _agent_id: &str,
) -> Option<Result<String, String>> {
    Some(match name {
        "skill_store_set" => execute_set(args, app_handle).map_err(|e| e.to_string()),
        "skill_store_get" => execute_get(args, app_handle).map_err(|e| e.to_string()),
        "skill_store_list" => execute_list(args, app_handle).map_err(|e| e.to_string()),
        "skill_store_delete" => execute_delete(args, app_handle).map_err(|e| e.to_string()),
        _ => return None,
    })
}

// ── Private handlers ───────────────────────────────────────────────────

fn execute_set(args: &serde_json::Value, app_handle: &tauri::AppHandle) -> Result<String, String> {
    let skill_id = args["skill_id"]
        .as_str()
        .ok_or("Missing required parameter: skill_id")?;
    let key = args["key"]
        .as_str()
        .ok_or("Missing required parameter: key")?;
    let value = args["value"]
        .as_str()
        .ok_or("Missing required parameter: value")?;

    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;
    state.store.skill_store_set(skill_id, key, value)?;

    info!("[engine] Skill storage set: {}:{}", skill_id, key);
    Ok(format!("Stored '{key}' for skill '{skill_id}'."))
}

fn execute_get(args: &serde_json::Value, app_handle: &tauri::AppHandle) -> Result<String, String> {
    let skill_id = args["skill_id"]
        .as_str()
        .ok_or("Missing required parameter: skill_id")?;
    let key = args["key"]
        .as_str()
        .ok_or("Missing required parameter: key")?;

    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;
    let result = state.store.skill_store_get(skill_id, key)?;

    match result {
        Some(v) => Ok(v),
        None => Ok(format!("No value found for key '{key}' in skill '{skill_id}'.")),
    }
}

fn execute_list(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
) -> Result<String, String> {
    let skill_id = args["skill_id"]
        .as_str()
        .ok_or("Missing required parameter: skill_id")?;

    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;
    let items = state.store.skill_store_list(skill_id)?;

    if items.is_empty() {
        return Ok(format!("No storage entries for skill '{skill_id}'."));
    }

    let json = serde_json::to_string_pretty(&items).unwrap_or_default();
    Ok(json)
}

fn execute_delete(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
) -> Result<String, String> {
    let skill_id = args["skill_id"]
        .as_str()
        .ok_or("Missing required parameter: skill_id")?;
    let key = args["key"]
        .as_str()
        .ok_or("Missing required parameter: key")?;

    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;
    state.store.skill_store_delete(skill_id, key)?;

    info!("[engine] Skill storage delete: {}:{}", skill_id, key);
    Ok(format!("Deleted '{key}' from skill '{skill_id}'."))
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_definitions_valid() {
        let defs = definitions();
        assert_eq!(defs.len(), 4);
        assert_eq!(defs[0].function.name, "skill_store_set");
        assert_eq!(defs[1].function.name, "skill_store_get");
        assert_eq!(defs[2].function.name, "skill_store_list");
        assert_eq!(defs[3].function.name, "skill_store_delete");
        for d in &defs {
            assert_eq!(d.tool_type, "function");
            assert!(!d.function.description.is_empty());
        }
    }

    #[test]
    fn test_definitions_have_required_params() {
        let defs = definitions();
        for d in &defs {
            let params = &d.function.parameters;
            assert!(params["required"].is_array());
            assert!(params["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v.as_str() == Some("skill_id")));
        }
    }
}
