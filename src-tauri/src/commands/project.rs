// commands/project.rs — Thin wrappers for project/orchestrator commands.
// Orchestration logic lives in engine/orchestrator.rs.

use crate::commands::state::EngineState;
use crate::engine::types::*;
use log::{info, error};
use tauri::State;

#[tauri::command]
pub fn engine_projects_list(
    state: State<'_, EngineState>,
) -> Result<Vec<Project>, String> {
    state.store.list_projects().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_project_create(
    state: State<'_, EngineState>,
    project: Project,
) -> Result<(), String> {
    state.store.create_project(&project).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_project_update(
    state: State<'_, EngineState>,
    project: Project,
) -> Result<(), String> {
    state.store.update_project(&project).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_project_delete(
    state: State<'_, EngineState>,
    project_id: String,
) -> Result<(), String> {
    state.store.delete_project(&project_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_project_set_agents(
    state: State<'_, EngineState>,
    project_id: String,
    agents: Vec<ProjectAgent>,
) -> Result<(), String> {
    state.store.set_project_agents(&project_id, &agents).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_project_messages(
    state: State<'_, EngineState>,
    project_id: String,
    limit: Option<i64>,
) -> Result<Vec<ProjectMessage>, String> {
    state.store.get_project_messages(&project_id, limit.unwrap_or(100)).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn engine_project_run(
    app_handle: tauri::AppHandle,
    project_id: String,
) -> Result<String, String> {
    let run_id = uuid::Uuid::new_v4().to_string();
    let app = app_handle.clone();
    let pid = project_id.clone();

    // Spawn the orchestrator in background
    tauri::async_runtime::spawn(async move {
        match crate::engine::orchestrator::run_project(&app, &pid).await {
            Ok(text) => info!("[orchestrator] Project {} completed: {}...", pid, truncate_utf8(&text, 200)),
            Err(e) => error!("[orchestrator] Project {} failed: {}", pid, e),
        }
    });

    Ok(run_id)
}
