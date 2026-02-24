// Paw Agent Engine — Soul tools
// soul_read, soul_write, soul_list

use crate::atoms::types::*;
use crate::engine::state::EngineState;
use log::info;
use tauri::Manager;
use crate::atoms::error::EngineResult;

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "soul_read".into(),
                description: "Read one of your own soul/persona files. These files define who you are. Available files: IDENTITY.md (name, role, purpose), SOUL.md (personality, values, voice), USER.md (facts about the user), AGENTS.md (other agents you know about), TOOLS.md (your tool preferences and notes).".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_name": { "type": "string", "description": "The soul file to read, e.g. 'SOUL.md', 'IDENTITY.md', 'USER.md', 'AGENTS.md', 'TOOLS.md'" }
                    },
                    "required": ["file_name"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "soul_write".into(),
                description: "Update one of your own soul/persona files. Use this to evolve your personality, record things about the user, or refine your identity. Be thoughtful — these files shape who you are across all conversations.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_name": { "type": "string", "description": "The soul file to write, e.g. 'SOUL.md', 'IDENTITY.md', 'USER.md', 'AGENTS.md', 'TOOLS.md'" },
                        "content": { "type": "string", "description": "The full new content for the file (Markdown format)" }
                    },
                    "required": ["file_name", "content"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "soul_list".into(),
                description: "List all your soul/persona files and their sizes. Use this to see what files exist before reading or writing them.".into(),
                parameters: serde_json::json!({"type": "object", "properties": {}}),
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
    match name {
        "soul_read"  => Some(execute_soul_read(args, app_handle, agent_id).await.map_err(|e| e.to_string())),
        "soul_write" => Some(execute_soul_write(args, app_handle, agent_id).await.map_err(|e| e.to_string())),
        "soul_list"  => Some(execute_soul_list(app_handle, agent_id).await.map_err(|e| e.to_string())),
        _ => None,
    }
}

async fn execute_soul_read(args: &serde_json::Value, app_handle: &tauri::AppHandle, agent_id: &str) -> EngineResult<String> {
    let file_name = args["file_name"].as_str().ok_or("soul_read: missing 'file_name' argument")?;
    info!("[engine] soul_read: {} (agent={})", file_name, agent_id);
    let state = app_handle.try_state::<EngineState>().ok_or("Engine state not available")?;
    match state.store.get_agent_file(agent_id, file_name)? {
        Some(file) => Ok(format!("# {}\n\n{}", file.file_name, file.content)),
        None => Ok(format!("File '{}' does not exist yet. You can create it with soul_write.", file_name)),
    }
}

async fn execute_soul_write(args: &serde_json::Value, app_handle: &tauri::AppHandle, agent_id: &str) -> EngineResult<String> {
    let file_name = args["file_name"].as_str().ok_or("soul_write: missing 'file_name' argument")?;
    let content = args["content"].as_str().ok_or("soul_write: missing 'content' argument")?;
    let allowed_files = ["IDENTITY.md", "SOUL.md", "USER.md", "AGENTS.md", "TOOLS.md"];
    if !allowed_files.contains(&file_name) {
        return Err(format!("soul_write: '{}' is not an allowed soul file. Allowed: {}", file_name, allowed_files.join(", ")).into());
    }
    info!("[engine] soul_write: {} ({} bytes, agent={})", file_name, content.len(), agent_id);
    let state = app_handle.try_state::<EngineState>().ok_or("Engine state not available")?;
    state.store.set_agent_file(agent_id, file_name, content)?;
    Ok(format!("Successfully updated {}. This change will take effect in future conversations.", file_name))
}

async fn execute_soul_list(app_handle: &tauri::AppHandle, agent_id: &str) -> EngineResult<String> {
    info!("[engine] soul_list (agent={})", agent_id);
    let state = app_handle.try_state::<EngineState>().ok_or("Engine state not available")?;
    let files = state.store.list_agent_files(agent_id)?;
    if files.is_empty() {
        return Ok("No soul files exist yet. You can create them with soul_write. Available files:\n- IDENTITY.md (your name, role, purpose)\n- SOUL.md (personality, values, voice)\n- USER.md (facts about the user)\n- AGENTS.md (other agents)\n- TOOLS.md (tool preferences)".into());
    }
    let mut output = String::from("Soul files:\n");
    for f in &files {
        output.push_str(&format!("- {} ({} bytes, updated {})\n", f.file_name, f.content.len(), f.updated_at));
    }
    output.push_str("\nUse soul_read to view a file, soul_write to update one.");
    Ok(output)
}
