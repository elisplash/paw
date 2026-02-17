// Paw Agent Engine — Managed Browser Profiles
// Each agent gets an isolated Chrome profile with persistent cookies,
// localStorage, and session state across conversations.

use headless_chrome::{Browser, LaunchOptions};
use log::{info, warn};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

// ── Per-Agent Browser Pool ─────────────────────────────────────────────

static BROWSER_PROFILES: OnceLock<Mutex<HashMap<String, Arc<Browser>>>> = OnceLock::new();

/// Base directory for all browser profiles: ~/.paw/browser-profiles/
fn profiles_base_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".paw")
        .join("browser-profiles")
}

/// Get the profile directory for a specific agent.
pub fn profile_dir_for_agent(agent_id: &str) -> PathBuf {
    let safe_id = agent_id.replace(
        |c: char| !c.is_alphanumeric() && c != '-' && c != '_',
        "_",
    );
    profiles_base_dir().join(safe_id)
}

/// Get or launch a Chrome browser with a specific agent's profile.
/// The browser persists cookies, localStorage, and IndexedDB across calls.
pub fn get_or_launch_browser_for_agent(agent_id: &str) -> Result<Arc<Browser>, String> {
    let profiles = BROWSER_PROFILES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = profiles
        .lock()
        .map_err(|e| format!("Browser profiles lock error: {}", e))?;

    // Return existing browser if alive
    if let Some(browser) = guard.get(agent_id) {
        if browser.get_version().is_ok() {
            return Ok(Arc::clone(browser));
        }
        warn!(
            "[browser] Agent '{}' browser process dead, relaunching",
            agent_id
        );
    }

    // Create profile directory
    let profile_dir = profile_dir_for_agent(agent_id);
    std::fs::create_dir_all(&profile_dir)
        .map_err(|e| format!("Failed to create browser profile dir: {}", e))?;

    info!(
        "[browser] Launching Chrome for agent '{}' with profile: {}",
        agent_id,
        profile_dir.display()
    );

    let browser = Browser::new(
        LaunchOptions::default_builder()
            .headless(true)
            .sandbox(false) // Required in containers / CI
            .idle_browser_timeout(Duration::from_secs(300))
            .user_data_dir(Some(profile_dir))
            .build()
            .map_err(|e| format!("Browser launch options error: {}", e))?,
    )
    .map_err(|e| {
        format!(
            "Failed to launch Chrome for agent '{}': {}. Ensure Chrome/Chromium is installed.",
            agent_id, e
        )
    })?;

    let arc = Arc::new(browser);
    guard.insert(agent_id.to_string(), Arc::clone(&arc));
    info!("[browser] Chrome ready for agent '{}'", agent_id);
    Ok(arc)
}

// ── Profile Management ─────────────────────────────────────────────────

/// List all browser profiles on disk.
pub fn list_profiles() -> Vec<crate::engine::types::BrowserProfileConfig> {
    let base = profiles_base_dir();
    let mut profiles = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&base) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let agent_id = entry.file_name().to_string_lossy().to_string();
                let profile_dir = entry.path().to_string_lossy().to_string();
                let metadata = entry.metadata().ok();
                let created_at = metadata
                    .as_ref()
                    .and_then(|m| m.created().ok())
                    .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
                    .unwrap_or_default();
                let last_used_at = metadata
                    .as_ref()
                    .and_then(|m| m.modified().ok())
                    .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
                    .unwrap_or_default();

                profiles.push(crate::engine::types::BrowserProfileConfig {
                    agent_id,
                    profile_dir,
                    created_at,
                    last_used_at,
                });
            }
        }
    }

    profiles
}

/// Delete a browser profile — closes the browser instance and removes files.
pub fn delete_profile(agent_id: &str) -> Result<(), String> {
    // Remove from active browsers
    let profiles = BROWSER_PROFILES.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut guard) = profiles.lock() {
        guard.remove(agent_id);
    }

    // Remove profile directory
    let profile_dir = profile_dir_for_agent(agent_id);
    if profile_dir.exists() {
        std::fs::remove_dir_all(&profile_dir)
            .map_err(|e| format!("Failed to delete profile dir: {}", e))?;
    }

    info!("[browser] Deleted browser profile for agent '{}'", agent_id);
    Ok(())
}

/// Close all browser instances (cleanup on app exit).
pub fn close_all_browsers() {
    if let Some(profiles) = BROWSER_PROFILES.get() {
        if let Ok(mut guard) = profiles.lock() {
            let count = guard.len();
            guard.clear();
            if count > 0 {
                info!("[browser] Closed {} browser instances", count);
            }
        }
    }
}
