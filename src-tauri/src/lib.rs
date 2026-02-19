use std::process::Command;
use std::path::PathBuf;
use std::fs;
use log::{info, warn, error};
use tauri::{Emitter, Manager};
use crate::atoms::constants::{DB_KEY_SERVICE, DB_KEY_USER};

// ── Paw Atoms (constants, error types) ────────────────────────────────────
pub mod atoms;

// ── Paw Agent Engine ───────────────────────────────────────────────────
pub mod engine;

// ── Paw Command Modules (Systems layer) ───────────────────────────────
pub mod commands;

/// Set restrictive file permissions (owner-only read/write) on Unix.
/// No-op on non-Unix platforms.
#[cfg(unix)]
fn set_owner_only_permissions(path: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let perms = fs::Permissions::from_mode(0o600);
    fs::set_permissions(path, perms)
        .map_err(|e| format!("Failed to set permissions on {:?}: {}", path, e))
}

#[cfg(not(unix))]
fn set_owner_only_permissions(_path: &std::path::Path) -> Result<(), String> {
    Ok(()) // Windows ACLs not handled here yet
}


/// Write (or merge) a Himalaya TOML config for an IMAP/SMTP email account.
/// Creates ~/.config/himalaya/config.toml with the account settings.
/// Password is stored in the OS keychain (macOS Keychain / libsecret / Windows
/// Credential Manager) — NOT in the TOML file.  The TOML only contains a
/// keyring reference so himalaya can look it up at runtime.
#[tauri::command]
fn write_himalaya_config(
    account_name: String,
    email: String,
    display_name: Option<String>,
    imap_host: String,
    imap_port: u16,
    smtp_host: String,
    smtp_port: u16,
    password: String,
) -> Result<bool, String> {
    let home = dirs::home_dir().ok_or("Cannot find home directory")?;
    let config_dir = home.join(".config/himalaya");
    let config_path = config_dir.join("config.toml");

    fs::create_dir_all(&config_dir)
        .map_err(|e| format!("Failed to create config dir: {}", e))?;

    // ── Store password in OS keychain ──────────────────────────────────
    let keyring_service = format!("paw-mail-{}", account_name);
    let entry = keyring::Entry::new(&keyring_service, &email)
        .map_err(|e| format!("Keyring init failed: {}", e))?;
    entry.set_password(&password)
        .map_err(|e| format!("Failed to store password in keychain: {}", e))?;
    info!("Stored password for '{}' in OS keychain (service={})", email, keyring_service);

    // ── Build TOML — password is a keyring reference, NOT plaintext ────
    let display = display_name.unwrap_or_else(|| email.clone());
    let account_toml = format!(
        r#"[accounts.{name}]
email = "{email}"
display-name = "{display}"

backend.type = "imap"
backend.host = "{imap_host}"
backend.port = {imap_port}
backend.encryption = "tls"
backend.login = "{email}"
backend.auth.type = "password"
backend.auth.cmd = "security find-generic-password -s '{service}' -a '{email}' -w 2>/dev/null || secret-tool lookup service '{service}' username '{email}' 2>/dev/null"

message.send.backend.type = "smtp"
message.send.backend.host = "{smtp_host}"
message.send.backend.port = {smtp_port}
message.send.backend.encryption = "tls"
message.send.backend.login = "{email}"
message.send.backend.auth.type = "password"
message.send.backend.auth.cmd = "security find-generic-password -s '{service}' -a '{email}' -w 2>/dev/null || secret-tool lookup service '{service}' username '{email}' 2>/dev/null"
"#,
        name = account_name,
        email = email,
        display = display,
        imap_host = imap_host,
        imap_port = imap_port,
        smtp_host = smtp_host,
        smtp_port = smtp_port,
        service = keyring_service,
    );

    if config_path.exists() {
        let existing = fs::read_to_string(&config_path)
            .map_err(|e| format!("Failed to read existing config: {}", e))?;

        let marker = format!("[accounts.{}]", account_name);
        if let Some(start) = existing.find(&marker) {
            let rest = &existing[start + marker.len()..];
            let end_offset = if let Some(next) = rest.find("\n[accounts.") {
                start + marker.len() + next
            } else {
                existing.len()
            };
            let mut new_content = String::new();
            new_content.push_str(&existing[..start]);
            new_content.push_str(&account_toml);
            new_content.push('\n');
            if end_offset < existing.len() {
                new_content.push_str(&existing[end_offset..]);
            }
            fs::write(&config_path, new_content.trim_end())
                .map_err(|e| format!("Failed to write config: {}", e))?;
        } else {
            let mut content = existing;
            if !content.ends_with('\n') {
                content.push('\n');
            }
            content.push('\n');
            content.push_str(&account_toml);
            fs::write(&config_path, content.trim_end())
                .map_err(|e| format!("Failed to write config: {}", e))?;
        }
    } else {
        fs::write(&config_path, account_toml.trim_end())
            .map_err(|e| format!("Failed to write config: {}", e))?;
    }

    set_owner_only_permissions(&config_path)?;

    info!("Wrote himalaya config for account '{}' at {:?} (mode 600, password in keychain)", account_name, config_path);
    Ok(true)
}

/// Read the current Himalaya config to list configured accounts.
/// Passwords are stored in the OS keychain — the TOML only contains command
/// references.  We still redact the auth.cmd lines so the frontend never sees
/// the shell command that retrieves the password.
#[tauri::command]
fn read_himalaya_config() -> Result<String, String> {
    let home = dirs::home_dir().ok_or("Cannot find home directory")?;
    let config_path = home.join(".config/himalaya/config.toml");
    if !config_path.exists() {
        return Ok(String::new());
    }
    let raw = fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read config: {}", e))?;

    // Redact auth command and raw password lines so they never reach JS.
    let redacted = raw.lines().map(|line| {
        let trimmed = line.trim();
        if trimmed.starts_with("backend.auth.raw")
            || trimmed.starts_with("message.send.backend.auth.raw")
            || trimmed.starts_with("backend.auth.cmd")
            || trimmed.starts_with("message.send.backend.auth.cmd")
        {
            if let Some(eq) = line.find('=') {
                format!("{} \"[stored in OS keychain]\"", &line[..eq+1])
            } else {
                line.to_string()
            }
        } else {
            line.to_string()
        }
    }).collect::<Vec<_>>().join("\n");

    Ok(redacted)
}

/// Remove a Himalaya account from the config file and delete its password from
/// the OS keychain.
#[tauri::command]
fn remove_himalaya_account(account_name: String) -> Result<bool, String> {
    let home = dirs::home_dir().ok_or("Cannot find home directory")?;
    let config_path = home.join(".config/himalaya/config.toml");

    // ── Try to delete password from OS keychain ──────────────────────
    // We need the email for the keyring lookup.  Parse it from the TOML.
    if config_path.exists() {
        if let Ok(raw) = fs::read_to_string(&config_path) {
            // Extract email from [accounts.<name>] section
            let marker = format!("[accounts.{}]", account_name);
            if let Some(start) = raw.find(&marker) {
                let section = &raw[start..];
                for line in section.lines().skip(1) {
                    if line.trim().starts_with("[accounts.") {
                        break;
                    }
                    if line.trim().starts_with("email") {
                        if let Some(eq) = line.find('=') {
                            let email = line[eq+1..].trim().trim_matches('"').to_string();
                            let service = format!("paw-mail-{}", account_name);
                            if let Ok(entry) = keyring::Entry::new(&service, &email) {
                                match entry.delete_credential() {
                                    Ok(()) => info!("Deleted keychain entry for '{}' (service={})", email, service),
                                    Err(e) => info!("Keychain delete for '{}': {} (may not exist)", email, e),
                                }
                            }
                        }
                        break;
                    }
                }
            }
        }
    }

    // ── Remove from TOML ─────────────────────────────────────────────
    if !config_path.exists() {
        return Ok(false);
    }

    let existing = fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read config: {}", e))?;

    let marker = format!("[accounts.{}]", account_name);
    if let Some(start) = existing.find(&marker) {
        let rest = &existing[start + marker.len()..];
        let end_offset = if let Some(next) = rest.find("\n[accounts.") {
            start + marker.len() + next
        } else {
            existing.len()
        };
        let mut new_content = String::new();
        new_content.push_str(existing[..start].trim_end());
        if end_offset < existing.len() {
            new_content.push('\n');
            new_content.push_str(existing[end_offset..].trim_start());
        }
        let final_content = new_content.trim().to_string();
        if final_content.is_empty() {
            fs::remove_file(&config_path)
                .map_err(|e| format!("Failed to remove config: {}", e))?;
        } else {
            fs::write(&config_path, final_content)
                .map_err(|e| format!("Failed to write config: {}", e))?;
            set_owner_only_permissions(&config_path)?;
        }
        Ok(true)
    } else {
        Ok(false)
    }
}

// ── Keyring helpers ──────────────────────────────────────────────────────────

/// Check whether the OS keychain has a stored password for the given account.
#[tauri::command]
fn keyring_has_password(account_name: String, email: String) -> Result<bool, String> {
    let service = format!("paw-mail-{}", account_name);
    let entry = keyring::Entry::new(&service, &email)
        .map_err(|e| format!("Keyring init failed: {}", e))?;
    match entry.get_password() {
        Ok(_) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(format!("Keyring error: {}", e)),
    }
}

/// Delete a password from the OS keychain.
#[tauri::command]
fn keyring_delete_password(account_name: String, email: String) -> Result<bool, String> {
    let service = format!("paw-mail-{}", account_name);
    let entry = keyring::Entry::new(&service, &email)
        .map_err(|e| format!("Keyring init failed: {}", e))?;
    match entry.delete_credential() {
        Ok(()) => {
            info!("Deleted keychain entry for '{}' (service={})", email, service);
            Ok(true)
        }
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(format!("Keyring delete failed: {}", e)),
    }
}

// ── Database encryption key (C2) ─────────────────────────────────────────────
// DB_KEY_SERVICE and DB_KEY_USER are defined in crate::atoms::constants.

/// Get or create a 256-bit database encryption key stored in the OS keychain.
/// On first call, generates a random key and stores it. Subsequent calls return
/// the same key. This key is used by the frontend to encrypt/decrypt sensitive
/// fields before writing them to SQLite.
#[tauri::command]
fn get_db_encryption_key() -> Result<String, String> {
    let entry = keyring::Entry::new(DB_KEY_SERVICE, DB_KEY_USER)
        .map_err(|e| format!("Keyring init failed: {}", e))?;
    match entry.get_password() {
        Ok(key) => {
            info!("Retrieved DB encryption key from OS keychain");
            Ok(key)
        }
        Err(keyring::Error::NoEntry) => {
            // Generate a random 256-bit key (hex-encoded = 64 chars)
            use rand::Rng;
            let key: String = (0..32)
                .map(|_| format!("{:02x}", rand::thread_rng().gen::<u8>()))
                .collect();
            entry.set_password(&key)
                .map_err(|e| format!("Failed to store DB key: {}", e))?;
            info!("Generated and stored new DB encryption key in OS keychain");
            Ok(key)
        }
        Err(e) => Err(format!("Keyring error: {}", e)),
    }
}

/// Check if a DB encryption key exists (for UI indicators).
#[tauri::command]
fn has_db_encryption_key() -> bool {
    keyring::Entry::new(DB_KEY_SERVICE, DB_KEY_USER)
        .ok()
        .and_then(|e| e.get_password().ok())
        .is_some()
}

/// Fetch weather data via wttr.in (bypasses CSP for the frontend).
#[tauri::command]
async fn fetch_weather(location: Option<String>) -> Result<String, String> {
    let loc = location.unwrap_or_default();
    let url = format!("https://wttr.in/{}?format=j1", loc);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;
    let resp = client
        .get(&url)
        .header("User-Agent", "curl")
        .send()
        .await
        .map_err(|e| format!("Weather fetch failed: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("Weather API returned {}", resp.status()));
    }
    resp.text()
        .await
        .map_err(|e| format!("Failed to read weather response: {}", e))
}

/// Fetch emails from an IMAP account via himalaya CLI.
#[tauri::command]
fn fetch_emails(account: Option<String>, folder: Option<String>, page_size: Option<u32>) -> Result<String, String> {
    let mut cmd = Command::new("himalaya");
    cmd.arg("envelope").arg("list");
    if let Some(acct) = account {
        cmd.arg("--account").arg(acct);
    }
    if let Some(f) = folder {
        cmd.arg("--folder").arg(f);
    }
    cmd.arg("--page-size").arg(page_size.unwrap_or(50).to_string());
    cmd.arg("--output").arg("json");
    let output = cmd.output().map_err(|e| format!("Failed to run himalaya: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("himalaya failed: {}", stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Fetch a single email's full content via himalaya CLI.
#[tauri::command]
fn fetch_email_content(account: Option<String>, folder: Option<String>, id: String) -> Result<String, String> {
    let mut cmd = Command::new("himalaya");
    cmd.arg("message").arg("read");
    if let Some(acct) = account {
        cmd.arg("--account").arg(acct);
    }
    if let Some(f) = folder {
        cmd.arg("--folder").arg(f);
    }
    cmd.arg(&id);
    let output = cmd.output().map_err(|e| format!("Failed to run himalaya: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("himalaya failed: {}", stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Send an email via himalaya CLI.
#[tauri::command]
fn send_email(account: Option<String>, to: String, subject: String, body: String) -> Result<(), String> {
    let mut cmd = Command::new("himalaya");
    cmd.arg("template").arg("send");
    if let Some(acct) = account {
        cmd.arg("--account").arg(acct);
    }
    let email_template = format!("To: {}\nSubject: {}\n\n{}", to, subject, body);
    cmd.stdin(std::process::Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn himalaya: {}", e))?;
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin.write_all(email_template.as_bytes()).map_err(|e| format!("Failed to write: {}", e))?;
    }
    let output = child.wait_with_output().map_err(|e| format!("Failed to wait: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("Folder doesn't exist") {
            return Err(format!("himalaya failed: {}", stderr));
        }
    }
    Ok(())
}

/// List mail folders via himalaya CLI.
#[tauri::command]
fn list_mail_folders(account: Option<String>) -> Result<String, String> {
    let mut cmd = Command::new("himalaya");
    cmd.arg("folder").arg("list");
    if let Some(acct) = account {
        cmd.arg("--account").arg(acct);
    }
    cmd.arg("--output").arg("json");
    let output = cmd.output().map_err(|e| format!("Failed to run himalaya: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("himalaya failed: {}", stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Move an email to a folder.
#[tauri::command]
fn move_email(account: Option<String>, id: String, folder: String) -> Result<(), String> {
    let mut cmd = Command::new("himalaya");
    cmd.arg("message").arg("move");
    if let Some(acct) = account {
        cmd.arg("--account").arg(acct);
    }
    cmd.arg(&id).arg(&folder);
    let output = cmd.output().map_err(|e| format!("Failed to run himalaya: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("himalaya failed: {}", stderr));
    }
    Ok(())
}

/// Delete an email.
#[tauri::command]
fn delete_email(account: Option<String>, id: String) -> Result<(), String> {
    let mut cmd = Command::new("himalaya");
    cmd.arg("message").arg("delete");
    if let Some(acct) = account {
        cmd.arg("--account").arg(acct);
    }
    cmd.arg(&id);
    let output = cmd.output().map_err(|e| format!("Failed to run himalaya: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("himalaya failed: {}", stderr));
    }
    Ok(())
}

/// Mark email as read/unread.
#[tauri::command]
fn set_email_flag(account: Option<String>, id: String, flag: String, add: bool) -> Result<(), String> {
    let mut cmd = Command::new("himalaya");
    cmd.arg("flag").arg(if add { "add" } else { "remove" });
    if let Some(acct) = account {
        cmd.arg("--account").arg(acct);
    }
    cmd.arg(&id).arg("--flag").arg(&flag);
    let output = cmd.output().map_err(|e| format!("Failed to run himalaya: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("himalaya failed: {}", stderr));
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize the Paw Agent Engine
    let engine_state = engine::commands::EngineState::new()
        .expect("Failed to initialize Paw Agent Engine");

    tauri::Builder::default()
        .manage(engine_state)
        .plugin(tauri_plugin_log::Builder::new()
            .target(tauri_plugin_log::Target::new(
                tauri_plugin_log::TargetKind::LogDir { file_name: Some("openpawz".into()) },
            ))
            .max_file_size(5_000_000) // 5MB max per log file
            .build())
        .plugin(tauri_plugin_sql::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            // ── Cron Heartbeat: autonomous task execution ──
            // Spawns a background loop that checks for due cron tasks
            // every 60 seconds and auto-executes them.
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // Initial delay: let the app fully initialize
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                log::info!("[heartbeat] Cron heartbeat started (60s interval)");

                loop {
                    engine::commands::run_cron_heartbeat(&app_handle).await;
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // ── lib.rs commands (email, weather, keychain, db crypto) ──
            write_himalaya_config,
            read_himalaya_config,
            remove_himalaya_account,
            keyring_has_password,
            keyring_delete_password,
            fetch_weather,
            fetch_emails,
            fetch_email_content,
            send_email,
            list_mail_folders,
            move_email,
            delete_email,
            set_email_flag,
            get_db_encryption_key,
            has_db_encryption_key,
            // ── Paw Agent Engine commands (no gateway needed) ──
            commands::chat::engine_chat_send,
            commands::chat::engine_chat_history,
            commands::chat::engine_sessions_list,
            commands::chat::engine_session_rename,
            commands::chat::engine_session_delete,
            commands::chat::engine_session_clear,
            commands::chat::engine_session_compact,
            engine::commands::engine_sandbox_check,
            engine::commands::engine_sandbox_get_config,
            engine::commands::engine_sandbox_set_config,
            engine::commands::engine_get_config,
            engine::commands::engine_get_daily_spend,
            engine::commands::engine_set_config,
            engine::commands::engine_upsert_provider,
            engine::commands::engine_remove_provider,
            engine::commands::engine_status,
            engine::commands::engine_auto_setup,
            commands::chat::engine_approve_tool,
            // ── Agent Files (Soul / Persona) ──
            commands::agent::engine_agent_file_list,
            commands::agent::engine_agent_file_get,
            commands::agent::engine_agent_file_set,
            commands::agent::engine_agent_file_delete,
            // ── Memory (Long-term Semantic) ──
            commands::memory::engine_memory_store,
            commands::memory::engine_memory_search,
            commands::memory::engine_memory_stats,
            commands::memory::engine_memory_delete,
            commands::memory::engine_memory_list,
            commands::memory::engine_get_memory_config,
            commands::memory::engine_set_memory_config,
            commands::memory::engine_test_embedding,
            commands::memory::engine_embedding_status,
            commands::memory::engine_embedding_pull_model,
            commands::memory::engine_ensure_embedding_ready,
            commands::memory::engine_memory_backfill,
            // ── Skill Vault ──
            commands::skills::engine_skills_list,
            commands::skills::engine_skill_set_enabled,
            commands::skills::engine_skill_set_credential,
            commands::skills::engine_skill_delete_credential,
            commands::skills::engine_skill_revoke_all,
            commands::skills::engine_skill_get_instructions,
            commands::skills::engine_skill_set_instructions,
            // ── Trading ──
            engine::commands::engine_trading_history,
            engine::commands::engine_trading_summary,
            engine::commands::engine_trading_policy_get,
            engine::commands::engine_trading_policy_set,
            // ── Positions (Stop-Loss / Take-Profit) ──
            engine::commands::engine_positions_list,
            engine::commands::engine_position_close,
            engine::commands::engine_position_update_targets,
            // ── Text-to-Speech ──
            engine::commands::engine_tts_speak,
            engine::commands::engine_tts_get_config,
            engine::commands::engine_tts_set_config,
            // ── Tasks (Kanban Board) ──
            engine::commands::engine_tasks_list,
            engine::commands::engine_task_create,
            engine::commands::engine_task_update,
            engine::commands::engine_task_delete,
            engine::commands::engine_task_move,
            engine::commands::engine_task_activity,
            engine::commands::engine_task_set_agents,
            engine::commands::engine_task_run,
            engine::commands::engine_tasks_cron_tick,
            // ── Telegram Bridge ──
            engine::commands::engine_telegram_start,
            engine::commands::engine_telegram_stop,
            engine::commands::engine_telegram_status,
            engine::commands::engine_telegram_get_config,
            engine::commands::engine_telegram_set_config,
            engine::commands::engine_telegram_approve_user,
            engine::commands::engine_telegram_deny_user,
            engine::commands::engine_telegram_remove_user,
            // ── Discord Bridge ──
            engine::commands::engine_discord_start,
            engine::commands::engine_discord_stop,
            engine::commands::engine_discord_status,
            engine::commands::engine_discord_get_config,
            engine::commands::engine_discord_set_config,
            engine::commands::engine_discord_approve_user,
            engine::commands::engine_discord_deny_user,
            engine::commands::engine_discord_remove_user,
            // ── IRC Bridge ──
            engine::commands::engine_irc_start,
            engine::commands::engine_irc_stop,
            engine::commands::engine_irc_status,
            engine::commands::engine_irc_get_config,
            engine::commands::engine_irc_set_config,
            engine::commands::engine_irc_approve_user,
            engine::commands::engine_irc_deny_user,
            engine::commands::engine_irc_remove_user,
            // ── Slack Bridge ──
            engine::commands::engine_slack_start,
            engine::commands::engine_slack_stop,
            engine::commands::engine_slack_status,
            engine::commands::engine_slack_get_config,
            engine::commands::engine_slack_set_config,
            engine::commands::engine_slack_approve_user,
            engine::commands::engine_slack_deny_user,
            engine::commands::engine_slack_remove_user,
            // ── Matrix Bridge ──
            engine::commands::engine_matrix_start,
            engine::commands::engine_matrix_stop,
            engine::commands::engine_matrix_status,
            engine::commands::engine_matrix_get_config,
            engine::commands::engine_matrix_set_config,
            engine::commands::engine_matrix_approve_user,
            engine::commands::engine_matrix_deny_user,
            engine::commands::engine_matrix_remove_user,
            // ── Mattermost Bridge ──
            engine::commands::engine_mattermost_start,
            engine::commands::engine_mattermost_stop,
            engine::commands::engine_mattermost_status,
            engine::commands::engine_mattermost_get_config,
            engine::commands::engine_mattermost_set_config,
            engine::commands::engine_mattermost_approve_user,
            engine::commands::engine_mattermost_deny_user,
            engine::commands::engine_mattermost_remove_user,
            // ── Nextcloud Talk Bridge ──
            engine::commands::engine_nextcloud_start,
            engine::commands::engine_nextcloud_stop,
            engine::commands::engine_nextcloud_status,
            engine::commands::engine_nextcloud_get_config,
            engine::commands::engine_nextcloud_set_config,
            engine::commands::engine_nextcloud_approve_user,
            engine::commands::engine_nextcloud_deny_user,
            engine::commands::engine_nextcloud_remove_user,
            // ── Nostr Bridge ──
            engine::commands::engine_nostr_start,
            engine::commands::engine_nostr_stop,
            engine::commands::engine_nostr_status,
            engine::commands::engine_nostr_get_config,
            engine::commands::engine_nostr_set_config,
            engine::commands::engine_nostr_approve_user,
            engine::commands::engine_nostr_deny_user,
            engine::commands::engine_nostr_remove_user,
            // ── Twitch Bridge ──
            engine::commands::engine_twitch_start,
            engine::commands::engine_twitch_stop,
            engine::commands::engine_twitch_status,
            engine::commands::engine_twitch_get_config,
            engine::commands::engine_twitch_set_config,
            engine::commands::engine_twitch_approve_user,
            engine::commands::engine_twitch_deny_user,
            engine::commands::engine_twitch_remove_user,
            // ── Web Chat Bridge ──
            engine::commands::engine_webchat_start,
            engine::commands::engine_webchat_stop,
            engine::commands::engine_webchat_status,
            engine::commands::engine_webchat_get_config,
            engine::commands::engine_webchat_set_config,
            engine::commands::engine_webchat_approve_user,
            engine::commands::engine_webchat_deny_user,
            engine::commands::engine_webchat_remove_user,
            // ── Orchestrator: Projects ──
            commands::project::engine_projects_list,
            commands::project::engine_project_create,
            commands::project::engine_project_update,
            commands::project::engine_project_delete,
            commands::project::engine_project_set_agents,
            commands::agent::engine_list_all_agents,
            commands::agent::engine_create_agent,
            commands::agent::engine_delete_agent,
            commands::project::engine_project_messages,
            commands::project::engine_project_run,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
