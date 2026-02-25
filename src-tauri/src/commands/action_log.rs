// commands/action_log.rs — Tauri IPC commands for integration action logging
//
// Phase 4: Persistent action log with human-readable summaries.

use crate::engine::channels;
use serde::{Deserialize, Serialize};

// ── Types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationActionLog {
    pub id: String,
    pub timestamp: String,
    pub service: String,
    #[serde(rename = "serviceName")]
    pub service_name: String,
    pub action: String,
    #[serde(rename = "actionLabel")]
    pub action_label: String,
    pub summary: String,
    pub agent: String,
    pub status: String, // success | failed | running
    #[serde(rename = "durationMs")]
    pub duration_ms: u64,
    #[serde(default)]
    pub input: Option<serde_json::Value>,
    #[serde(default)]
    pub output: Option<serde_json::Value>,
    #[serde(rename = "errorMessage", default)]
    pub error_message: Option<String>,
    #[serde(rename = "externalUrl", default)]
    pub external_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionStatsResult {
    pub total: u64,
    pub success: u64,
    pub failed: u64,
    pub running: u64,
    #[serde(rename = "byService")]
    pub by_service: std::collections::HashMap<String, ServiceActionStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceActionStats {
    pub count: u64,
    pub failed: u64,
    pub label: String,
}

// ── Storage ────────────────────────────────────────────────────────────

const STORAGE_KEY: &str = "integration_action_log";

fn load_log(app: &tauri::AppHandle) -> Vec<IntegrationActionLog> {
    channels::load_channel_config::<Vec<IntegrationActionLog>>(app, STORAGE_KEY)
        .unwrap_or_default()
}

fn save_log(
    app: &tauri::AppHandle,
    log: &[IntegrationActionLog],
) -> Result<(), String> {
    channels::save_channel_config(app, STORAGE_KEY, &log.to_vec())
        .map_err(|e| e.to_string())
}

// ── Commands ───────────────────────────────────────────────────────────

/// Record an integration action.
#[tauri::command]
pub fn engine_action_log_record(
    app_handle: tauri::AppHandle,
    service: String,
    service_name: String,
    action: String,
    action_label: String,
    summary: String,
    agent: String,
    status: String,
    duration_ms: u64,
    input: Option<serde_json::Value>,
    output: Option<serde_json::Value>,
    error_message: Option<String>,
    external_url: Option<String>,
) -> Result<IntegrationActionLog, String> {
    let mut log = load_log(&app_handle);

    let now = chrono::Utc::now().to_rfc3339();
    let entry = IntegrationActionLog {
        id: format!(
            "act_{}_{}_{}",
            service,
            action.replace(' ', "_"),
            now.replace([':', '-', '.'], "")
        ),
        timestamp: now,
        service,
        service_name,
        action,
        action_label,
        summary,
        agent,
        status,
        duration_ms,
        input,
        output,
        error_message,
        external_url,
    };

    log.push(entry.clone());

    // Keep only last 1000 entries
    if log.len() > 1000 {
        log = log.split_off(log.len() - 1000);
    }

    save_log(&app_handle, &log)?;
    Ok(entry)
}

/// List actions, optionally filtered by service and limited.
#[tauri::command]
pub fn engine_action_log_list(
    app_handle: tauri::AppHandle,
    limit: Option<u32>,
    service: Option<String>,
) -> Result<Vec<IntegrationActionLog>, String> {
    let log = load_log(&app_handle);
    let limit = limit.unwrap_or(50) as usize;

    let filtered: Vec<IntegrationActionLog> = if let Some(ref svc) = service {
        log.into_iter()
            .filter(|a| &a.service == svc)
            .collect()
    } else {
        log
    };

    // Return most recent first
    let mut result: Vec<IntegrationActionLog> = filtered
        .into_iter()
        .rev()
        .take(limit)
        .collect();
    result.reverse();

    Ok(result)
}

/// Compute aggregate stats for recent actions (today by default).
#[tauri::command]
pub fn engine_action_log_stats(
    app_handle: tauri::AppHandle,
) -> Result<ActionStatsResult, String> {
    let log = load_log(&app_handle);
    let today = chrono::Utc::now()
        .format("%Y-%m-%d")
        .to_string();

    let today_actions: Vec<&IntegrationActionLog> = log
        .iter()
        .filter(|a| a.timestamp.starts_with(&today))
        .collect();

    let mut by_service = std::collections::HashMap::new();
    let mut success: u64 = 0;
    let mut failed: u64 = 0;
    let mut running: u64 = 0;

    for a in &today_actions {
        match a.status.as_str() {
            "success" => success += 1,
            "failed" => failed += 1,
            _ => running += 1,
        }

        let entry = by_service
            .entry(a.service.clone())
            .or_insert_with(|| ServiceActionStats {
                count: 0,
                failed: 0,
                label: a.service_name.clone(),
            });
        entry.count += 1;
        if a.status == "failed" {
            entry.failed += 1;
        }
    }

    Ok(ActionStatsResult {
        total: today_actions.len() as u64,
        success,
        failed,
        running,
        by_service,
    })
}

/// Clear the action log.
#[tauri::command]
pub fn engine_action_log_clear(
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    save_log(&app_handle, &[])
}
