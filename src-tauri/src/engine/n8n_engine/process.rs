// n8n_engine/process.rs — Node.js child process lifecycle (no-Docker fallback)
//
// Manages starting and stopping n8n via `npx n8n` when Docker is unavailable.

use super::health::poll_n8n_ready;
use super::types::*;
use crate::atoms::error::{EngineError, EngineResult};

// ── Runtime check ──────────────────────────────────────────────────────

/// Check if Node.js is available on the system (for process mode fallback).
pub fn is_node_available() -> bool {
    std::process::Command::new("node")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// ── Process provisioning ───────────────────────────────────────────────

/// Start n8n as a managed child process via `npx n8n`.
pub async fn start_n8n_process(app_handle: &tauri::AppHandle) -> EngineResult<N8nEndpoint> {
    let port = find_available_port(DEFAULT_PORT);

    // Reuse encryption key and API key from previous config if they exist.
    // The n8n data directory persists — generating a new encryption key on
    // re-provision causes a mismatch with the key saved in the data dir.
    let prev_config = super::load_config(app_handle).ok();
    let api_key = prev_config
        .as_ref()
        .filter(|c| !c.api_key.is_empty())
        .map(|c| c.api_key.clone())
        .unwrap_or_else(generate_random_key);
    let encryption_key = prev_config
        .as_ref()
        .and_then(|c| c.encryption_key.clone())
        .filter(|k| !k.is_empty())
        .unwrap_or_else(generate_random_key);

    let data_dir = super::app_data_dir(app_handle).join("n8n-data");
    std::fs::create_dir_all(&data_dir)
        .map_err(|e| EngineError::Other(format!("Failed to create n8n data dir: {}", e)))?;

    super::emit_status(
        app_handle,
        "starting",
        "Starting integration engine (Node.js)...",
    );

    // Redirect n8n output to a log file so we can diagnose startup failures.
    let log_path = data_dir.join("n8n-process.log");
    let log_file = std::fs::File::create(&log_path).ok();
    let stdout_sink = log_file
        .as_ref()
        .and_then(|f| f.try_clone().ok())
        .map(std::process::Stdio::from)
        .unwrap_or_else(std::process::Stdio::null);
    let stderr_sink = log_file
        .and_then(|f| f.try_clone().ok())
        .map(std::process::Stdio::from)
        .unwrap_or_else(std::process::Stdio::null);

    let child = std::process::Command::new("npx")
        .arg("--yes")
        .arg("n8n@latest")
        .env("N8N_PORT", port.to_string())
        .env("N8N_BASIC_AUTH_ACTIVE", "false")
        .env("N8N_SECURE_COOKIE", "false")
        .env("N8N_ENCRYPTION_KEY", &encryption_key)
        .env("N8N_API_KEY", &api_key)
        .env("N8N_USER_FOLDER", data_dir.to_string_lossy().as_ref())
        .env("N8N_DIAGNOSTICS_ENABLED", "false")
        .env("N8N_PERSONALIZATION_ENABLED", "false")
        // Enable community node installation (required for 25K+ packages)
        .env("N8N_COMMUNITY_PACKAGES_ENABLED", "true")
        // Allow installation of packages not in n8n's verified registry
        .env("N8N_COMMUNITY_PACKAGES_ALLOW_UNVERIFIED", "true")
        .env("N8N_REINSTALL_MISSING_PACKAGES", "true")
        .env("N8N_COMMUNITY_PACKAGES_ALLOW_TOOL_USAGE", "true")
        .stdout(stdout_sink)
        .stderr(stderr_sink)
        .spawn()
        .map_err(|e| EngineError::Other(format!("Failed to start npx n8n: {}", e)))?;

    let pid = child.id();
    let url = format!("http://127.0.0.1:{}", port);

    // Poll for readiness
    let ready = poll_n8n_ready(&url, &api_key).await;
    if !ready {
        // Try to kill the process since it didn't become ready
        #[cfg(unix)]
        {
            let _ = std::process::Command::new("kill")
                .arg(pid.to_string())
                .status();
        }

        // Read tail of n8n log for diagnostics
        let tail = std::fs::read_to_string(&log_path)
            .ok()
            .map(|s| {
                let lines: Vec<&str> = s.lines().collect();
                let start = lines.len().saturating_sub(30);
                lines[start..].join("\n")
            })
            .unwrap_or_default();
        if !tail.is_empty() {
            log::error!("[n8n] Process log tail:\n{}", tail);
        } else {
            log::error!("[n8n] No process log output captured");
        }

        super::emit_status(
            app_handle,
            "error",
            "Integration engine failed to start. Check that Node.js 18+ is installed.",
        );
        return Err(EngineError::Other(format!(
            "npx n8n started but failed to become healthy within {}s",
            super::types::STARTUP_TIMEOUT_SECS
        )));
    }

    // Set up the owner account for headless operation.
    if let Err(e) = super::health::setup_owner_if_needed(&url).await {
        log::warn!("[n8n] Owner setup failed (non-fatal): {}", e);
    }

    // Log the n8n version for diagnostics (MCP requires recent versions)
    if let Some(version) = super::health::get_n8n_version(&url, &api_key).await {
        log::info!("[n8n] Process mode running n8n v{}", version);
    }

    // Enable MCP access (disabled by default even after owner creation)
    if let Err(e) = super::health::enable_mcp_access(&url).await {
        log::warn!("[n8n] MCP access enable failed (non-fatal): {}", e);
    }

    // Persist config
    let new_config = N8nEngineConfig {
        mode: N8nMode::Process,
        url: url.clone(),
        api_key: api_key.clone(),
        container_id: None,
        container_port: None,
        encryption_key: Some(encryption_key),
        process_pid: Some(pid),
        process_port: Some(port),
        mcp_token: None,
        enabled: true,
        auto_discover: true,
        mcp_mode: true,
    };
    super::save_config(app_handle, &new_config)?;

    super::emit_status(app_handle, "ready", "Integration engine ready.");

    Ok(N8nEndpoint {
        url,
        api_key,
        mode: N8nMode::Process,
    })
}

// ── Process stop ───────────────────────────────────────────────────────

/// Kill a managed child process by PID.
pub fn stop_process(pid: u32) {
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("kill")
            .arg(pid.to_string())
            .status();
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .status();
    }
}
