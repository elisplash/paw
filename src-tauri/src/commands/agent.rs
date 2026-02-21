// Paw Commands — Agent System Layer
//
// Thin Tauri command wrappers for:
//   - Agent CRUD    (engine_list_all_agents, _create_agent, _delete_agent)
//   - Agent Files   (engine_agent_file_list, _get, _set, _delete)
//
// All commands are 1-3 lines: extract, delegate to SessionStore, return.

use tauri::State;
use log::info;

use crate::commands::state::EngineState;
use crate::engine::types::*;

// ── Agent CRUD ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn engine_list_all_agents(
    state: State<'_, EngineState>,
) -> Result<Vec<serde_json::Value>, String> {
    let agents = state.store.list_all_agents()?;
    Ok(agents
        .into_iter()
        .map(|(project_id, agent)| {
            serde_json::json!({
                "project_id": project_id,
                "agent_id": agent.agent_id,
                "role": agent.role,
                "specialty": agent.specialty,
                "status": agent.status,
                "current_task": agent.current_task,
                "model": agent.model,
                "system_prompt": agent.system_prompt,
                "capabilities": agent.capabilities,
            })
        })
        .collect())
}

/// Create a standalone agent (user-created, not from orchestrator).
/// Uses project_id="_standalone" as a sentinel so it lives alongside project agents
/// but is clearly user-created.
#[tauri::command]
pub fn engine_create_agent(
    state: State<'_, EngineState>,
    agent_id: String,
    role: String,
    specialty: Option<String>,
    model: Option<String>,
    system_prompt: Option<String>,
    capabilities: Option<Vec<String>>,
) -> Result<(), String> {
    let agent = ProjectAgent {
        agent_id: agent_id.clone(),
        role,
        specialty: specialty.unwrap_or_else(|| "general".into()),
        status: "idle".into(),
        current_task: None,
        model,
        system_prompt,
        capabilities: capabilities.unwrap_or_default(),
    };
    state.store.add_project_agent("_standalone", &agent)?;
    info!("[engine] Created standalone agent: {}", agent_id);
    Ok(())
}

/// Delete a standalone agent by agent_id.
#[tauri::command]
pub fn engine_delete_agent(
    state: State<'_, EngineState>,
    agent_id: String,
) -> Result<(), String> {
    state.store.delete_agent("_standalone", &agent_id)?;
    info!("[engine] Deleted standalone agent: {}", agent_id);
    Ok(())
}

// ── Agent Files (Soul / Persona) ──────────────────────────────────────────────

#[tauri::command]
pub fn engine_agent_file_list(
    state: State<'_, EngineState>,
    agent_id: Option<String>,
) -> Result<Vec<AgentFile>, String> {
    let aid = agent_id.unwrap_or_else(|| "default".into());
    state.store.list_agent_files(&aid).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_agent_file_get(
    state: State<'_, EngineState>,
    agent_id: Option<String>,
    file_name: String,
) -> Result<Option<AgentFile>, String> {
    let aid = agent_id.unwrap_or_else(|| "default".into());
    state.store.get_agent_file(&aid, &file_name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_agent_file_set(
    state: State<'_, EngineState>,
    agent_id: Option<String>,
    file_name: String,
    content: String,
) -> Result<(), String> {
    let aid = agent_id.unwrap_or_else(|| "default".into());
    info!(
        "[engine] Setting agent file {}/{} ({} bytes)",
        aid,
        file_name,
        content.len()
    );
    state.store.set_agent_file(&aid, &file_name, &content).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_agent_file_delete(
    state: State<'_, EngineState>,
    agent_id: Option<String>,
    file_name: String,
) -> Result<(), String> {
    let aid = agent_id.unwrap_or_else(|| "default".into());
    state.store.delete_agent_file(&aid, &file_name).map_err(|e| e.to_string())
}
