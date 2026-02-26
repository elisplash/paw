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
pub async fn start_n8n_process(
    app_handle: &tauri::AppHandle,
) -> EngineResult<N8nEndpoint> {
    let port = find_available_port(DEFAULT_PORT);
    let api_key = generate_random_key();
    let encryption_key = generate_random_key();

    let data_dir = super::app_data_dir(app_handle).join("n8n-data");
    std::fs::create_dir_all(&data_dir)
        .map_err(|e| EngineError::Other(format!("Failed to create n8n data dir: {}", e)))?;

    super::emit_status(app_handle, "starting", "Starting integration engine (Node.js)...");

    let child = std::process::Command::new("npx")
        .arg("n8n")
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
        .env("N8N_COMMUNITY_PACKAGES_ALLOW_TOOL_USAGE", "true")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
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
        super::emit_status(
            app_handle,
            "error",
            "Integration engine failed to start. Check that Node.js 18+ is installed.",
        );
        return Err(EngineError::Other(
            "npx n8n started but failed to become healthy within 60s".into(),
        ));
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
