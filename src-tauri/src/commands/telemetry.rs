// Telemetry Commands — Tauri IPC wrappers (Canvas Phase 5).
// Exposes daily/weekly metrics and session metric history to the frontend.

use crate::atoms::types::TelemetryMetricRow;
use crate::engine::sessions::telemetry::{TelemetryDailySummary, TelemetryModelBreakdown};
use crate::engine::state::EngineState;
use tauri::State;

/// Get aggregated metrics for a single date (YYYY-MM-DD).
#[tauri::command]
pub fn engine_get_daily_metrics(
    state: State<'_, EngineState>,
    date: String,
) -> Result<TelemetryDailySummary, String> {
    state
        .store
        .get_daily_metrics(&date)
        .map_err(|e| e.to_string())
}

/// Get daily aggregated metrics for a date range (inclusive).
#[tauri::command]
pub fn engine_get_metrics_range(
    state: State<'_, EngineState>,
    start_date: String,
    end_date: String,
) -> Result<Vec<TelemetryDailySummary>, String> {
    state
        .store
        .get_metrics_range(&start_date, &end_date)
        .map_err(|e| e.to_string())
}

/// Get per-model cost breakdown for a date.
#[tauri::command]
pub fn engine_get_model_breakdown(
    state: State<'_, EngineState>,
    date: String,
) -> Result<Vec<TelemetryModelBreakdown>, String> {
    state
        .store
        .get_model_breakdown(&date)
        .map_err(|e| e.to_string())
}

/// List individual metric rows for a session (Inspector detail view).
#[tauri::command]
pub fn engine_list_session_metrics(
    state: State<'_, EngineState>,
    session_id: String,
) -> Result<Vec<TelemetryMetricRow>, String> {
    state
        .store
        .list_session_metrics(&session_id)
        .map_err(|e| e.to_string())
}

/// Delete metrics older than cutoff_date (YYYY-MM-DD). Returns count deleted.
#[tauri::command]
pub fn engine_purge_old_metrics(
    state: State<'_, EngineState>,
    cutoff_date: String,
) -> Result<u64, String> {
    state
        .store
        .purge_metrics_before(&cutoff_date)
        .map_err(|e| e.to_string())
}
