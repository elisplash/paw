// src-tauri/src/commands/integrations.rs — Integration Management Commands
//
// Consolidated Tauri commands for connecting, disconnecting, listing,
// and managing integration services. Bridges the frontend Integrations
// view to the channel config persistence layer.

use crate::engine::channels;
use crate::engine::state::EngineState;
use log::{info, warn};
use serde::{Deserialize, Serialize};
use tauri::Manager;

// ── Types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectedService {
    #[serde(alias = "service_id")]
    pub service_id: String,
    #[serde(alias = "connected_at")]
    pub connected_at: String,
    #[serde(alias = "last_used")]
    pub last_used: Option<String>,
    #[serde(alias = "tool_count")]
    pub tool_count: u32,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationOverview {
    pub connected: Vec<ConnectedService>,
    #[serde(alias = "total_actions_today")]
    pub total_actions_today: u32,
    #[serde(alias = "services_needing_attention")]
    pub services_needing_attention: u32,
}

// ── Storage keys ───────────────────────────────────────────────────────

const CONNECTED_KEY: &str = "connected_service_ids";
const CONNECTED_DETAILS_KEY: &str = "connected_service_details";

// ── Helpers ────────────────────────────────────────────────────────────

fn load_connected_ids(app: &tauri::AppHandle) -> Vec<String> {
    channels::load_channel_config::<Vec<String>>(app, CONNECTED_KEY).unwrap_or_default()
}

fn save_connected_ids(app: &tauri::AppHandle, ids: &[String]) -> Result<(), String> {
    channels::save_channel_config(app, CONNECTED_KEY, &ids.to_vec()).map_err(|e| e.to_string())
}

fn load_details(app: &tauri::AppHandle) -> Vec<ConnectedService> {
    channels::load_channel_config::<Vec<ConnectedService>>(app, CONNECTED_DETAILS_KEY)
        .unwrap_or_default()
}

fn save_details(app: &tauri::AppHandle, details: &[ConnectedService]) -> Result<(), String> {
    channels::save_channel_config(app, CONNECTED_DETAILS_KEY, &details.to_vec())
        .map_err(|e| e.to_string())
}

// ── Commands ───────────────────────────────────────────────────────────

/// List all connected service IDs.
#[tauri::command]
pub fn engine_integrations_list_connected(
    app_handle: tauri::AppHandle,
) -> Result<Vec<String>, String> {
    Ok(load_connected_ids(&app_handle))
}

/// Get detailed info for all connected services.
#[tauri::command]
pub fn engine_integrations_get_connected(
    app_handle: tauri::AppHandle,
) -> Result<Vec<ConnectedService>, String> {
    Ok(load_details(&app_handle))
}

/// Connect a service — adds to connected list and creates a detail record.
#[tauri::command]
pub fn engine_integrations_connect(
    app_handle: tauri::AppHandle,
    service_id: String,
    tool_count: u32,
) -> Result<ConnectedService, String> {
    let mut ids = load_connected_ids(&app_handle);
    if !ids.contains(&service_id) {
        ids.push(service_id.clone());
        save_connected_ids(&app_handle, &ids)?;
    }

    let now = chrono::Utc::now().to_rfc3339();
    let mut details = load_details(&app_handle);
    // Remove old entry if exists
    details.retain(|d| d.service_id != service_id);

    let svc = ConnectedService {
        service_id: service_id.clone(),
        connected_at: now.clone(),
        last_used: Some(now),
        tool_count,
        status: "connected".into(),
    };
    details.push(svc.clone());
    save_details(&app_handle, &details)?;

    // Also update health monitor with initial status
    let _ = crate::commands::health_monitor::engine_health_update_service(
        app_handle,
        service_id,
        "healthy".into(),
        Some("Connected".into()),
        None,
    );

    Ok(svc)
}

/// Disconnect a service — removes from connected list and updates status.
#[tauri::command]
pub fn engine_integrations_disconnect(
    app_handle: tauri::AppHandle,
    service_id: String,
) -> Result<(), String> {
    let mut ids = load_connected_ids(&app_handle);
    ids.retain(|id| id != &service_id);
    save_connected_ids(&app_handle, &ids)?;

    let mut details = load_details(&app_handle);
    if let Some(d) = details.iter_mut().find(|d| d.service_id == service_id) {
        d.status = "disconnected".into();
    }
    save_details(&app_handle, &details)?;

    // Purge the skill vault for this service so stale/corrupted credentials
    // don't persist and break future reconnections.
    let skill_id = crate::commands::n8n::service_to_skill_id(&service_id);
    if let Some(state) = app_handle.try_state::<EngineState>() {
        if let Err(e) = state.store.delete_all_skill_credentials(&skill_id) {
            warn!(
                "[disconnect] Failed to purge vault for skill '{}': {}",
                skill_id, e
            );
        }
        // Also disable the skill so the agent doesn't try to use it
        if let Err(e) = state.store.set_skill_enabled(&skill_id, false) {
            warn!("[disconnect] Failed to disable skill '{}': {}", skill_id, e);
        }
    }

    // Update health monitor
    let _ = crate::commands::health_monitor::engine_health_update_service(
        app_handle,
        service_id,
        "unknown".into(),
        Some("Disconnected".into()),
        None,
    );

    Ok(())
}

/// Update the last-used timestamp for a service.
#[tauri::command]
pub fn engine_integrations_touch(
    app_handle: tauri::AppHandle,
    service_id: String,
) -> Result<(), String> {
    let mut details = load_details(&app_handle);
    let now = chrono::Utc::now().to_rfc3339();
    if let Some(d) = details.iter_mut().find(|d| d.service_id == service_id) {
        d.last_used = Some(now);
    }
    save_details(&app_handle, &details)?;
    Ok(())
}

/// Get a high-level overview: connected count, today's actions, attention needed.
#[tauri::command]
pub fn engine_integrations_overview(
    app_handle: tauri::AppHandle,
) -> Result<IntegrationOverview, String> {
    let connected = load_details(&app_handle);
    let needing_attention = connected
        .iter()
        .filter(|c| c.status == "error" || c.status == "expired")
        .count() as u32;

    // Get action stats from action_log
    let total_today = match crate::commands::action_log::engine_action_log_stats(app_handle) {
        Ok(stats) => stats.total as u32,
        Err(_) => 0,
    };

    Ok(IntegrationOverview {
        connected,
        total_actions_today: total_today,
        services_needing_attention: needing_attention,
    })
}

// ── Calendar Events (Today widget) ─────────────────────────────────────

/// A single calendar event returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEvent {
    pub id: String,
    pub summary: String,
    pub start: String,
    pub end: String,
    pub location: Option<String>,
    pub all_day: bool,
}

/// Fetch today's Google Calendar events using the stored OAuth access token.
/// Returns an empty list if Google Calendar is not connected or the token is
/// expired (does NOT error — the frontend just shows the fallback).
#[tauri::command]
pub async fn engine_calendar_events_today(
    app_handle: tauri::AppHandle,
) -> Result<Vec<CalendarEvent>, String> {
    use crate::engine::key_vault;
    use crate::engine::skills::crypto::{decrypt_credential, get_vault_key};

    // ── 1. Check if google-calendar is in the connected list ─────────
    let connected = load_connected_ids(&app_handle);
    let is_google_cal = connected
        .iter()
        .any(|id| id == "google-calendar" || id == "google-workspace");
    if !is_google_cal {
        return Ok(vec![]);
    }

    // ── 2. Load the OAuth access token ───────────────────────────────
    let vault_key = get_vault_key().map_err(|e| format!("Vault key error: {e}"))?;
    let encrypted =
        match key_vault::get("oauth:google-workspace").or_else(|| key_vault::get("oauth:google")) {
            Some(v) => v,
            None => {
                info!("[calendar] No Google OAuth tokens found — skipping");
                return Ok(vec![]);
            }
        };
    let json =
        decrypt_credential(&encrypted, &vault_key).map_err(|e| format!("Decrypt error: {e}"))?;

    #[derive(Deserialize)]
    struct Tokens {
        access_token: String,
    }
    let tokens: Tokens =
        serde_json::from_str(&json).map_err(|e| format!("Deserialize error: {e}"))?;

    // ── 3. Build time range: today (local midnight → midnight) ───────
    let now = chrono::Utc::now();
    let today_start = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .to_rfc3339();
    let today_end = now
        .date_naive()
        .and_hms_opt(23, 59, 59)
        .unwrap()
        .and_utc()
        .to_rfc3339();

    // ── 4. Call Google Calendar API ───────────────────────────────────
    let url = format!(
        "https://www.googleapis.com/calendar/v3/calendars/primary/events\
         ?timeMin={}&timeMax={}&singleEvents=true&orderBy=startTime&maxResults=20",
        urlencoding::encode(&today_start),
        urlencoding::encode(&today_end),
    );

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .bearer_auth(&tokens.access_token)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("Calendar API request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        warn!(
            "[calendar] Google Calendar API returned {}: {}",
            status,
            &body[..body.len().min(200)]
        );
        // Return empty instead of erroring — the token might be expired
        // and the background refresh will fix it on the next cycle.
        return Ok(vec![]);
    }

    // ── 5. Parse the response ────────────────────────────────────────
    #[derive(Deserialize)]
    struct GCalResponse {
        items: Option<Vec<GCalEvent>>,
    }
    #[derive(Deserialize)]
    struct GCalEvent {
        id: Option<String>,
        summary: Option<String>,
        start: Option<GCalTime>,
        end: Option<GCalTime>,
        location: Option<String>,
    }
    #[derive(Deserialize)]
    struct GCalTime {
        #[serde(rename = "dateTime")]
        date_time: Option<String>,
        date: Option<String>,
    }

    let gcal: GCalResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse Calendar response: {e}"))?;

    let events: Vec<CalendarEvent> = gcal
        .items
        .unwrap_or_default()
        .into_iter()
        .map(|ev| {
            let all_day = ev
                .start
                .as_ref()
                .map(|s| s.date.is_some() && s.date_time.is_none())
                .unwrap_or(false);
            let start = ev
                .start
                .as_ref()
                .and_then(|s| s.date_time.clone().or(s.date.clone()))
                .unwrap_or_default();
            let end = ev
                .end
                .as_ref()
                .and_then(|s| s.date_time.clone().or(s.date.clone()))
                .unwrap_or_default();
            CalendarEvent {
                id: ev.id.unwrap_or_default(),
                summary: ev.summary.unwrap_or_else(|| "(No title)".to_string()),
                start,
                end,
                location: ev.location,
                all_day,
            }
        })
        .collect();

    info!("[calendar] Fetched {} event(s) for today", events.len());
    Ok(events)
}

// ── Integration Pipeline Preflight Check ───────────────────────────────
//
// Validates the integration pipeline is correctly wired before runtime.
// Called by the frontend's Integration Hub to surface configuration
// issues proactively rather than failing silently at OAuth time.

/// Result of a preflight check for the integration pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationPreflightResult {
    /// Overall readiness: true = all checks passed.
    pub ready: bool,
    /// Individual check results.
    pub checks: Vec<PreflightCheck>,
}

/// A single preflight check item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightCheck {
    pub name: String,
    pub passed: bool,
    pub message: String,
}

/// Run preflight checks on the integration pipeline.
///
/// Checks:
///   1. OAuth engine has at least one service configured
///   2. Google scopes cover all five services (Gmail, Calendar, Drive, Sheets, Docs)
///   3. Google tool definitions match dispatch table (no orphan/missing tools)
///   4. Skill→tool wiring is consistent
///   5. n8n engine is available (advisory, not blocking)
#[tauri::command]
pub async fn engine_integration_preflight(
    app_handle: tauri::AppHandle,
) -> Result<IntegrationPreflightResult, String> {
    use crate::engine::oauth;
    use crate::engine::tools::google;

    let mut checks = Vec::new();

    // ── Check 1: OAuth services registered ─────────────────────────
    let oauth_ids = oauth::oauth_service_ids();
    let has_services = !oauth_ids.is_empty();
    checks.push(PreflightCheck {
        name: "oauth_services".into(),
        passed: has_services,
        message: if has_services {
            format!("{} OAuth service(s) registered", oauth_ids.len())
        } else {
            "No OAuth services registered — integration hub will be empty".into()
        },
    });

    // ── Check 2: Google scopes cover all G Suite services ──────────
    let google_config = oauth::get_oauth_config("google");
    let scopes_ok = if let Some(config) = google_config {
        let scopes = config.default_scopes.join(" ");
        let has_gmail = scopes.contains("gmail");
        let has_calendar = scopes.contains("calendar");
        let has_drive = scopes.contains("drive");
        let has_sheets = scopes.contains("spreadsheets");
        let has_docs = scopes.contains("documents");
        has_gmail && has_calendar && has_drive && has_sheets && has_docs
    } else {
        false
    };
    checks.push(PreflightCheck {
        name: "google_scopes".into(),
        passed: scopes_ok,
        message: if scopes_ok {
            "Google OAuth scopes cover Gmail, Calendar, Drive, Sheets, Docs".into()
        } else {
            "Google OAuth scopes are incomplete — some services won't work".into()
        },
    });

    // ── Check 3: Google tool definitions match dispatch ─────────────
    let google_defs = google::definitions();
    let expected_tools = [
        "google_gmail_list",
        "google_gmail_read",
        "google_gmail_send",
        "google_calendar_list",
        "google_calendar_create",
        "google_drive_list",
        "google_drive_read",
        "google_drive_upload",
        "google_drive_share",
        "google_sheets_read",
        "google_sheets_append",
        "google_docs_create",
        "google_api",
    ];
    let def_names: Vec<&str> = google_defs
        .iter()
        .map(|d| d.function.name.as_str())
        .collect();
    let mut missing_tools: Vec<&str> = Vec::new();
    for tool in &expected_tools {
        if !def_names.contains(tool) {
            missing_tools.push(tool);
        }
    }
    let tools_ok = missing_tools.is_empty() && google_defs.len() == expected_tools.len();
    checks.push(PreflightCheck {
        name: "google_tools".into(),
        passed: tools_ok,
        message: if tools_ok {
            format!(
                "All {} Google tools registered and dispatched",
                expected_tools.len()
            )
        } else if !missing_tools.is_empty() {
            format!("Missing Google tools: {}", missing_tools.join(", "))
        } else {
            format!(
                "Tool count mismatch: {} definitions vs {} expected",
                google_defs.len(),
                expected_tools.len()
            )
        },
    });

    // ── Check 4: Google aliases resolve consistently ───────────────
    let aliases = [
        "google",
        "google-workspace",
        "gmail",
        "google-drive",
        "google-calendar",
        "google-sheets",
        "google-docs",
    ];
    let alias_ok = aliases.iter().all(|a| {
        oauth::get_oauth_config(a).is_some()
            && oauth::get_n8n_oauth_type(a) == Some("googleOAuth2Api")
    });
    checks.push(PreflightCheck {
        name: "google_aliases".into(),
        passed: alias_ok,
        message: if alias_ok {
            "All Google service aliases route correctly".into()
        } else {
            "Some Google aliases have broken routing — check oauth.rs match arms".into()
        },
    });

    // ── Check 5: n8n engine availability (advisory) ────────────────
    let n8n_ready = match crate::engine::n8n_engine::load_config(&app_handle) {
        Ok(config) => config.enabled,
        Err(_) => false,
    };
    checks.push(PreflightCheck {
        name: "n8n_engine".into(),
        passed: n8n_ready,
        message: if n8n_ready {
            "n8n integration engine enabled".into()
        } else {
            "n8n engine not enabled — Tier 2 OAuth services and MCP bridge unavailable (non-blocking)".into()
        },
    });

    let all_passed = checks.iter().all(|c| c.passed);
    Ok(IntegrationPreflightResult {
        ready: all_passed,
        checks,
    })
}
