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
        "email": "paw@localhost",
        "firstName": "Paw",
        "lastName": "Agent",
        "password": "paw-headless-owner-do-not-use"
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
