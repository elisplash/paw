// Dashboard & Template Commands — Tauri IPC wrappers.
// Thin layer: deserialise, delegate to SessionStore, serialise.

use crate::atoms::types::{DashboardRow, DashboardTemplateRow};
use crate::engine::state::EngineState;
use tauri::State;

// ── Dashboard CRUD ──────────────────────────────────────────────────────

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
