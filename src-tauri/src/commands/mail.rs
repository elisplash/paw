// commands/mail.rs — Himalaya email bridge commands + Gmail API bridge.

use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::fs;
use std::process::Command;

/// Set restrictive file permissions (owner-only read/write) on Unix.
#[cfg(unix)]
fn set_owner_only_permissions(path: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let perms = fs::Permissions::from_mode(0o600);
    fs::set_permissions(path, perms)
        .map_err(|e| format!("Failed to set permissions on {:?}: {}", path, e))
}

#[cfg(not(unix))]
fn set_owner_only_permissions(_path: &std::path::Path) -> Result<(), String> {
    Ok(())
}

/// Write (or merge) a Himalaya TOML config for an IMAP/SMTP email account.
/// Password is stored in the OS keychain — the TOML contains only a keyring
/// command reference so himalaya can look it up at runtime.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn write_himalaya_config(
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

    fs::create_dir_all(&config_dir).map_err(|e| format!("Failed to create config dir: {}", e))?;

    let keyring_service = format!("paw-mail-{}", account_name);
    let entry = keyring::Entry::new(&keyring_service, &email)
        .map_err(|e| format!("Keyring init failed: {}", e))?;
    entry
        .set_password(&password)
        .map_err(|e| format!("Failed to store password in keychain: {}", e))?;
    info!(
        "Stored password for '{}' in OS keychain (service={})",
        email, keyring_service
    );

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
    info!(
        "Wrote himalaya config for account '{}' at {:?} (mode 600, password in keychain)",
        account_name, config_path
    );
    Ok(true)
}

/// Read the current Himalaya config. Auth command lines are redacted.
#[tauri::command]
pub fn read_himalaya_config() -> Result<String, String> {
    let home = dirs::home_dir().ok_or("Cannot find home directory")?;
    let config_path = home.join(".config/himalaya/config.toml");
    if !config_path.exists() {
        return Ok(String::new());
    }
    let raw =
        fs::read_to_string(&config_path).map_err(|e| format!("Failed to read config: {}", e))?;

    let redacted = raw
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("backend.auth.raw")
                || trimmed.starts_with("message.send.backend.auth.raw")
                || trimmed.starts_with("backend.auth.cmd")
                || trimmed.starts_with("message.send.backend.auth.cmd")
            {
                if let Some(eq) = line.find('=') {
                    format!("{} \"[stored in OS keychain]\"", &line[..eq + 1])
                } else {
                    line.to_string()
                }
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    Ok(redacted)
}

/// Remove a Himalaya account from config + delete its keychain entry.
#[tauri::command]
pub fn remove_himalaya_account(account_name: String) -> Result<bool, String> {
    let home = dirs::home_dir().ok_or("Cannot find home directory")?;
    let config_path = home.join(".config/himalaya/config.toml");

    if config_path.exists() {
        if let Ok(raw) = fs::read_to_string(&config_path) {
            let marker = format!("[accounts.{}]", account_name);
            if let Some(start) = raw.find(&marker) {
                let section = &raw[start..];
                for line in section.lines().skip(1) {
                    if line.trim().starts_with("[accounts.") {
                        break;
                    }
                    if line.trim().starts_with("email") {
                        if let Some(eq) = line.find('=') {
                            let email = line[eq + 1..].trim().trim_matches('"').to_string();
                            let service = format!("paw-mail-{}", account_name);
                            if let Ok(entry) = keyring::Entry::new(&service, &email) {
                                match entry.delete_credential() {
                                    Ok(()) => info!(
                                        "Deleted keychain entry for '{}' (service={})",
                                        email, service
                                    ),
                                    Err(e) => info!(
                                        "Keychain delete for '{}': {} (may not exist)",
                                        email, e
                                    ),
                                }
                            }
                        }
                        break;
                    }
                }
            }
        }
    }

    if !config_path.exists() {
        return Ok(false);
    }

    let existing =
        fs::read_to_string(&config_path).map_err(|e| format!("Failed to read config: {}", e))?;

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
            fs::remove_file(&config_path).map_err(|e| format!("Failed to remove config: {}", e))?;
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

/// Fetch emails from an IMAP account via himalaya CLI.
#[tauri::command]
pub fn fetch_emails(
    account: Option<String>,
    folder: Option<String>,
    page_size: Option<u32>,
) -> Result<String, String> {
    let mut cmd = Command::new("himalaya");
    cmd.arg("envelope").arg("list");
    if let Some(acct) = account {
        cmd.arg("--account").arg(acct);
    }
    if let Some(f) = folder {
        cmd.arg("--folder").arg(f);
    }
    cmd.arg("--page-size")
        .arg(page_size.unwrap_or(50).to_string());
    cmd.arg("--output").arg("json");
    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run himalaya: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("himalaya failed: {}", stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Fetch a single email's full content via himalaya CLI.
#[tauri::command]
pub fn fetch_email_content(
    account: Option<String>,
    folder: Option<String>,
    id: String,
) -> Result<String, String> {
    let mut cmd = Command::new("himalaya");
    cmd.arg("message").arg("read");
    if let Some(acct) = account {
        cmd.arg("--account").arg(acct);
    }
    if let Some(f) = folder {
        cmd.arg("--folder").arg(f);
    }
    cmd.arg(&id);
    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run himalaya: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("himalaya failed: {}", stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Send an email via himalaya CLI.
#[tauri::command]
pub fn send_email(
    account: Option<String>,
    to: String,
    subject: String,
    body: String,
) -> Result<(), String> {
    let mut cmd = Command::new("himalaya");
    cmd.arg("template").arg("send");
    if let Some(acct) = account {
        cmd.arg("--account").arg(acct);
    }
    let email_template = format!("To: {}\nSubject: {}\n\n{}", to, subject, body);
    cmd.stdin(std::process::Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn himalaya: {}", e))?;
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin
            .write_all(email_template.as_bytes())
            .map_err(|e| format!("Failed to write: {}", e))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed to wait: {}", e))?;
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
pub fn list_mail_folders(account: Option<String>) -> Result<String, String> {
    let mut cmd = Command::new("himalaya");
    cmd.arg("folder").arg("list");
    if let Some(acct) = account {
        cmd.arg("--account").arg(acct);
    }
    cmd.arg("--output").arg("json");
    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run himalaya: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("himalaya failed: {}", stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Move an email to a folder via himalaya CLI.
#[tauri::command]
pub fn move_email(account: Option<String>, id: String, folder: String) -> Result<(), String> {
    let mut cmd = Command::new("himalaya");
    cmd.arg("message").arg("move");
    if let Some(acct) = account {
        cmd.arg("--account").arg(acct);
    }
    cmd.arg(&id).arg(&folder);
    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run himalaya: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("himalaya failed: {}", stderr));
    }
    Ok(())
}

/// Delete an email via himalaya CLI.
#[tauri::command]
pub fn delete_email(account: Option<String>, id: String) -> Result<(), String> {
    let mut cmd = Command::new("himalaya");
    cmd.arg("message").arg("delete");
    if let Some(acct) = account {
        cmd.arg("--account").arg(acct);
    }
    cmd.arg(&id);
    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run himalaya: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("himalaya failed: {}", stderr));
    }
    Ok(())
}

/// Set/remove a flag on an email via himalaya CLI.
#[tauri::command]
pub fn set_email_flag(
    account: Option<String>,
    id: String,
    flag: String,
    add: bool,
) -> Result<(), String> {
    let mut cmd = Command::new("himalaya");
    cmd.arg("flag").arg(if add { "add" } else { "remove" });
    if let Some(acct) = account {
        cmd.arg("--account").arg(acct);
    }
    cmd.arg(&id).arg("--flag").arg(&flag);
    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run himalaya: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("himalaya failed: {}", stderr));
    }
    Ok(())
}

// ── Gmail API Inbox ────────────────────────────────────────────────────

/// A single Gmail message returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailMessage {
    pub id: String,
    pub from: String,
    pub subject: String,
    pub snippet: String,
    pub date: String,
    pub read: bool,
}

/// Fetch inbox messages via Gmail API using the stored Google OAuth token.
/// Returns an empty list if Gmail/Google is not connected.
#[tauri::command]
pub async fn engine_gmail_inbox(page_size: Option<u32>) -> Result<Vec<GmailMessage>, String> {
    use crate::engine::key_vault;
    use crate::engine::skills::crypto::{decrypt_credential, get_vault_key};

    // ── 1. Load the OAuth access token ───────────────────────────────
    let vault_key = get_vault_key().map_err(|e| format!("Vault key error: {e}"))?;
    let encrypted =
        match key_vault::get("oauth:google-workspace").or_else(|| key_vault::get("oauth:google")) {
            Some(v) => v,
            None => {
                info!("[gmail] No Google OAuth tokens found");
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

    // ── 2. List messages from inbox ──────────────────────────────────
    let max = page_size.unwrap_or(20).min(50);
    let list_url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/me/messages?maxResults={}&labelIds=INBOX",
        max
    );

    let client = reqwest::Client::new();
    let list_resp = client
        .get(&list_url)
        .bearer_auth(&tokens.access_token)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("Gmail list request failed: {e}"))?;

    if !list_resp.status().is_success() {
        let status = list_resp.status();
        let body = list_resp.text().await.unwrap_or_default();
        warn!(
            "[gmail] API returned {}: {}",
            status,
            &body[..body.len().min(200)]
        );
        return Ok(vec![]);
    }

    #[derive(Deserialize)]
    struct ListResponse {
        messages: Option<Vec<MessageRef>>,
    }
    #[derive(Deserialize)]
    struct MessageRef {
        id: String,
    }

    let list: ListResponse = list_resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse Gmail list: {e}"))?;

    let msg_refs = list.messages.unwrap_or_default();
    if msg_refs.is_empty() {
        return Ok(vec![]);
    }

    // ── 3. Fetch message metadata in parallel ────────────────────────
    let mut messages = Vec::new();

    let futures: Vec<_> = msg_refs
        .iter()
        .map(|m| {
            let url = format!(
                "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}?format=metadata&metadataHeaders=From&metadataHeaders=Subject&metadataHeaders=Date",
                m.id
            );
            client
                .get(&url)
                .bearer_auth(&tokens.access_token)
                .timeout(std::time::Duration::from_secs(10))
                .send()
        })
        .collect();

    let results = futures::future::join_all(futures).await;

    for result in results {
        let resp = match result {
            Ok(r) if r.status().is_success() => r,
            _ => continue,
        };

        #[derive(Deserialize)]
        struct GmailMsg {
            id: String,
            snippet: Option<String>,
            #[serde(rename = "labelIds")]
            label_ids: Option<Vec<String>>,
            payload: Option<Payload>,
        }
        #[derive(Deserialize, Clone)]
        struct Payload {
            headers: Option<Vec<Header>>,
        }
        #[derive(Deserialize, Clone)]
        struct Header {
            name: String,
            value: String,
        }

        if let Ok(msg) = resp.json::<GmailMsg>().await {
            let headers = msg
                .payload
                .as_ref()
                .and_then(|p| p.headers.as_ref())
                .cloned()
                .unwrap_or_default();

            let from = headers
                .iter()
                .find(|h| h.name.eq_ignore_ascii_case("From"))
                .map(|h| h.value.clone())
                .unwrap_or_else(|| "Unknown".to_string());

            let subject = headers
                .iter()
                .find(|h| h.name.eq_ignore_ascii_case("Subject"))
                .map(|h| h.value.clone())
                .unwrap_or_else(|| "(No subject)".to_string());

            let date = headers
                .iter()
                .find(|h| h.name.eq_ignore_ascii_case("Date"))
                .map(|h| h.value.clone())
                .unwrap_or_default();

            let read = msg
                .label_ids
                .as_ref()
                .map(|labels| !labels.contains(&"UNREAD".to_string()))
                .unwrap_or(true);

            messages.push(GmailMessage {
                id: msg.id,
                from,
                subject,
                snippet: msg.snippet.unwrap_or_default(),
                date,
                read,
            });
        }
    }

    info!("[gmail] Fetched {} message(s) from inbox", messages.len());
    Ok(messages)
}
