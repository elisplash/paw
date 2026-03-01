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

/// Check if the running n8n instance supports MCP (has /rest/mcp/api-key endpoint).
/// Returns false if n8n is an old version that predates MCP support.
///
/// Detection strategy: POST to `/rest/mcp/api-key` without auth.
///   - Old n8n: route doesn't exist → Express returns 404 HTML "Cannot POST"
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
    match client
        .post(&url)
        .header("Content-Type", "application/json")
        .body("{}")
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status().as_u16();
            if status == 404 {
                // Confirm it's the Express "Cannot POST" page, not a JSON 404
                let body = resp.text().await.unwrap_or_default();
                if body.contains("Cannot POST") {
                    log::debug!("[n8n] MCP not supported: /rest/mcp/api-key returns 'Cannot POST'");
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

// ── MCP access enablement ──────────────────────────────────────────────

/// Enable MCP access on the n8n instance.
///
/// MCP is disabled by default even after the owner is created.
/// This signs in as owner and calls `PATCH /rest/mcp/settings`
/// with `{ "mcpAccessEnabled": true }`. Idempotent — safe to call
/// multiple times.
pub async fn enable_mcp_access(base_url: &str) -> Result<(), String> {
    let base = base_url.trim_end_matches('/');

    let client = reqwest::Client::builder()
        .cookie_store(true)
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    // Sign in to get session
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
        .map_err(|e| format!("Login for MCP enable failed: {}", e))?;

    if !login_resp.status().is_success() {
        let status = login_resp.status();
        let body = login_resp.text().await.unwrap_or_default();
        return Err(format!(
            "Login for MCP enable failed (HTTP {}): {}",
            status,
            &body[..body.len().min(200)]
        ));
    }

    // Enable MCP access
    let settings_url = format!("{}/rest/mcp/settings", base);
    let settings_resp = client
        .patch(&settings_url)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({"mcpAccessEnabled": true}))
        .send()
        .await
        .map_err(|e| format!("MCP settings request failed: {}", e))?;

    match settings_resp.status().as_u16() {
        200 | 204 => {
            log::info!("[n8n] MCP access enabled");
            Ok(())
        }
        status => {
            let body = settings_resp.text().await.unwrap_or_default();
            Err(format!(
                "MCP enable failed (HTTP {}): {}",
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
///   2. POST /rest/mcp/api-key → create MCP API key (returns data.apiKey)
///
/// The MCP API key is a JWT with audience "mcp-server-api", separate from
/// N8N_API_KEY. It is required for Bearer auth on `/mcp-server/http`.
pub async fn retrieve_mcp_token(base_url: &str) -> Result<String, String> {
    let base = base_url.trim_end_matches('/');

    // Use a cookie-aware client for session management
    let client = reqwest::Client::builder()
        .cookie_store(true)
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    // Step 1: Sign in to get session cookie
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

    log::info!("[n8n] Login successful, creating MCP API key");

    // Step 2: Create MCP API key via POST /rest/mcp/api-key
    // Response: { "data": { "apiKey": "<jwt>", "audience": "mcp-server-api", ... } }
    let mcp_key_url = format!("{}/rest/mcp/api-key", base);
    let mcp_resp = client
        .post(&mcp_key_url)
        .send()
        .await
        .map_err(|e| format!("MCP API key request failed: {}", e))?;

    let mcp_status = mcp_resp.status();
    if !mcp_status.is_success() {
        let body = mcp_resp.text().await.unwrap_or_default();
        return Err(format!(
            "MCP API key creation failed (HTTP {}): {}",
            mcp_status,
            &body[..body.len().min(500)]
        ));
    }

    let mcp_data: serde_json::Value = mcp_resp
        .json()
        .await
        .map_err(|e| format!("Parse MCP API key response: {}", e))?;

    log::debug!(
        "[n8n] MCP API key response: {}",
        serde_json::to_string(&mcp_data).unwrap_or_default()
    );

    // Extract the JWT from data.apiKey
    if let Some(token) = mcp_data
        .get("data")
        .and_then(|d| d.get("apiKey"))
        .and_then(|t| t.as_str())
    {
        if !token.is_empty() {
            log::info!("[n8n] Retrieved MCP API key (audience: mcp-server-api)");
            return Ok(token.to_string());
        }
    }

    // Log the response shape for debugging if apiKey wasn't found
    let resp_str = serde_json::to_string(&mcp_data).unwrap_or_default();
    Err(format!(
        "MCP API key response missing data.apiKey: {}",
        &resp_str[..resp_str.len().min(300)]
    ))
}
