// Dashboard Tab & Window Commands — Tauri IPC wrappers.
// Tab operations + pop-out window geometry persistence.

use crate::atoms::types::{DashboardTabRow, DashboardWindowRow};
use crate::engine::state::EngineState;
use tauri::State;

// ── Tab Operations ──────────────────────────────────────────────────────

/// Open a new tab for a dashboard.
#[tauri::command]
pub fn engine_open_tab(
    state: State<'_, EngineState>,
    tab_id: String,
    dashboard_id: String,
    window_id: Option<String>,
) -> Result<(), String> {
    let wid = window_id.as_deref().unwrap_or("main");
    state
        .store
        .open_tab(&tab_id, &dashboard_id, wid)
        .map_err(|e| e.to_string())
}

/// Close a tab by ID.
#[tauri::command]
pub fn engine_close_tab(state: State<'_, EngineState>, tab_id: String) -> Result<bool, String> {
    state.store.close_tab(&tab_id).map_err(|e| e.to_string())
}

/// Set a tab as the active tab in its window.
#[tauri::command]
pub fn engine_activate_tab(
    state: State<'_, EngineState>,
    tab_id: String,
    window_id: Option<String>,
) -> Result<(), String> {
    let wid = window_id.as_deref().unwrap_or("main");
    state
        .store
        .activate_tab(&tab_id, wid)
        .map_err(|e| e.to_string())
}

/// Reorder a tab to a new position.
#[tauri::command]
pub fn engine_reorder_tab(
    state: State<'_, EngineState>,
    tab_id: String,
    new_order: i32,
) -> Result<(), String> {
    state
        .store
        .reorder_tab(&tab_id, new_order)
        .map_err(|e| e.to_string())
}

/// List all tabs for a window (ordered).
#[tauri::command]
pub fn engine_list_tabs(
    state: State<'_, EngineState>,
    window_id: Option<String>,
) -> Result<Vec<DashboardTabRow>, String> {
    let wid = window_id.as_deref().unwrap_or("main");
    state.store.list_tabs(wid).map_err(|e| e.to_string())
}

/// List all tabs across all windows.
#[tauri::command]
pub fn engine_list_all_tabs(state: State<'_, EngineState>) -> Result<Vec<DashboardTabRow>, String> {
    state.store.list_all_tabs().map_err(|e| e.to_string())
}

// ── Window Geometry ─────────────────────────────────────────────────────

/// Save or update pop-out window geometry.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn engine_save_window_geometry(
    state: State<'_, EngineState>,
    dashboard_id: String,
    x: Option<i32>,
    y: Option<i32>,
    width: i32,
    height: i32,
    monitor: Option<i32>,
    popped_out: bool,
) -> Result<(), String> {
    state
        .store
        .upsert_window_geometry(&dashboard_id, x, y, width, height, monitor, popped_out)
        .map_err(|e| e.to_string())
}

/// Get stored window geometry for a dashboard.
#[tauri::command]
pub fn engine_get_window_geometry(
    state: State<'_, EngineState>,
    dashboard_id: String,
) -> Result<Option<DashboardWindowRow>, String> {
    state
        .store
        .get_window_geometry(&dashboard_id)
        .map_err(|e| e.to_string())
}

/// List all dashboards that were popped out (for startup restore).
#[tauri::command]
pub fn engine_list_popped_out_windows(
    state: State<'_, EngineState>,
) -> Result<Vec<DashboardWindowRow>, String> {
    state
        .store
        .list_popped_out_windows()
        .map_err(|e| e.to_string())
}

/// Mark a window as no longer popped out.
#[tauri::command]
pub fn engine_mark_window_closed(
    state: State<'_, EngineState>,
    dashboard_id: String,
) -> Result<bool, String> {
    state
        .store
        .mark_window_closed(&dashboard_id)
        .map_err(|e| e.to_string())
}
