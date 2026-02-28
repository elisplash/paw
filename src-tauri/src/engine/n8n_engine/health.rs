// n8n_engine/health.rs — HTTP health probing and version detection
//
// Pure network probes with no container/process lifecycle side-effects.

use super::types::*;

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
const OWNER_PASSWORD: &str = "PawAgent2026!";

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

    let body = serde_json::json!({
        "email": OWNER_EMAIL,
        "firstName": "Paw",
        "lastName": "Agent",
        "password": OWNER_PASSWORD
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
                &body[..body.len().min(200)]
            ))
        }
    }
}

// ── MCP token retrieval ────────────────────────────────────────────────

/// Retrieve the MCP access token from n8n.
///
/// Flow:
///   1. Sign in as owner → get session cookie
///   2. Fetch MCP settings → get the access token
///
/// This token is separate from N8N_API_KEY and is required for
/// authenticating to `/mcp-server/http`.
pub async fn retrieve_mcp_token(base_url: &str) -> Result<String, String> {
    let base = base_url.trim_end_matches('/');

    // Use a cookie-aware client for session management
    let client = reqwest::Client::builder()
        .cookie_store(true)
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    // Step 1: Sign in to get session
    let login_url = format!("{}/rest/login", base);
    let login_body = serde_json::json!({
        "emailOrLdapLoginId": OWNER_EMAIL,
        "password": OWNER_PASSWORD
    });

    let login_resp = client
        .post(&login_url)
        .header("Content-Type", "application/json")
        .json(&login_body)
        .send()
        .await
        .map_err(|e| format!("Login request failed: {}", e))?;

    if !login_resp.status().is_success() {
        let status = login_resp.status();
        let body = login_resp.text().await.unwrap_or_default();
        return Err(format!(
            "Login failed (HTTP {}): {}",
            status,
            &body[..body.len().min(200)]
        ));
    }

    // Parse login response — may contain the MCP token directly,
    // or we may need to fetch it from settings
    let login_data: serde_json::Value = login_resp
        .json()
        .await
        .map_err(|e| format!("Parse login response: {}", e))?;

    log::debug!("[n8n] Login successful, fetching MCP settings");

    // Step 2: Try to get MCP settings (with session cookie from login)
    // n8n's internal API for MCP settings
    let mcp_settings_url = format!("{}/rest/mcp", base);
    let mcp_resp = client
        .get(&mcp_settings_url)
        .send()
        .await
        .map_err(|e| format!("MCP settings request failed: {}", e))?;

    if mcp_resp.status().is_success() {
        let mcp_data: serde_json::Value = mcp_resp
            .json()
            .await
            .map_err(|e| format!("Parse MCP settings: {}", e))?;

        // Look for access token / API key in the response
        if let Some(token) = mcp_data
            .get("data")
            .and_then(|d| d.get("accessToken"))
            .and_then(|t| t.as_str())
        {
            log::info!("[n8n] Retrieved MCP access token from settings");
            return Ok(token.to_string());
        }

        // Also check mcp_token, token, apiKey fields
        for field in &["accessToken", "mcp_token", "token", "apiKey"] {
            if let Some(token) = mcp_data.get(field).and_then(|t| t.as_str()) {
                log::info!("[n8n] Retrieved MCP token from field '{}'", field);
                return Ok(token.to_string());
            }
            // Check nested under "data"
            if let Some(token) = mcp_data
                .get("data")
                .and_then(|d| d.get(field))
                .and_then(|t| t.as_str())
            {
                log::info!("[n8n] Retrieved MCP token from data.{}", field);
                return Ok(token.to_string());
            }
        }

        log::debug!(
            "[n8n] MCP settings response (no token found): {}",
            &serde_json::to_string(&mcp_data).unwrap_or_default()[..200.min(
                serde_json::to_string(&mcp_data)
                    .unwrap_or_default()
                    .len()
            )]
        );
    } else {
        let status = mcp_resp.status();
        log::debug!("[n8n] MCP settings endpoint returned HTTP {}", status);
    }

    // Step 3: Fallback — try the user's settings/API page
    let settings_url = format!("{}/rest/mcp/api-key", base);
    let settings_resp = client.get(&settings_url).send().await;

    if let Ok(resp) = settings_resp {
        if resp.status().is_success() {
            if let Ok(data) = resp.json::<serde_json::Value>().await {
                for field in &["apiKey", "key", "token", "data"] {
                    if let Some(token) = data.get(field).and_then(|t| t.as_str()) {
                        if !token.is_empty() {
                            log::info!("[n8n] Retrieved MCP token from api-key endpoint");
                            return Ok(token.to_string());
                        }
                    }
                }
            }
        }
    }

    // Step 4: Try using the login response itself — some n8n versions
    // return an API key in the login response that works for MCP
    if let Some(token) = login_data
        .get("data")
        .and_then(|d| d.get("apiKey"))
        .and_then(|t| t.as_str())
    {
        if !token.is_empty() {
            log::info!("[n8n] Using apiKey from login response for MCP auth");
            return Ok(token.to_string());
        }
    }

    Err("Could not retrieve MCP access token from n8n settings".to_string())
}
