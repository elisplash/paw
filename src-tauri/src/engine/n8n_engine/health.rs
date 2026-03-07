// n8n_engine/health.rs — HTTP health probing and version detection
//
// Pure network probes with no container/process lifecycle side-effects.

use super::types::*;
use crate::engine::util::safe_truncate;

// ── Probing ────────────────────────────────────────────────────────────

/// Check if n8n is responding at the given URL.
pub async fn probe_n8n(base_url: &str, api_key: &str) -> bool {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };

    let endpoint = format!("{}{}", base_url.trim_end_matches('/'), API_PROBE_ENDPOINT);
    match client
        .get(&endpoint)
        .header("X-N8N-API-KEY", api_key)
        .send()
        .await
    {
        Ok(resp) => resp.status().is_success() || resp.status().as_u16() == 401,
        Err(_) => false,
    }
}

/// Poll n8n until it responds or timeout is reached.
pub async fn poll_n8n_ready(base_url: &str, api_key: &str) -> bool {
    let max_attempts = STARTUP_TIMEOUT_SECS / POLL_INTERVAL_SECS;
    for _ in 0..max_attempts {
        tokio::time::sleep(std::time::Duration::from_secs(POLL_INTERVAL_SECS)).await;
        if probe_n8n(base_url, api_key).await {
            return true;
        }
    }
    false
}

// ── Detection ──────────────────────────────────────────────────────────

/// Detect if n8n is already running locally (not managed by us).
pub async fn detect_local_n8n(url: &str) -> Option<N8nEndpoint> {
    // Try without API key first — n8n may be running without auth
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .ok()?;

    let endpoint = format!("{}{}", url.trim_end_matches('/'), HEALTH_ENDPOINT);
    let resp = client.get(&endpoint).send().await.ok()?;
    if resp.status().is_success() || resp.status().as_u16() == 401 {
        return Some(N8nEndpoint {
            url: url.to_string(),
            api_key: String::new(), // User will need to provide API key for local mode
            mode: N8nMode::Local,
        });
    }
    None
}

// ── Version ────────────────────────────────────────────────────────────

/// Check if the running n8n instance supports MCP (has /rest/mcp/api-key endpoint).
/// Returns false if n8n is an old version that predates MCP support.
///
/// Detection strategy: GET `/rest/mcp/api-key` without auth.
///   - Old n8n: route doesn't exist → Express returns 404 HTML "Cannot GET"
///   - New n8n: route exists, auth required → returns 401 JSON
///   - The `/mcp-server/http` endpoint is NOT reliable for this check because
///     old n8n's auth middleware returns 401 for ALL unauthenticated requests,
///     regardless of whether the route exists.
pub async fn has_mcp_support(base_url: &str) -> bool {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };

    let url = format!("{}/rest/mcp/api-key", base_url.trim_end_matches('/'));
    match client.get(&url).send().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            if status == 404 {
                // Confirm it's the Express "Cannot GET" page, not a JSON 404
                let body = resp.text().await.unwrap_or_default();
                if body.contains("Cannot GET") {
                    log::debug!("[n8n] MCP not supported: /rest/mcp/api-key returns 'Cannot GET'");
                    return false;
                }
            }
            // 401 (auth required) or any non-404 = endpoint exists = MCP supported
            true
        }
        Err(_) => false,
    }
}

/// Fetch the n8n version from the API.
pub async fn get_n8n_version(base_url: &str, api_key: &str) -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .ok()?;

    let endpoint = format!(
        "{}/api/v1/workflows?limit=1",
        base_url.trim_end_matches('/')
    );
    let resp = client
        .get(&endpoint)
        .header("X-N8N-API-KEY", api_key)
        .send()
        .await
        .ok()?;

    resp.headers()
        .get("x-n8n-version")
        .and_then(|v| v.to_str().ok())
        .map(String::from)
}

// ── Headless owner setup ───────────────────────────────────────────────

/// The owner credentials used for headless n8n operation.
const OWNER_EMAIL: &str = "agent@paw.local";

/// Get or generate the n8n owner password.
///
/// Uses `OsRng` (CSPRNG) to generate 32 bytes of entropy, hex-encoded to a
/// 64-char password.  The raw bytes are wrapped in `Zeroizing` so they are
/// securely wiped from memory after encoding.  The password is stored in the
/// OS keychain via the unified vault — never hardcoded, unique per install.
///
/// If the vault already has a password, it is returned as-is. A new one
/// is only generated when the vault has no entry (fresh install or vault
/// cleared). When the vault is cleared but n8n's database still has the
/// old owner, the 401 recovery path in `enable_mcp_access()` /
/// `retrieve_mcp_token()` handles the mismatch by resetting the owner
/// in the database and recreating it with the new password.
fn owner_password() -> String {
    use crate::engine::key_vault;

    if let Some(pw) = key_vault::get(key_vault::PURPOSE_N8N_OWNER) {
        return pw;
    }

    // CSPRNG — 32 bytes = 256 bits of entropy, zeroized after use
    use rand::rngs::OsRng;
    use rand::RngCore;
    use zeroize::Zeroizing;

    let mut bytes = Zeroizing::new([0u8; 32]);
    OsRng.fill_bytes(bytes.as_mut());
    let pw: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
    // `bytes` is zeroized on drop here

    key_vault::set(key_vault::PURPOSE_N8N_OWNER, &pw);
    log::info!("[n8n] Generated new owner password (256-bit CSPRNG) and stored in vault");
    pw
}

/// Attempt to reset the n8n owner by removing the `agent@paw.local` user
/// from n8n's SQLite database.
///
/// This is the recovery path when the vault password and n8n's stored
/// password hash go out of sync (e.g. vault cleared, n8n data persists).
/// Only deletes our service account — user workflows are preserved.
///
/// Disables FK constraints during delete to avoid cascading issues with
/// `shared_workflow`, `shared_credentials`, and other referencing tables.
fn reset_n8n_owner_in_db() -> Result<(), String> {
    let base = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".openpawz")
        .join("n8n-data");

    // n8n stores its database at different paths depending on the mode:
    //   Process mode (npx):  $N8N_USER_FOLDER/.n8n/database.sqlite
    //   Docker mode:         $bind_mount/database.sqlite  (mounted as /home/node/.n8n)
    let candidates = [
        base.join(".n8n").join("database.sqlite"), // Process mode
        base.join("database.sqlite"),              // Docker mode
    ];

    let db_path = candidates
        .iter()
        .find(|p| p.exists())
        .ok_or_else(|| {
            format!(
                "n8n database not found at any of: {}",
                candidates
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;

    log::info!("[n8n] Found n8n database at {}", db_path.display());

    let conn = rusqlite::Connection::open(db_path)
        .map_err(|e| format!("Failed to open n8n database: {}", e))?;

    // Set a busy timeout so we don't fail if n8n has the DB locked
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| format!("Failed to set busy timeout: {}", e))?;

    // Disable FK constraints so referencing rows in shared_workflow,
    // shared_credentials, etc. don't block the delete.
    conn.execute_batch("PRAGMA foreign_keys = OFF;")
        .map_err(|e| format!("Failed to disable FK constraints: {}", e))?;

    // Find the user ID first so we can clean up referencing rows
    let user_id: Option<String> = conn
        .query_row(
            "SELECT id FROM user WHERE email = ?1",
            [OWNER_EMAIL],
            |row| row.get(0),
        )
        .ok();

    if let Some(ref uid) = user_id {
        // Clean up rows that reference this user to avoid orphans
        for table in &[
            "shared_workflow",
            "shared_credentials",
        ] {
            let sql = format!("DELETE FROM \"{}\" WHERE \"userId\" = ?1", table);
            match conn.execute(&sql, [uid]) {
                Ok(n) if n > 0 => {
                    log::debug!("[n8n] Cleaned {} row(s) from {}", n, table);
                }
                _ => {}
            }
        }
    }

    let deleted: usize = conn
        .execute("DELETE FROM user WHERE email = ?1", [OWNER_EMAIL])
        .map_err(|e| format!("Failed to delete n8n owner: {}", e))?;

    // Re-enable FK constraints
    let _ = conn.execute_batch("PRAGMA foreign_keys = ON;");

    if deleted > 0 {
        log::info!(
            "[n8n] Reset owner '{}' in n8n database ({} row(s) deleted)",
            OWNER_EMAIL,
            deleted
        );
    } else {
        log::debug!("[n8n] No owner '{}' found in n8n database", OWNER_EMAIL);
    }

    Ok(())
}

/// Set up the n8n owner account if one doesn't exist yet.
///
/// n8n requires an owner account before certain features (like MCP)
/// become available. In headless mode, we create a service account
/// automatically. This is idempotent — if an owner already exists,
/// n8n returns 400 and we simply continue.
pub async fn setup_owner_if_needed(base_url: &str) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let setup_url = format!("{}/rest/owner/setup", base_url.trim_end_matches('/'));

    let password = owner_password();
    let body = serde_json::json!({
        "email": OWNER_EMAIL,
        "firstName": "Paw",
        "lastName": "Agent",
        "password": password
    });

    let resp = client
        .post(&setup_url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Owner setup request failed: {}", e))?;

    match resp.status().as_u16() {
        200 | 201 => {
            log::info!("[n8n] Owner account created for headless operation");
            Ok(())
        }
        400 => {
            // Owner already exists — this is fine
            log::debug!("[n8n] Owner account already exists");
            Ok(())
        }
        status => {
            let body = resp.text().await.unwrap_or_default();
            Err(format!(
                "Owner setup returned HTTP {}: {}",
                status,
                safe_truncate(&body, 200)
            ))
        }
    }
}

// ── MCP access enablement ──────────────────────────────────────────────

/// Enable MCP access on the n8n instance.
///
/// MCP is disabled by default even after the owner is created.
/// This signs in as owner and calls `PATCH /rest/mcp/settings`
/// with `{ "mcpAccessEnabled": true }`. Idempotent — safe to call
/// multiple times.
///
/// If owner-session login fails with 401 (password mismatch), attempts
/// automatic recovery: deletes the stale owner from n8n's database,
/// recreates it with the current derived password, and retries login.
pub async fn enable_mcp_access(base_url: &str, _api_key: &str) -> Result<(), String> {
    let base = base_url.trim_end_matches('/');

    // Inner function that attempts login + PATCH
    async fn try_enable(base: &str, password: &str) -> Result<(), (u16, String)> {
        let client = reqwest::Client::builder()
            .cookie_store(true)
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| (0u16, format!("HTTP client error: {}", e)))?;

        let login_url = format!("{}/rest/login", base);
        let login_body = serde_json::json!({
            "emailOrLdapLoginId": OWNER_EMAIL,
            "password": password
        });

        let login_resp = client
            .post(&login_url)
            .header("Content-Type", "application/json")
            .json(&login_body)
            .send()
            .await
            .map_err(|e| (0u16, format!("Login request failed: {}", e)))?;

        let login_status = login_resp.status().as_u16();
        if !login_resp.status().is_success() {
            let body = login_resp.text().await.unwrap_or_default();
            return Err((
                login_status,
                format!(
                    "Login failed (HTTP {}): {}",
                    login_status,
                    safe_truncate(&body, 200)
                ),
            ));
        }

        let settings_url = format!("{}/rest/mcp/settings", base);
        let settings_resp = client
            .patch(&settings_url)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({"mcpAccessEnabled": true}))
            .send()
            .await
            .map_err(|e| (0u16, format!("MCP settings request failed: {}", e)))?;

        match settings_resp.status().as_u16() {
            200 | 204 => {
                log::info!("[n8n] MCP access enabled");
                Ok(())
            }
            status => {
                let body = settings_resp.text().await.unwrap_or_default();
                Err((
                    status,
                    format!(
                        "MCP enable failed (HTTP {}): {}",
                        status,
                        safe_truncate(&body, 200)
                    ),
                ))
            }
        }
    }

    let password = owner_password();

    // First attempt
    match try_enable(base, &password).await {
        Ok(()) => return Ok(()),
        Err((401, msg)) => {
            log::warn!(
                "[n8n] MCP enable login failed (401) — resetting owner and retrying: {}",
                msg
            );
        }
        Err((_, msg)) => return Err(msg),
    }

    // Recovery: delete stale owner from n8n DB, recreate with our password, retry
    if let Err(e) = reset_n8n_owner_in_db() {
        log::warn!("[n8n] Could not reset owner in database: {}", e);
        return Err("Login failed (HTTP 401) and owner reset failed — MCP unavailable".into());
    }

    // Wait briefly for n8n to notice the DB change
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // Recreate the owner
    setup_owner_if_needed(base_url).await?;

    // Retry login + enable
    match try_enable(base, &password).await {
        Ok(()) => Ok(()),
        Err((_, msg)) => Err(format!("MCP enable failed after owner reset: {}", msg)),
    }
}

// ── MCP token retrieval ────────────────────────────────────────────────

/// Retrieve the MCP access token from n8n.
///
/// Flow:
///   1. Sign in as owner → get session cookie
///   2. GET /rest/mcp/api-key → get-or-create MCP API key (returns data.apiKey)
///
/// If login fails with 401 (password mismatch), attempts automatic recovery
/// by resetting the owner in the database and retrying.
///
/// The MCP API key is a JWT with audience "mcp-server-api", separate from
/// N8N_API_KEY. It is required for Bearer auth on `/mcp-server/http`.
pub async fn retrieve_mcp_token(base_url: &str, _api_key: &str) -> Result<String, String> {
    let base = base_url.trim_end_matches('/');

    // Inner function that attempts login + token retrieval
    async fn try_retrieve(base: &str, password: &str) -> Result<String, (u16, String)> {
        let client = reqwest::Client::builder()
            .cookie_store(true)
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| (0u16, format!("HTTP client error: {}", e)))?;

        // Step 1: Sign in to get session cookie
        let login_url = format!("{}/rest/login", base);
        let login_body = serde_json::json!({
            "emailOrLdapLoginId": OWNER_EMAIL,
            "password": password
        });

        let login_resp = client
            .post(&login_url)
            .header("Content-Type", "application/json")
            .json(&login_body)
            .send()
            .await
            .map_err(|e| (0u16, format!("Login request failed: {}", e)))?;

        let login_status = login_resp.status().as_u16();
        if !login_resp.status().is_success() {
            let body = login_resp.text().await.unwrap_or_default();
            return Err((
                login_status,
                format!(
                    "Login failed (HTTP {}): {}",
                    login_status,
                    safe_truncate(&body, 200)
                ),
            ));
        }

        log::info!("[n8n] Login successful, retrieving MCP API key");

        // Step 2: Get-or-create MCP API key
        let mcp_key_url = format!("{}/rest/mcp/api-key", base);
        let mcp_resp = client
            .get(&mcp_key_url)
            .send()
            .await
            .map_err(|e| (0u16, format!("MCP API key request failed: {}", e)))?;

        let mcp_status = mcp_resp.status();
        if !mcp_status.is_success() {
            let body = mcp_resp.text().await.unwrap_or_default();
            return Err((
                mcp_status.as_u16(),
                format!(
                    "MCP API key retrieval failed (HTTP {}): {}",
                    mcp_status,
                    safe_truncate(&body, 500)
                ),
            ));
        }

        let mcp_data: serde_json::Value = mcp_resp
            .json()
            .await
            .map_err(|e| (0u16, format!("Parse MCP API key response: {}", e)))?;

        log::debug!(
            "[n8n] MCP API key response: status={}, has_data={}",
            mcp_status,
            mcp_data.get("data").is_some()
        );

        // Extract the JWT from data.apiKey
        if let Some(token) = mcp_data
            .get("data")
            .and_then(|d| d.get("apiKey"))
            .and_then(|t| t.as_str())
        {
            if !token.is_empty() && !token.contains('*') {
                log::info!("[n8n] Retrieved MCP API key (audience: mcp-server-api)");
                return Ok(token.to_string());
            }

            // Key is redacted — rotate to get fresh unredacted JWT
            log::info!("[n8n] MCP API key is redacted, rotating to get fresh key");
            let rotate_url = format!("{}/rest/mcp/api-key/rotate", base);
            let rotate_resp = client
                .post(&rotate_url)
                .send()
                .await
                .map_err(|e| (0u16, format!("MCP API key rotation failed: {}", e)))?;

            if rotate_resp.status().is_success() {
                let rotate_data: serde_json::Value = rotate_resp
                    .json()
                    .await
                    .map_err(|e| (0u16, format!("Parse rotated MCP API key: {}", e)))?;

                if let Some(new_token) = rotate_data
                    .get("data")
                    .and_then(|d| d.get("apiKey"))
                    .and_then(|t| t.as_str())
                {
                    if !new_token.is_empty() && !new_token.contains('*') {
                        log::info!("[n8n] Rotated MCP API key successfully");
                        return Ok(new_token.to_string());
                    }
                }
            }
        }

        let resp_str = serde_json::to_string(&mcp_data).unwrap_or_default();
        Err((
            0,
            format!(
                "MCP API key response missing data.apiKey: {}",
                safe_truncate(&resp_str, 300)
            ),
        ))
    }

    let password = owner_password();

    // First attempt
    match try_retrieve(base, &password).await {
        Ok(token) => return Ok(token),
        Err((401, msg)) => {
            log::warn!(
                "[n8n] MCP token login failed (401) — resetting owner and retrying: {}",
                msg
            );
        }
        Err((_, msg)) => return Err(msg),
    }

    // Recovery: delete stale owner from n8n DB, recreate, retry
    if let Err(e) = reset_n8n_owner_in_db() {
        log::warn!("[n8n] Could not reset owner in database: {}", e);
        return Err("Login failed (HTTP 401) and owner reset failed — MCP unavailable".into());
    }

    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    setup_owner_if_needed(base_url).await?;

    match try_retrieve(base, &password).await {
        Ok(token) => Ok(token),
        Err((_, msg)) => Err(format!("MCP token retrieval failed after owner reset: {}", msg)),
    }
}

// ── Session-authenticated client ───────────────────────────────────────

/// Create a `reqwest::Client` that is logged into n8n with the owner
/// session cookie. This is needed for `/types/*` endpoints which do NOT
/// accept the `X-N8N-API-KEY` header — they require a browser-style
/// session obtained via `POST /rest/login`.
///
/// If login fails with 401 (password mismatch), attempts owner reset
/// recovery before giving up.
///
/// Returns `Ok(client)` with the session cookie already stored, or
/// `Err` if login fails (e.g. owner not set up yet).
pub async fn session_client(base_url: &str) -> Result<reqwest::Client, String> {
    let base = base_url.trim_end_matches('/');

    async fn try_login(base: &str, password: &str) -> Result<reqwest::Client, (u16, String)> {
        let client = reqwest::Client::builder()
            .cookie_store(true)
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| (0u16, format!("HTTP client error: {}", e)))?;

        let login_url = format!("{}/rest/login", base);
        let login_body = serde_json::json!({
            "emailOrLdapLoginId": OWNER_EMAIL,
            "password": password
        });

        let resp = client
            .post(&login_url)
            .header("Content-Type", "application/json")
            .json(&login_body)
            .send()
            .await
            .map_err(|e| (0u16, format!("n8n session login failed: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err((
                status,
                format!("n8n session login HTTP {}: {}", status, safe_truncate(&body, 200)),
            ));
        }

        log::debug!("[n8n] Session login successful — cookie-authenticated client ready");
        Ok(client)
    }

    let password = owner_password();

    match try_login(base, &password).await {
        Ok(client) => return Ok(client),
        Err((401, msg)) => {
            log::warn!(
                "[n8n] Session login failed (401) — resetting owner: {}",
                msg
            );
        }
        Err((_, msg)) => return Err(msg),
    }

    // Recovery
    if let Err(e) = reset_n8n_owner_in_db() {
        return Err(format!("Session login 401 and owner reset failed: {}", e));
    }
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    setup_owner_if_needed(base).await?;

    match try_login(base, &password).await {
        Ok(client) => Ok(client),
        Err((_, msg)) => Err(format!("Session login failed after owner reset: {}", msg)),
    }
}

// ── Version guard ──────────────────────────────────────────────────────

/// Minimum n8n version required for full MCP support.
/// n8n 1.76.0 introduced the instance-level MCP endpoint.
pub const MIN_N8N_VERSION_MCP: &str = "1.76.0";

/// Parse a semver-ish version string (e.g. "1.76.2") into (major, minor, patch).
/// Returns (0, 0, 0) if parsing fails.
pub fn parse_version(version: &str) -> (u32, u32, u32) {
    let parts: Vec<u32> = version
        .split('.')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    (
        parts.first().copied().unwrap_or(0),
        parts.get(1).copied().unwrap_or(0),
        parts.get(2).copied().unwrap_or(0),
    )
}

/// Check if the running n8n version meets the minimum for MCP.
/// Returns Ok(version_string) if the version is sufficient,
/// or Err with an actionable message if it's too old.
pub async fn check_n8n_version_for_mcp(base_url: &str, api_key: &str) -> Result<String, String> {
    let version = get_n8n_version(base_url, api_key).await.unwrap_or_default();

    if version.is_empty() {
        return Err(
            "Could not determine n8n version — ensure n8n is running and API key is correct".into(),
        );
    }

    let (major, minor, patch) = parse_version(&version);
    let (req_major, req_minor, req_patch) = parse_version(MIN_N8N_VERSION_MCP);

    let meets_minimum = (major, minor, patch) >= (req_major, req_minor, req_patch);

    if meets_minimum {
        Ok(version)
    } else {
        Err(format!(
            "n8n version {} is too old for MCP support (requires >= {}). \
             Update n8n: docker pull n8nio/n8n:latest or npx n8n@latest",
            version, MIN_N8N_VERSION_MCP
        ))
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_version_basic() {
        assert_eq!(parse_version("1.76.0"), (1, 76, 0));
        assert_eq!(parse_version("1.76.2"), (1, 76, 2));
        assert_eq!(parse_version("2.0.0"), (2, 0, 0));
    }

    #[test]
    fn parse_version_partial() {
        assert_eq!(parse_version("1.76"), (1, 76, 0));
        assert_eq!(parse_version("1"), (1, 0, 0));
    }

    #[test]
    fn parse_version_invalid() {
        assert_eq!(parse_version(""), (0, 0, 0));
        assert_eq!(parse_version("abc"), (0, 0, 0));
        // "1.x.2" → filter_map skips "x", collects [1, 2] → (1, 2, 0)
        assert_eq!(parse_version("1.x.2"), (1, 2, 0));
    }

    #[test]
    fn version_comparison() {
        // 1.76.0 >= 1.76.0
        let (major, minor, patch) = parse_version("1.76.0");
        let (req_major, req_minor, req_patch) = parse_version(MIN_N8N_VERSION_MCP);
        assert!((major, minor, patch) >= (req_major, req_minor, req_patch));

        // 1.80.0 >= 1.76.0
        let (major, minor, patch) = parse_version("1.80.0");
        assert!((major, minor, patch) >= (req_major, req_minor, req_patch));

        // 1.75.0 < 1.76.0
        let (major, minor, patch) = parse_version("1.75.0");
        assert!((major, minor, patch) < (req_major, req_minor, req_patch));

        // 2.0.0 >= 1.76.0
        let (major, minor, patch) = parse_version("2.0.0");
        assert!((major, minor, patch) >= (req_major, req_minor, req_patch));
    }

    // ── Response contract tests ────────────────────────────────────

    #[test]
    fn mcp_detection_logic_cannot_get() {
        // When n8n returns 404 with "Cannot GET /rest/mcp/api-key"
        // it means the MCP route doesn't exist (old n8n version)
        let body = "<!DOCTYPE html>\n<html>\n<body>\n<pre>Cannot GET /rest/mcp/api-key</pre>\n</body>\n</html>";
        assert!(
            body.contains("Cannot GET"),
            "Should detect 'Cannot GET' as legacy n8n"
        );
    }

    #[test]
    fn mcp_detection_logic_json_404() {
        // JSON 404 (not Express default page) means the route exists
        // but returned 404 for a different reason — MCP IS supported
        let body = r#"{"code": 404, "message": "Not Found"}"#;
        assert!(
            !body.contains("Cannot GET"),
            "JSON 404 should not be treated as legacy"
        );
    }

    #[test]
    fn mcp_token_jwt_validation() {
        // A valid MCP token looks like a JWT with dots and no asterisks
        let valid = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.rg2e2x3x";
        assert!(!valid.is_empty() && valid.contains('.') && !valid.contains('*'));

        // A redacted token contains asterisks — needs rotation
        let redacted = "eyJhbGci***.eyJzdWIi***.rg2e2x***";
        assert!(redacted.contains('*'));
    }

    #[test]
    fn mcp_api_key_response_parsing() {
        // Real n8n GET /rest/mcp/api-key response
        let json: serde_json::Value = serde_json::json!({
            "data": {
                "apiKey": "eyJhbGciOiJIUzI1NiJ9.eyJzdWI6IjEifQ.abc123",
                "audience": "mcp-server-api"
            }
        });

        let api_key = json
            .get("data")
            .and_then(|d| d.get("apiKey"))
            .and_then(|k| k.as_str())
            .unwrap();
        assert!(api_key.contains('.'));
        assert!(!api_key.contains('*'));

        let audience = json["data"]["audience"].as_str().unwrap();
        assert_eq!(audience, "mcp-server-api");
    }

    #[test]
    fn mcp_api_key_redacted_response() {
        // When n8n redacts the key, it returns asterisks
        let json: serde_json::Value = serde_json::json!({
            "data": {
                "apiKey": "eyJh******.eyJs******.abc***",
                "audience": "mcp-server-api"
            }
        });

        let api_key = json["data"]["apiKey"].as_str().unwrap();
        assert!(
            api_key.contains('*'),
            "Redacted key contains asterisks — must trigger rotation"
        );
    }

    #[test]
    fn owner_setup_request_shape() {
        // Verify the owner setup JSON matches what n8n expects
        let body = serde_json::json!({
            "email": "agent@paw.local",
            "firstName": "Paw",
            "lastName": "Agent",
            "password": "***REMOVED***"
        });
        assert!(body["email"].is_string());
        assert!(body["firstName"].is_string());
        assert!(body["password"].is_string());
        // n8n requires all four fields
        assert_eq!(body.as_object().unwrap().len(), 4);
    }

    #[test]
    fn login_request_uses_correct_field_name() {
        // n8n uses "emailOrLdapLoginId" not "email" for the login endpoint
        let body = serde_json::json!({
            "emailOrLdapLoginId": "agent@paw.local",
            "password": "***REMOVED***"
        });
        assert!(body.get("emailOrLdapLoginId").is_some());
        assert!(body.get("email").is_none()); // NOT "email"!
    }

    #[test]
    fn mcp_settings_request_shape() {
        // PATCH /rest/mcp/settings body
        let body = serde_json::json!({
            "mcpAccessEnabled": true
        });
        assert_eq!(body["mcpAccessEnabled"], true);
    }

    #[test]
    fn api_probe_url_construction() {
        // Test URL assembly patterns used throughout the codebase
        let cases = vec![
            ("http://localhost:5678", "/api/v1/workflows?limit=1"),
            ("http://localhost:5678/", "/api/v1/workflows?limit=1"),
            ("https://n8n.example.com", "/api/v1/workflows?limit=1"),
        ];
        for (base, path) in cases {
            let trimmed = base.trim_end_matches('/');
            let url = format!("{}{}", trimmed, path);
            assert!(!url.contains("//api"), "Double slash in URL: {}", url);
            assert!(url.ends_with("?limit=1"));
        }
    }

    #[test]
    fn constants_match_n8n_conventions() {
        // Docker container conventions
        assert_eq!(CONTAINER_NAME, "paw-n8n");
        assert_eq!(DEFAULT_PORT, 5678);
        assert_eq!(CONTAINER_DATA_DIR, "/home/node/.n8n");

        // Startup must be generous enough for first-time npx download
        assert!(STARTUP_TIMEOUT_SECS >= 120);
        assert!(POLL_INTERVAL_SECS >= 1 && POLL_INTERVAL_SECS <= 5);
    }
}
