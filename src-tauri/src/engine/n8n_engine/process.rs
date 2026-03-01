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
    //   1. OS keychain (authoritative, secure store)
    //   2. n8n's own config file (plaintext fallback — migrated to keychain)
    //   3. Our saved config (from a previous provision)
    //   4. Generate a new random key (first-ever run)
    let encryption_key = get_n8n_encryption_key_from_keychain()
        .or_else(|| {
            // Migrate from n8n's plaintext config → keychain
            let key = read_n8n_encryption_key(&data_dir)?;
            log::info!("[n8n] Migrating encryption key from n8n config file to OS keychain");
            store_n8n_encryption_key_in_keychain(&key);
            Some(key)
        })
        .or_else(|| {
            prev_config
                .as_ref()
                .and_then(|c| c.encryption_key.clone())
                .filter(|k| !k.is_empty())
                .inspect(|key| {
                    log::info!("[n8n] Migrating encryption key from saved config to OS keychain");
                    store_n8n_encryption_key_in_keychain(key);
                })
        })
        .unwrap_or_else(|| {
            log::info!("[n8n] No existing encryption key found — generating new one");
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
        match super::health::enable_mcp_access(&url).await {
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

// ── Helpers ────────────────────────────────────────────────────────────

/// Read n8n's own encryption key from its config file.
///
/// n8n stores `{"encryptionKey": "..."}` in `<N8N_USER_FOLDER>/.n8n/config`.
/// Used as a migration source — the key is moved to the OS keychain.
fn read_n8n_encryption_key(data_dir: &std::path::Path) -> Option<String> {
    let config_path = data_dir.join(".n8n").join("config");
    let content = std::fs::read_to_string(&config_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    let key = json.get("encryptionKey")?.as_str()?.to_string();
    if key.is_empty() {
        return None;
    }
    log::info!(
        "[n8n] Read encryption key from n8n config at {}",
        config_path.display()
    );
    Some(key)
}

// ── Keychain storage for n8n encryption key ────────────────────────────

const N8N_KEY_SERVICE: &str = "paw-n8n-encryption";
const N8N_KEY_USER: &str = "openpawz-n8n";

/// Retrieve the n8n encryption key from the OS keychain.
fn get_n8n_encryption_key_from_keychain() -> Option<String> {
    let entry = keyring::Entry::new(N8N_KEY_SERVICE, N8N_KEY_USER).ok()?;
    match entry.get_password() {
        Ok(key) if !key.is_empty() => {
            log::info!("[n8n] Retrieved encryption key from OS keychain");
            Some(key)
        }
        _ => None,
    }
}

/// Store the n8n encryption key in the OS keychain.
fn store_n8n_encryption_key_in_keychain(key: &str) {
    match keyring::Entry::new(N8N_KEY_SERVICE, N8N_KEY_USER) {
        Ok(entry) => match entry.set_password(key) {
            Ok(_) => log::info!("[n8n] Stored encryption key in OS keychain"),
            Err(e) => log::warn!("[n8n] Failed to store encryption key in keychain: {}", e),
        },
        Err(e) => log::warn!("[n8n] Failed to init keyring entry: {}", e),
    }
}

/// Write the encryption key to n8n's config file so it matches the env var.
///
/// n8n checks `<N8N_USER_FOLDER>/.n8n/config` against `N8N_ENCRYPTION_KEY`
/// on startup. We write the keychain value here to prevent mismatch errors.
fn sync_encryption_key_to_n8n_config(data_dir: &std::path::Path, key: &str) {
    let n8n_dir = data_dir.join(".n8n");
    let _ = std::fs::create_dir_all(&n8n_dir);
    let config_path = n8n_dir.join("config");

    let json = serde_json::json!({ "encryptionKey": key });
    match std::fs::write(
        &config_path,
        serde_json::to_string_pretty(&json).unwrap_or_default(),
    ) {
        Ok(_) => log::debug!("[n8n] Synced encryption key to {}", config_path.display()),
        Err(e) => log::warn!(
            "[n8n] Failed to sync encryption key to {}: {}",
            config_path.display(),
            e
        ),
    }
}
