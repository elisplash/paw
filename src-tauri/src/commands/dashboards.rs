// Dashboard & Template Commands — Tauri IPC wrappers.
// Thin layer: deserialise, delegate to SessionStore, serialise.

use crate::atoms::types::{DashboardRow, DashboardTemplateRow};
use crate::engine::state::EngineState;
use tauri::State;

// ── Dashboard CRUD ──────────────────────────────────────────────────────

/// Create a new saved dashboard (from the UI).
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn engine_create_dashboard(
    state: State<'_, EngineState>,
    dashboard_id: String,
    name: String,
    icon: Option<String>,
    agent_id: Option<String>,
    source_session_id: Option<String>,
    pinned: Option<bool>,
) -> Result<(), String> {
    state
        .store
        .create_dashboard(
            &dashboard_id,
            &name,
            icon.as_deref().unwrap_or("dashboard"),
            agent_id.as_deref().unwrap_or(""),
            source_session_id.as_deref(),
            None,
            pinned.unwrap_or(false),
            None,
            None,
        )
        .map_err(|e| e.to_string())
}

/// Update a dashboard's metadata (name, icon, pinned, refresh settings).
#[tauri::command]
pub fn engine_update_dashboard(
    state: State<'_, EngineState>,
    dashboard_id: String,
    name: Option<String>,
    icon: Option<String>,
    pinned: Option<bool>,
) -> Result<bool, String> {
    state
        .store
        .update_dashboard(
            &dashboard_id,
            name.as_deref(),
            icon.as_deref(),
            pinned,
            None,
            None,
        )
        .map_err(|e| e.to_string())
}

/// Clone session canvas components into a dashboard scope.
#[tauri::command]
pub fn engine_clone_canvas_to_dashboard(
    state: State<'_, EngineState>,
    source_session_id: String,
    dashboard_id: String,
) -> Result<u64, String> {
    state
        .store
        .clone_components_to_dashboard(&source_session_id, &dashboard_id)
        .map_err(|e| e.to_string())
}

/// List all saved dashboards (pinned first, then by updated_at).
#[tauri::command]
pub fn engine_list_dashboards(state: State<'_, EngineState>) -> Result<Vec<DashboardRow>, String> {
    state.store.list_dashboards().map_err(|e| e.to_string())
}

/// List only pinned dashboards (for sidebar rendering).
#[tauri::command]
pub fn engine_list_pinned_dashboards(
    state: State<'_, EngineState>,
) -> Result<Vec<DashboardRow>, String> {
    state
        .store
        .list_pinned_dashboards()
        .map_err(|e| e.to_string())
}

/// Get a single dashboard by ID.
#[tauri::command]
pub fn engine_get_dashboard(
    state: State<'_, EngineState>,
    dashboard_id: String,
) -> Result<Option<DashboardRow>, String> {
    state
        .store
        .get_dashboard(&dashboard_id)
        .map_err(|e| e.to_string())
}

/// Delete a dashboard and its components.
#[tauri::command]
pub fn engine_delete_dashboard(
    state: State<'_, EngineState>,
    dashboard_id: String,
) -> Result<bool, String> {
    state
        .store
        .delete_dashboard(&dashboard_id)
        .map_err(|e| e.to_string())
}

// ── Template CRUD ───────────────────────────────────────────────────────

/// List all templates, optionally filtered by source.
#[tauri::command]
pub fn engine_list_templates(
    state: State<'_, EngineState>,
    source: Option<String>,
) -> Result<Vec<DashboardTemplateRow>, String> {
    state
        .store
        .list_templates(source.as_deref())
        .map_err(|e| e.to_string())
}

/// Get a single template by ID.
#[tauri::command]
pub fn engine_get_template(
    state: State<'_, EngineState>,
    template_id: String,
) -> Result<Option<DashboardTemplateRow>, String> {
    state
        .store
        .get_template(&template_id)
        .map_err(|e| e.to_string())
}

/// Delete a template by ID.
#[tauri::command]
pub fn engine_delete_template(
    state: State<'_, EngineState>,
    template_id: String,
) -> Result<bool, String> {
    state
        .store
        .delete_template(&template_id)
        .map_err(|e| e.to_string())
}

/// Seed built-in templates if not already present.
#[tauri::command]
pub fn engine_seed_templates(state: State<'_, EngineState>) -> Result<u64, String> {
    state
        .store
        .seed_builtin_templates()
        .map_err(|e| e.to_string())
}
