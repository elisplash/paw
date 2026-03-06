// n8n_engine/process.rs — Node.js child process lifecycle (no-Docker fallback)
//
// Manages starting and stopping n8n via `npx n8n` when Docker is unavailable.

use super::health::poll_n8n_ready;
use super::types::*;
use crate::atoms::error::{EngineError, EngineResult};
use crate::engine::key_vault;

// ── Runtime check ──────────────────────────────────────────────────────

/// Check if Node.js is available on the system **and** meets the
/// minimum version requirement (>= 18) for n8n.
pub fn is_node_available() -> bool {
    let output = std::process::Command::new("node").arg("--version").output();
    match output {
        Ok(o) if o.status.success() => {
            // `node --version` returns e.g. "v20.11.0\n"
            let ver = String::from_utf8_lossy(&o.stdout);
            parse_node_major(&ver) >= super::types::MIN_NODE_MAJOR
        }
        _ => false,
    }
}

/// Extract the major version number from a Node.js version string
/// like "v20.11.0" or "v18.19.1\n".
pub(crate) fn parse_node_major(version_str: &str) -> u32 {
    version_str
        .trim()
        .trim_start_matches('v')
        .split('.')
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0)
}

// ── Process provisioning ───────────────────────────────────────────────

/// Build the PATH environment variable with the local Node.js bin dir
/// prepended (if we auto-downloaded Node.js).  Falls back to the system
/// PATH if no local Node.js is present.
fn path_with_local_node() -> String {
    let system_path = std::env::var("PATH").unwrap_or_default();
    if let Some(bin_dir) = super::node_provision::local_node_bin_dir() {
        format!("{}:{}", bin_dir.to_string_lossy(), system_path)
    } else {
        system_path
    }
}

/// Resolve the `npx` command.  If we auto-downloaded Node.js, returns
/// the full path to the local `npx`.  Otherwise returns `"npx"` (system).
fn resolve_npx() -> std::path::PathBuf {
    if let Some(bin_dir) = super::node_provision::local_node_bin_dir() {
        let npx = if cfg!(target_os = "windows") {
            bin_dir.join("npx.cmd")
        } else {
            bin_dir.join("npx")
        };
        if npx.exists() {
            return npx;
        }
    }
    std::path::PathBuf::from("npx")
}

/// Start n8n as a managed child process via `npx n8n`.
pub async fn start_n8n_process(app_handle: &tauri::AppHandle) -> EngineResult<N8nEndpoint> {
    let port = find_available_port(DEFAULT_PORT);

    let data_dir = super::n8n_data_dir(app_handle);

    // Reuse encryption key and API key from previous config if they exist.
    // The n8n data directory persists — generating a new encryption key on
    // re-provision causes a mismatch with the key saved in the data dir.
    let prev_config = super::load_config(app_handle).ok();
    let api_key = prev_config
        .as_ref()
        .filter(|c| !c.api_key.is_empty())
        .map(|c| c.api_key.clone())
        .unwrap_or_else(generate_random_key);

    // Priority for encryption key:
    //   1. OS keychain vault (single source of truth)
    //   2. Generate a new random key (first-ever run)
    let encryption_key = get_n8n_encryption_key_from_keychain().unwrap_or_else(|| {
        log::info!("[n8n] No encryption key in vault — generating new one");
        let key = generate_random_key();
        store_n8n_encryption_key_in_keychain(&key);
        key
    });

    // Sync keychain key → n8n's config file so n8n doesn't see a mismatch.
    sync_encryption_key_to_n8n_config(&data_dir, &encryption_key);
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

    let npx_cmd = resolve_npx();
    let enriched_path = path_with_local_node();

    let child = std::process::Command::new(&npx_cmd)
        .arg("--yes")
        .arg("n8n@latest")
        .env("PATH", &enriched_path)
        .env("N8N_PORT", port.to_string())
        // SECURITY: bind to loopback only — never expose n8n to the LAN
        .env("N8N_HOST", "127.0.0.1")
        .env("N8N_BASIC_AUTH_ACTIVE", "false")
        .env("N8N_SECURE_COOKIE", "false")
        .env("N8N_ENCRYPTION_KEY", &encryption_key)
        .env("N8N_API_KEY", &api_key)
        .env("N8N_USER_FOLDER", data_dir.to_string_lossy().as_ref())
        .env("N8N_DIAGNOSTICS_ENABLED", "false")
        .env("N8N_PERSONALIZATION_ENABLED", "false")
        // Enable community node installation (verified registry only)
        .env("N8N_COMMUNITY_PACKAGES_ENABLED", "true")
        // Only allow verified packages — unverified can execute arbitrary code
        .env("N8N_COMMUNITY_PACKAGES_ALLOW_UNVERIFIED", "false")
        .env("N8N_REINSTALL_MISSING_PACKAGES", "true")
        .env("N8N_COMMUNITY_PACKAGES_ALLOW_TOOL_USAGE", "true")
        .stdout(stdout_sink)
        .stderr(stderr_sink)
        .spawn()
        .map_err(|e| EngineError::Other(format!("Failed to start npx n8n: {}", e)))?;

    let mut pid = child.id();
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

    // ── Post-startup crash detection ──────────────────────────────────
    //
    // n8n may pass the /healthz check and then crash shortly after while
    // loading community packages (known n8n bug: "Cannot read properties
    // of undefined (reading 'manager')" in InstalledNodesRepository).
    //
    // We wait 4s and re-probe.  If it's dead, inspect the log and attempt
    // a recovery restart with N8N_REINSTALL_MISSING_PACKAGES=true. If that
    // also fails, temporarily rename package.json to quarantine the broken
    // packages so n8n can at least boot.
    tokio::time::sleep(std::time::Duration::from_secs(4)).await;
    if !super::health::probe_n8n(&url, &api_key).await {
        let log_content = std::fs::read_to_string(&log_path).unwrap_or_default();
        let has_manager_crash = log_content
            .contains("Cannot read properties of undefined (reading 'manager')")
            || log_content.contains("installed-nodes.repository");

        if has_manager_crash {
            log::warn!("[n8n] Detected n8n DI crash (InstalledNodesRepository.manager) — attempting recovery");
            super::emit_status(
                app_handle,
                "starting",
                "Recovering from community-package crash…",
            );

            // Kill the crashed process
            stop_process(pid);
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;

            // Quarantine: rename package.json so n8n boots clean, then
            // restore it so N8N_REINSTALL_MISSING_PACKAGES can re-install.
            let pkg_json = data_dir.join("package.json");
            let pkg_json_bak = data_dir.join("package.json.bak");
            let had_pkg_json = pkg_json.exists();
            if had_pkg_json {
                let _ = std::fs::rename(&pkg_json, &pkg_json_bak);
                log::info!("[n8n] Quarantined community package.json for clean boot");
            }

            // Redirect fresh log
            let log_file2 = std::fs::File::create(&log_path).ok();
            let stdout2 = log_file2
                .as_ref()
                .and_then(|f| f.try_clone().ok())
                .map(std::process::Stdio::from)
                .unwrap_or_else(std::process::Stdio::null);
            let stderr2 = log_file2
                .and_then(|f| f.try_clone().ok())
                .map(std::process::Stdio::from)
                .unwrap_or_else(std::process::Stdio::null);

            let child2 = std::process::Command::new(&npx_cmd)
                .arg("--yes")
                .arg("n8n@latest")
                .env("PATH", &enriched_path)
                .env("N8N_PORT", port.to_string())
                .env("N8N_HOST", "127.0.0.1")
                .env("N8N_BASIC_AUTH_ACTIVE", "false")
                .env("N8N_SECURE_COOKIE", "false")
                .env("N8N_ENCRYPTION_KEY", &encryption_key)
                .env("N8N_API_KEY", &api_key)
                .env("N8N_USER_FOLDER", data_dir.to_string_lossy().as_ref())
                .env("N8N_DIAGNOSTICS_ENABLED", "false")
                .env("N8N_PERSONALIZATION_ENABLED", "false")
                .env("N8N_COMMUNITY_PACKAGES_ENABLED", "true")
                .env("N8N_COMMUNITY_PACKAGES_ALLOW_UNVERIFIED", "false")
                .env("N8N_REINSTALL_MISSING_PACKAGES", "true")
                .env("N8N_COMMUNITY_PACKAGES_ALLOW_TOOL_USAGE", "true")
                .stdout(stdout2)
                .stderr(stderr2)
                .spawn();

            match child2 {
                Ok(c2) => {
                    let pid2 = c2.id();
                    let ready2 = poll_n8n_ready(&url, &api_key).await;

                    // Restore package.json so the packages are visible again
                    if had_pkg_json && pkg_json_bak.exists() {
                        let _ = std::fs::rename(&pkg_json_bak, &pkg_json);
                        log::info!("[n8n] Restored community package.json after clean boot");
                    }

                    if ready2 {
                        log::info!("[n8n] Recovery restart succeeded (pid {})", pid2);
                        pid = pid2;
                    } else {
                        stop_process(pid2);
                        log::error!(
                            "[n8n] Recovery restart also failed — n8n may need manual intervention"
                        );
                        super::emit_status(
                            app_handle,
                            "error",
                            "Integration engine crashed due to a community-package bug. Try removing community packages from ~/.openpawz/n8n-data/package.json and restarting.",
                        );
                        return Err(EngineError::Other(
                            "n8n crashed during startup (InstalledNodesRepository.manager). Recovery restart also failed.".into()
                        ));
                    }
                }
                Err(e) => {
                    // Restore package.json even on spawn failure
                    if had_pkg_json && pkg_json_bak.exists() {
                        let _ = std::fs::rename(&pkg_json_bak, &pkg_json);
                    }
                    return Err(EngineError::Other(format!(
                        "Recovery restart failed to spawn: {}",
                        e
                    )));
                }
            }
        } else {
            // Not the manager crash — generic early exit
            let tail = std::fs::read_to_string(&log_path)
                .ok()
                .map(|s| {
                    let lines: Vec<&str> = s.lines().collect();
                    let start = lines.len().saturating_sub(20);
                    lines[start..].join("\n")
                })
                .unwrap_or_default();
            log::error!("[n8n] Process died shortly after health check:\n{}", tail);
            super::emit_status(
                app_handle,
                "error",
                "Integration engine started but crashed shortly after.",
            );
            return Err(EngineError::Other(
                "n8n process died shortly after passing health check".into(),
            ));
        }
    }

    // Set up owner + MCP access with retry.  n8n's internal services
    // (e.g. database migrations, encryption setup) may not be fully
    // initialized even though the health endpoint responds. We retry
    // up to 3 times with a short delay so the first-run experience is
    // seamless for the user.
    for attempt in 1..=3 {
        match super::health::setup_owner_if_needed(&url).await {
            Ok(_) => break,
            Err(e) if attempt < 3 => {
                log::info!(
                    "[n8n] Owner setup attempt {}/3 failed: {} — retrying…",
                    attempt,
                    e
                );
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
            Err(e) => {
                log::warn!(
                    "[n8n] Owner setup failed after 3 attempts (non-fatal): {}",
                    e
                );
            }
        }
    }

    // Log the n8n version for diagnostics (MCP requires recent versions)
    if let Some(version) = super::health::get_n8n_version(&url, &api_key).await {
        log::info!("[n8n] Process mode running n8n v{}", version);
    }

    // Enable MCP access with retry (disabled by default even after owner creation)
    for attempt in 1..=3 {
        match super::health::enable_mcp_access(&url, &api_key).await {
            Ok(_) => break,
            Err(e) if attempt < 3 => {
                log::info!(
                    "[n8n] MCP enable attempt {}/3 failed: {} — retrying…",
                    attempt,
                    e
                );
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
            Err(e) => {
                log::warn!(
                    "[n8n] MCP access enable failed after 3 attempts (non-fatal): {}",
                    e
                );
            }
        }
    }

    // Persist config (encryption key is NOT included — it lives in the vault)
    let new_config = N8nEngineConfig {
        mode: N8nMode::Process,
        url: url.clone(),
        api_key: api_key.clone(),
        container_id: None,
        container_port: None,
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

// ── Helpers ────────────────────────────────────────────────────────────

// ── Key vault storage for n8n encryption key ──────────────────────────

/// Retrieve the n8n encryption key from the unified key vault.
fn get_n8n_encryption_key_from_keychain() -> Option<String> {
    match key_vault::get(key_vault::PURPOSE_N8N_ENCRYPTION) {
        Some(key) if !key.is_empty() => {
            log::info!("[n8n] Retrieved encryption key from unified vault");
            Some(key)
        }
        _ => None,
    }
}

/// Store the n8n encryption key in the unified key vault.
fn store_n8n_encryption_key_in_keychain(key: &str) {
    key_vault::set(key_vault::PURPOSE_N8N_ENCRYPTION, key);
    log::info!("[n8n] Stored encryption key in unified vault");
}

/// Write the encryption key to n8n's config file so it matches the env var.
///
/// n8n checks `<N8N_USER_FOLDER>/.n8n/config` against `N8N_ENCRYPTION_KEY`
/// on startup. We write the keychain value here to prevent mismatch errors.
///
/// SECURITY NOTE: This file contains the encryption key in plaintext.
/// File permissions are restricted to owner-only (0o600).
fn sync_encryption_key_to_n8n_config(data_dir: &std::path::Path, key: &str) {
    let n8n_dir = data_dir.join(".n8n");
    let _ = std::fs::create_dir_all(&n8n_dir);
    let config_path = n8n_dir.join("config");

    let json = serde_json::json!({ "encryptionKey": key });
    match std::fs::write(
        &config_path,
        serde_json::to_string_pretty(&json).unwrap_or_default(),
    ) {
        Ok(_) => {
            // Restrict file permissions to owner-only
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ =
                    std::fs::set_permissions(&config_path, std::fs::Permissions::from_mode(0o600));
            }
            log::debug!("[n8n] Synced encryption key to {}", config_path.display());
        }
        Err(e) => log::warn!(
            "[n8n] Failed to sync encryption key to {}: {}",
            config_path.display(),
            e
        ),
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_node_major_standard() {
        assert_eq!(parse_node_major("v20.11.0\n"), 20);
        assert_eq!(parse_node_major("v18.19.1"), 18);
        assert_eq!(parse_node_major("v22.0.0"), 22);
    }

    #[test]
    fn parse_node_major_edge_cases() {
        assert_eq!(parse_node_major(""), 0);
        assert_eq!(parse_node_major("garbage"), 0);
        assert_eq!(parse_node_major("v"), 0);
        assert_eq!(parse_node_major("v14.17.0"), 14);
    }

    #[test]
    fn parse_node_major_with_whitespace() {
        assert_eq!(parse_node_major("  v20.11.0  "), 20);
        assert_eq!(parse_node_major("\nv18.0.0\n"), 18);
    }
}
