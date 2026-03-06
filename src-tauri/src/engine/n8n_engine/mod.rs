// n8n_engine/mod.rs — Orchestrator, config persistence, status, and re-exports
//
// This is the thin barrel module for the n8n engine. It re-exports the
// public API surface and contains the top-level orchestration logic that
// coordinates across docker, process, and health sub-modules.

mod docker;
pub mod health;
pub mod node_provision;
mod process;
pub mod types;

// ── Re-exports (public API surface used by commands/n8n.rs) ────────────

pub use types::{N8nEndpoint, N8nEngineConfig, N8nEngineStatus, N8nMode};

use crate::atoms::error::{EngineError, EngineResult};
use crate::engine::channels;
use types::{CONFIG_KEY, DEFAULT_PORT};

// ── Config persistence ─────────────────────────────────────────────────

pub fn load_config(app_handle: &tauri::AppHandle) -> EngineResult<N8nEngineConfig> {
    channels::load_channel_config::<N8nEngineConfig>(app_handle, CONFIG_KEY)
}

pub fn save_config(app_handle: &tauri::AppHandle, config: &N8nEngineConfig) -> EngineResult<()> {
    channels::save_channel_config(app_handle, CONFIG_KEY, config)
}

// ── Status events ──────────────────────────────────────────────────────

/// Emit an n8n status event to the frontend.
fn emit_status(app_handle: &tauri::AppHandle, kind: &str, message: &str) {
    use tauri::Emitter;
    let _ = app_handle.emit(
        "n8n-status",
        serde_json::json!({
            "kind": kind,
            "message": message,
        }),
    );
}

// ── Utility ────────────────────────────────────────────────────────────

/// Get the application data directory.
fn app_data_dir(app_handle: &tauri::AppHandle) -> std::path::PathBuf {
    use tauri::Manager;
    app_handle
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

/// Get the n8n data directory at a space-free path.
///
/// node-gyp / native npm packages break when the install path contains
/// spaces (macOS `~/Library/Application Support/...`).  We use
/// `~/.openpawz/n8n-data` instead — no spaces, no node-gyp failures.
///
/// On first call, if old data exists at `<app_data_dir>/n8n-data`, it is
/// moved to the new location automatically.
pub fn n8n_data_dir(app_handle: &tauri::AppHandle) -> std::path::PathBuf {
    let new_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".openpawz")
        .join("n8n-data");

    // Migrate from old location if it exists and new one doesn't
    let old_dir = app_data_dir(app_handle).join("n8n-data");
    if old_dir.exists() && !new_dir.exists() {
        log::info!(
            "[n8n] Migrating data dir from {} → {}",
            old_dir.display(),
            new_dir.display()
        );
        if let Some(parent) = new_dir.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::rename(&old_dir, &new_dir) {
            log::warn!("[n8n] Migration rename failed ({}), trying copy…", e);
            // rename fails across mount points — fall through to use old dir
            if !new_dir.exists() {
                return old_dir;
            }
        }
    }

    new_dir
}

// ── Main orchestrator ──────────────────────────────────────────────────

/// Ensure the n8n engine is running and return its endpoint.
///
/// This is the single entry point other modules call. It handles all modes:
///   1. If already configured (remote/local) → verify + return
///   2. Detect local n8n on localhost:5678
///   3. Docker available → provision container
///   4. Node.js available → start via npx
///   5. Nothing available → error with actionable message
pub async fn ensure_n8n_ready(app_handle: &tauri::AppHandle) -> EngineResult<N8nEndpoint> {
    let config = load_config(app_handle)?;

    // ── Already configured? ────────────────────────────────────────
    match config.mode {
        N8nMode::Remote if !config.url.is_empty() && !config.api_key.is_empty() => {
            // Verify remote endpoint is reachable
            if health::probe_n8n(&config.url, &config.api_key).await {
                return Ok(N8nEndpoint {
                    url: config.url,
                    api_key: config.api_key,
                    mode: N8nMode::Remote,
                });
            }
            // Remote configured but unreachable — fall through to auto-provision
        }
        N8nMode::Embedded if config.container_id.is_some() => {
            // Container previously provisioned — try to reconnect
            let port = config.container_port.unwrap_or(DEFAULT_PORT);
            let url = format!("http://127.0.0.1:{}", port);
            if health::probe_n8n(&url, &config.api_key).await {
                // Ensure owner + MCP are set up (idempotent, handles external restarts)
                let _ = health::setup_owner_if_needed(&url).await;
                let _ = health::enable_mcp_access(&url, &config.api_key).await;
                return Ok(N8nEndpoint {
                    url,
                    api_key: config.api_key,
                    mode: N8nMode::Embedded,
                });
            }
            // Container exists but not responding — try to start it
            if let Ok(endpoint) = docker::restart_existing_container(app_handle, &config).await {
                return Ok(endpoint);
            }
            // Container is broken — will re-provision below
        }
        N8nMode::Process if config.process_port.is_some() => {
            let port = config.process_port.unwrap_or(DEFAULT_PORT);
            let url = format!("http://127.0.0.1:{}", port);
            if health::probe_n8n(&url, &config.api_key).await {
                // Verify the running n8n version supports MCP (requires 1.x+).
                // An old cached npx version may still be running without MCP.
                if health::has_mcp_support(&url).await {
                    // Ensure owner + MCP are set up every time we reconnect.
                    // This is idempotent and handles the case where a previous
                    // MCP setup failed silently or n8n was restarted externally.
                    let _ = health::setup_owner_if_needed(&url).await;
                    let _ = health::enable_mcp_access(&url, &config.api_key).await;
                    return Ok(N8nEndpoint {
                        url,
                        api_key: config.api_key,
                        mode: N8nMode::Process,
                    });
                }
                // Old n8n without MCP — kill it and fall through to re-provision
                log::warn!(
                    "[n8n] Running n8n on port {} lacks MCP support — restarting with latest version",
                    port
                );
                kill_port(port);
                // Brief pause for the process to fully exit
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
            // Process died or outdated — will re-start below
        }
        _ => {}
    }

    // ── Detect local n8n on default port ───────────────────────────
    let local_url = format!("http://127.0.0.1:{}", DEFAULT_PORT);
    // Only auto-detect if we don't already own that port
    if config.container_port != Some(DEFAULT_PORT) && config.process_port != Some(DEFAULT_PORT) {
        if let Some(endpoint) = health::detect_local_n8n(&local_url).await {
            // Save detected config
            let mut new_config = config.clone();
            new_config.mode = N8nMode::Local;
            new_config.url = endpoint.url.clone();
            new_config.api_key = endpoint.api_key.clone();
            new_config.enabled = true;
            save_config(app_handle, &new_config)?;
            return Ok(endpoint);
        }
    }

    // ── Docker mode ────────────────────────────────────────────────
    if docker::is_docker_available().await {
        emit_status(
            app_handle,
            "provisioning",
            "Setting up integration engine...",
        );
        return docker::provision_docker_container(app_handle).await;
    }

    // ── Process mode (Node.js fallback) ────────────────────────────
    if process::is_node_available() {
        emit_status(
            app_handle,
            "provisioning",
            "Setting up integration engine...",
        );
        return process::start_n8n_process(app_handle).await;
    }

    // ── Auto-download Node.js ──────────────────────────────────────
    //
    // Neither Docker nor a system Node.js ≥ 18 was found.
    // Download a standalone Node.js binary so .exe / .dmg users get
    // a zero-setup experience.
    match node_provision::ensure_node_available(app_handle).await {
        Ok(_node_bin) => {
            log::info!("[n8n] Node.js auto-provisioned — starting n8n via process mode");
            return process::start_n8n_process(app_handle).await;
        }
        Err(e) => {
            log::warn!("[n8n] Auto-download of Node.js failed: {}", e);
        }
    }

    // ── Nothing available ──────────────────────────────────────────
    Err(EngineError::Other(
        "Integration engine requires Docker or Node.js. \
         Install Docker (docker.com/get-docker) or Node.js 18+ (nodejs.org) \
         to enable 400+ service integrations."
            .into(),
    ))
}

// ── Shutdown ───────────────────────────────────────────────────────────

/// Kill any process listening on the given port (used to evict stale n8n).
fn kill_port(port: u16) {
    #[cfg(unix)]
    {
        // lsof -ti :<port> -sTCP:LISTEN — only kill the LISTEN-ing process,
        // NOT client connections (which would include our own app).
        if let Ok(output) = std::process::Command::new("lsof")
            .args(["-ti", &format!(":{}", port), "-sTCP:LISTEN"])
            .output()
        {
            let pids = String::from_utf8_lossy(&output.stdout);
            for pid in pids.split_whitespace() {
                log::info!(
                    "[n8n] kill_port: killing LISTEN pid {} on port {}",
                    pid,
                    port
                );
                let _ = std::process::Command::new("kill")
                    .args(["-9", pid])
                    .status();
            }
        }
    }
    #[cfg(windows)]
    {
        // On Windows, use netstat + taskkill
        if let Ok(output) = std::process::Command::new("cmd")
            .args(["/C", &format!("for /f \"tokens=5\" %a in ('netstat -aon ^| findstr :{} ^| findstr LISTENING') do @taskkill /PID %a /F", port)])
            .output()
        {
            let _ = output; // best-effort
        }
    }
}

/// Restart the n8n process after a community package install.
///
/// Kills the existing n8n child process and re-provisions with
/// `start_n8n_process`, which re-reads the data dir (with its new
/// `node_modules`) and registers all community nodes on startup.
pub async fn restart_process(app_handle: &tauri::AppHandle) -> EngineResult<N8nEndpoint> {
    let config = load_config(app_handle)?;

    // Kill the old n8n process.  Prefer PID (targeted) over port-based kill
    // to avoid accidentally killing our own app which has client connections
    // on the same port.
    if let Some(pid) = config.process_pid {
        log::info!("[n8n] Killing n8n process (pid {}) for restart", pid);
        process::stop_process(pid);
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    } else if let Some(port) = config.process_port {
        log::info!(
            "[n8n] No PID saved — killing LISTEN-ing process on port {} for restart",
            port
        );
        kill_port(port);
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    // Re-provision — start_n8n_process will reuse the same data dir
    // (with the newly installed packages) and existing encryption/API keys.
    process::start_n8n_process(app_handle).await
}

/// Gracefully stop the n8n engine (called on app quit).
pub async fn shutdown(app_handle: &tauri::AppHandle) {
    let config = match load_config(app_handle) {
        Ok(c) => c,
        Err(_) => return,
    };

    match config.mode {
        N8nMode::Embedded => {
            if let Some(container_id) = &config.container_id {
                if let Ok(docker_conn) = docker::connect_docker().await {
                    let _ = docker_conn.stop_container(container_id, None).await;
                }
            }
        }
        N8nMode::Process => {
            if let Some(pid) = config.process_pid {
                process::stop_process(pid);
            }
        }
        _ => {} // Remote/Local — nothing to shut down
    }
}

// ── Health check ───────────────────────────────────────────────────────

/// Check engine health (called periodically if engine is active).
pub async fn health_check(app_handle: &tauri::AppHandle) -> bool {
    let config = match load_config(app_handle) {
        Ok(c) => c,
        Err(_) => return false,
    };
    if !config.enabled {
        return false;
    }
    let url = match config.mode {
        N8nMode::Remote | N8nMode::Local => config.url.clone(),
        N8nMode::Embedded => format!(
            "http://127.0.0.1:{}",
            config.container_port.unwrap_or(DEFAULT_PORT)
        ),
        N8nMode::Process => format!(
            "http://127.0.0.1:{}",
            config.process_port.unwrap_or(DEFAULT_PORT)
        ),
    };
    let healthy = health::probe_n8n(&url, &config.api_key).await;
    if healthy {
        emit_status(app_handle, "healthy", "Integration engine is running.");
    } else {
        emit_status(
            app_handle,
            "unhealthy",
            "Integration engine is not responding.",
        );
    }
    healthy
}

// ── Engine status ──────────────────────────────────────────────────────

/// Get the current status of the n8n engine.
pub async fn get_status(app_handle: &tauri::AppHandle) -> N8nEngineStatus {
    let config = load_config(app_handle).unwrap_or_default();
    let docker_available = docker::is_docker_available().await;
    let node_available = process::is_node_available();

    let url = match config.mode {
        N8nMode::Remote | N8nMode::Local => config.url.clone(),
        N8nMode::Embedded => format!(
            "http://127.0.0.1:{}",
            config.container_port.unwrap_or(DEFAULT_PORT)
        ),
        N8nMode::Process => format!(
            "http://127.0.0.1:{}",
            config.process_port.unwrap_or(DEFAULT_PORT)
        ),
    };

    let running = if config.enabled {
        health::probe_n8n(&url, &config.api_key).await
    } else {
        false
    };

    // Try to get version if running
    let version = if running {
        health::get_n8n_version(&url, &config.api_key)
            .await
            .unwrap_or_default()
    } else {
        String::new()
    };

    N8nEngineStatus {
        running,
        mode: config.mode,
        url,
        docker_available,
        node_available,
        container_id: config.container_id,
        process_pid: config.process_pid,
        version,
    }
}
