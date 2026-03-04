// Canvas Commands — Tauri IPC wrappers for Agent Canvas CRUD.
// Thin layer: deserialise, delegate to SessionStore, serialise.

use crate::atoms::types::CanvasComponentRow;
use crate::engine::state::EngineState;
use tauri::State;

/// List canvas components for a given session.
#[tauri::command]
pub fn engine_canvas_list_by_session(
    state: State<'_, EngineState>,
    session_id: String,
) -> Result<Vec<CanvasComponentRow>, String> {
    state
        .store
        .list_canvas_by_session(&session_id)
        .map_err(|e| e.to_string())
}

/// List canvas components for a saved dashboard.
#[tauri::command]
pub fn engine_canvas_list_by_dashboard(
    state: State<'_, EngineState>,
    dashboard_id: String,
) -> Result<Vec<CanvasComponentRow>, String> {
    state
        .store
        .list_canvas_by_dashboard(&dashboard_id)
        .map_err(|e| e.to_string())
}

/// Delete a single canvas component by ID.
#[tauri::command]
pub fn engine_canvas_delete_component(
    state: State<'_, EngineState>,
    component_id: String,
) -> Result<bool, String> {
    state
        .store
        .delete_canvas_component(&component_id)
        .map_err(|e| e.to_string())
}

/// Clear all canvas components for a session.
#[tauri::command]
pub fn engine_canvas_clear_session(
    state: State<'_, EngineState>,
    session_id: String,
) -> Result<u64, String> {
    state
        .store
        .clear_canvas_session(&session_id)
        .map_err(|e| e.to_string())
}
