// Pawz OAuth Commands — Tauri IPC wrappers for OAuth 2.0 PKCE flows.
//
// Thin command layer over engine::oauth. Handles vault storage,
// provisioning pipeline, and serialization.
//
// Supports the hybrid OAuth tier system:
//   Tier 1 — Shipped PKCE (engine_oauth_start)
//   Tier 2 — n8n delegation (engine_oauth_n8n_url)
//   Tier 3 — RFC 7591 dynamic registration (engine_oauth_rfc7591_start)
//   Tier 5 — Manual API keys (existing flow)

use crate::commands::n8n::{get_n8n_endpoint, map_integration_to_skill};
use crate::engine::key_vault;
use crate::engine::oauth::{
    get_n8n_oauth_type, get_oauth_config, get_rfc7591_config, n8n_credential_url,
    n8n_oauth_service_ids, oauth_service_ids, refresh_access_token, resolve_tier,
    rfc7591_service_ids, start_oauth_flow, start_rfc7591_flow, tier_label, OAuthResult, OAuthTier,
    OAuthTokens,
};
use crate::engine::skills::{
    self,
    crypto::{decrypt_credential, encrypt_credential, get_vault_key},
};
use crate::engine::state::EngineState;
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use tauri::Manager;

/// Check which services support OAuth (frontend uses this to show OAuth buttons).
/// Now includes tier information for the hybrid OAuth system.
#[tauri::command]
pub async fn engine_oauth_services() -> Result<Vec<OAuthServiceInfo>, String> {
    let mut services: Vec<OAuthServiceInfo> = Vec::new();

    // Tier 1: Shipped PKCE services
    for id in oauth_service_ids() {
        if let Some(config) = get_oauth_config(id) {
            services.push(OAuthServiceInfo {
                service_id: id.to_string(),
                name: config.name.to_string(),
                default_scopes: config
                    .default_scopes
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                write_scopes: config.write_scopes.iter().map(|s| s.to_string()).collect(),
                tier: OAuthTier::ShippedPkce,
                tier_label: tier_label(OAuthTier::ShippedPkce).to_string(),
            });
        }
    }

    // Tier 3: RFC 7591 dynamic registration services
    for id in rfc7591_service_ids() {
        if let Some(config) = get_rfc7591_config(id) {
            services.push(OAuthServiceInfo {
                service_id: id.to_string(),
                name: config.name.to_string(),
                default_scopes: config
                    .default_scopes
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                write_scopes: config.write_scopes.iter().map(|s| s.to_string()).collect(),
                tier: OAuthTier::DynamicRegistration,
                tier_label: tier_label(OAuthTier::DynamicRegistration).to_string(),
            });
        }
    }

    // Tier 2: n8n OAuth delegation services
    for id in n8n_oauth_service_ids() {
        // Skip if already covered by a higher tier
        if services.iter().any(|s| s.service_id == id) {
            continue;
        }
        if let Some(_cred_type) = get_n8n_oauth_type(id) {
            services.push(OAuthServiceInfo {
                service_id: id.to_string(),
                name: id.replace('-', " "),
                default_scopes: vec![],
                write_scopes: vec![],
                tier: OAuthTier::N8nDelegation,
                tier_label: tier_label(OAuthTier::N8nDelegation).to_string(),
            });
        }
    }

    Ok(services)
}

/// Start the OAuth PKCE flow for a service.
/// Opens the system browser, waits for authorization, exchanges tokens,
/// and stores them encrypted in the skill vault.
#[tauri::command]
pub async fn engine_oauth_start(
    service_id: String,
    app_handle: tauri::AppHandle,
) -> Result<OAuthResult, String> {
    info!("[oauth-cmd] Starting OAuth flow for '{}'", service_id);

    // Run the OAuth flow
    let tokens = match start_oauth_flow(&service_id, &app_handle).await {
        Ok(t) => t,
        Err(e) => {
            error!("[oauth-cmd] OAuth flow failed for '{}': {}", service_id, e);
            return Ok(OAuthResult {
                service_id,
                success: false,
                scopes_granted: vec![],
                error: Some(e.to_string()),
            });
        }
    };

    // Store tokens encrypted in the skill vault
    if let Err(e) = store_oauth_tokens(&service_id, &tokens) {
        error!(
            "[oauth-cmd] Failed to store tokens for '{}': {}",
            service_id, e
        );
        return Ok(OAuthResult {
            service_id,
            success: false,
            scopes_granted: vec![],
            error: Some(format!("Token storage failed: {}", e)),
        });
    }

    let scopes_granted = tokens
        .scope
        .as_deref()
        .unwrap_or("")
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();

    // ── Auto-provision: bridge OAuth tokens → skill vault ──
    // Build credential map from OAuth tokens and feed through the
    // integration→skill mapping so agent tools can use them immediately.
    provision_oauth_to_skill_vault(&service_id, &tokens.access_token, &app_handle);

    // ── Auto-provision: bridge OAuth tokens → n8n engine ──
    // Push the full tokens (including refresh_token) into the embedded
    // n8n engine as a credential, then deploy an MCP workflow so the
    // agent discovers the service's tools through the MCP bridge.
    // Runs in background so it doesn't delay the OAuth response.
    {
        let sid = service_id.clone();
        let tok = tokens.clone();
        let app = app_handle.clone();
        tauri::async_runtime::spawn(async move {
            provision_oauth_to_n8n(&sid, &tok, &app).await;
        });
    }

    info!(
        "[oauth-cmd] OAuth flow completed successfully for '{}'",
        service_id
    );

    Ok(OAuthResult {
        service_id,
        success: true,
        scopes_granted,
        error: None,
    })
}

/// Refresh an expired OAuth token for a service.
#[tauri::command]
pub async fn engine_oauth_refresh(service_id: String) -> Result<OAuthResult, String> {
    info!("[oauth-cmd] Refreshing token for '{}'", service_id);

    // Load existing tokens
    let existing = match load_oauth_tokens(&service_id) {
        Ok(Some(t)) => t,
        Ok(None) => {
            return Ok(OAuthResult {
                service_id,
                success: false,
                scopes_granted: vec![],
                error: Some("No OAuth tokens found — connect the service first".to_string()),
            });
        }
        Err(e) => {
            return Ok(OAuthResult {
                service_id,
                success: false,
                scopes_granted: vec![],
                error: Some(format!("Failed to load tokens: {}", e)),
            });
        }
    };

    let refresh_token = match &existing.refresh_token {
        Some(rt) => rt.clone(),
        None => {
            return Ok(OAuthResult {
                service_id,
                success: false,
                scopes_granted: vec![],
                error: Some("No refresh token available — reconnect the service".to_string()),
            });
        }
    };

    // Refresh
    let new_tokens = match refresh_access_token(&service_id, &refresh_token).await {
        Ok(t) => t,
        Err(e) => {
            warn!(
                "[oauth-cmd] Token refresh failed for '{}': {}",
                service_id, e
            );
            return Ok(OAuthResult {
                service_id,
                success: false,
                scopes_granted: vec![],
                error: Some(e.to_string()),
            });
        }
    };

    // Store updated tokens
    if let Err(e) = store_oauth_tokens(&service_id, &new_tokens) {
        return Ok(OAuthResult {
            service_id,
            success: false,
            scopes_granted: vec![],
            error: Some(format!("Failed to store refreshed tokens: {}", e)),
        });
    }

    let scopes_granted = new_tokens
        .scope
        .as_deref()
        .unwrap_or("")
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();

    info!("[oauth-cmd] Token refresh successful for '{}'", service_id);

    Ok(OAuthResult {
        service_id,
        success: true,
        scopes_granted,
        error: None,
    })
}

/// Check OAuth token status for a service.
#[tauri::command]
pub async fn engine_oauth_status(service_id: String) -> Result<OAuthTokenStatus, String> {
    let tokens = match load_oauth_tokens(&service_id) {
        Ok(Some(t)) => t,
        Ok(None) => {
            return Ok(OAuthTokenStatus {
                service_id,
                connected: false,
                expired: false,
                scopes: vec![],
                expires_at: None,
            });
        }
        Err(e) => return Err(format!("Failed to check OAuth status: {}", e)),
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let expired = tokens.expires_at.map(|exp| now >= exp).unwrap_or(false);

    let scopes = tokens
        .scope
        .as_deref()
        .unwrap_or("")
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();

    Ok(OAuthTokenStatus {
        service_id,
        connected: true,
        expired,
        scopes,
        expires_at: tokens.expires_at,
    })
}

/// Revoke and delete stored OAuth tokens for a service.
#[tauri::command]
pub async fn engine_oauth_revoke(service_id: String) -> Result<(), String> {
    info!("[oauth-cmd] Revoking OAuth tokens for '{}'", service_id);

    // Delete from unified vault
    let vault_purpose = format!("oauth:{}", service_id);
    key_vault::remove(&vault_purpose);
    info!("[oauth-cmd] Deleted OAuth tokens for '{}'", service_id);
    Ok(())
}

/// Resolve the OAuth tier for a service. The frontend uses this to
/// decide which connect button/flow to show.
#[tauri::command]
pub async fn engine_oauth_resolve_tier(service_id: String) -> Result<OAuthTierInfo, String> {
    let tier = resolve_tier(&service_id);
    Ok(OAuthTierInfo {
        service_id: service_id.clone(),
        tier,
        tier_label: tier_label(tier).to_string(),
        n8n_credential_type: get_n8n_oauth_type(&service_id).map(|s| s.to_string()),
    })
}

/// Get the n8n credential creation URL for a Tier 2 service.
/// Returns the URL the frontend should open (in an iframe or new tab).
#[tauri::command]
pub async fn engine_oauth_n8n_url(
    service_id: String,
    n8n_base_url: Option<String>,
) -> Result<String, String> {
    let cred_type = get_n8n_oauth_type(&service_id).ok_or_else(|| {
        format!(
            "Service '{}' does not have an n8n OAuth credential type",
            service_id
        )
    })?;

    let base_url = n8n_base_url.unwrap_or_else(|| "http://127.0.0.1:5678".to_string());
    let url = n8n_credential_url(&base_url, cred_type);

    info!(
        "[oauth-cmd] n8n credential URL for '{}': {}",
        service_id, url
    );
    Ok(url)
}

/// Start the RFC 7591 Dynamic Client Registration flow for a service.
/// Automatically registers a client_id and then runs the PKCE flow.
#[tauri::command]
pub async fn engine_oauth_rfc7591_start(
    service_id: String,
    domain: Option<String>,
    realm: Option<String>,
    app_handle: tauri::AppHandle,
) -> Result<OAuthResult, String> {
    info!("[oauth-cmd] Starting RFC 7591 flow for '{}'", service_id);

    let config = get_rfc7591_config(&service_id).ok_or_else(|| {
        format!(
            "Service '{}' does not support RFC 7591 Dynamic Client Registration",
            service_id
        )
    })?;

    // Substitute {domain} and {realm} in URLs
    let domain_val = domain.unwrap_or_default();
    let realm_val = realm.unwrap_or_else(|| "master".to_string());

    let reg_url = config
        .registration_url
        .replace("{domain}", &domain_val)
        .replace("{realm}", &realm_val);
    let auth_url = config
        .auth_url
        .replace("{domain}", &domain_val)
        .replace("{realm}", &realm_val);
    let token_url = config
        .token_url
        .replace("{domain}", &domain_val)
        .replace("{realm}", &realm_val);

    // Run the RFC 7591 flow
    let tokens = match start_rfc7591_flow(
        &service_id,
        &reg_url,
        &auth_url,
        &token_url,
        config.default_scopes,
        &app_handle,
    )
    .await
    {
        Ok(t) => t,
        Err(e) => {
            error!(
                "[oauth-cmd] RFC 7591 flow failed for '{}': {}",
                service_id, e
            );
            return Ok(OAuthResult {
                service_id,
                success: false,
                scopes_granted: vec![],
                error: Some(e.to_string()),
            });
        }
    };

    // Store tokens
    if let Err(e) = store_oauth_tokens(&service_id, &tokens) {
        error!(
            "[oauth-cmd] Failed to store RFC 7591 tokens for '{}': {}",
            service_id, e
        );
        return Ok(OAuthResult {
            service_id,
            success: false,
            scopes_granted: vec![],
            error: Some(format!("Token storage failed: {}", e)),
        });
    }

    let scopes_granted = tokens
        .scope
        .as_deref()
        .unwrap_or("")
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();

    // Auto-provision to skill vault
    provision_oauth_to_skill_vault(&service_id, &tokens.access_token, &app_handle);

    info!("[oauth-cmd] RFC 7591 flow completed for '{}'", service_id);

    Ok(OAuthResult {
        service_id,
        success: true,
        scopes_granted,
        error: None,
    })
}

/// Get the access token for a service (used internally by tool execution).
/// Automatically refreshes if expired.
pub async fn get_oauth_access_token(service_id: &str) -> Result<String, String> {
    let tokens = load_oauth_tokens(service_id)
        .map_err(|e| format!("Failed to load OAuth tokens: {}", e))?
        .ok_or_else(|| format!("No OAuth tokens for service '{}'", service_id))?;

    // Check if token needs refresh
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    if let Some(expires_at) = tokens.expires_at {
        // Refresh 5 minutes before actual expiry
        if now >= expires_at.saturating_sub(300) {
            if let Some(ref refresh_token) = tokens.refresh_token {
                info!("[oauth] Auto-refreshing expired token for '{}'", service_id);
                match refresh_access_token(service_id, refresh_token).await {
                    Ok(new_tokens) => {
                        let _ = store_oauth_tokens(service_id, &new_tokens);
                        return Ok(new_tokens.access_token);
                    }
                    Err(e) => {
                        warn!(
                            "[oauth] Auto-refresh failed for '{}': {} — using existing token",
                            service_id, e
                        );
                        // Fall through to return existing token
                    }
                }
            }
        }
    }

    Ok(tokens.access_token)
}

// ── Vault Storage ────────────────────────────────────────────────

/// Store OAuth tokens encrypted in the unified key vault.
fn store_oauth_tokens(service_id: &str, tokens: &OAuthTokens) -> Result<(), String> {
    let vault_key = get_vault_key().map_err(|e| format!("Vault key error: {}", e))?;
    let json = serde_json::to_string(tokens).map_err(|e| format!("Serialize error: {}", e))?;
    let encrypted = encrypt_credential(&json, &vault_key);

    let vault_purpose = format!("oauth:{}", service_id);
    key_vault::set(&vault_purpose, &encrypted);

    info!(
        "[oauth] Stored encrypted OAuth tokens for '{}' in unified vault",
        service_id
    );
    Ok(())
}

/// Load OAuth tokens from the unified key vault.
fn load_oauth_tokens(service_id: &str) -> Result<Option<OAuthTokens>, String> {
    let vault_key = get_vault_key().map_err(|e| format!("Vault key error: {}", e))?;

    let vault_purpose = format!("oauth:{}", service_id);

    match key_vault::get(&vault_purpose) {
        Some(encrypted) => {
            let json = decrypt_credential(&encrypted, &vault_key)
                .map_err(|e| format!("Decrypt error: {}", e))?;
            let tokens: OAuthTokens =
                serde_json::from_str(&json).map_err(|e| format!("Deserialize error: {}", e))?;
            Ok(Some(tokens))
        }
        None => Ok(None),
    }
}

// ── Frontend Types ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthServiceInfo {
    pub service_id: String,
    pub name: String,
    pub default_scopes: Vec<String>,
    pub write_scopes: Vec<String>,
    pub tier: OAuthTier,
    pub tier_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokenStatus {
    pub service_id: String,
    pub connected: bool,
    pub expired: bool,
    pub scopes: Vec<String>,
    pub expires_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTierInfo {
    pub service_id: String,
    pub tier: OAuthTier,
    pub tier_label: String,
    pub n8n_credential_type: Option<String>,
}

// ── OAuth → Skill Vault Provisioning ─────────────────────────────

/// Bridge an OAuth access token into the skill vault so agent tools
/// can use it immediately. Non-fatal: logs warnings but never fails
/// the OAuth flow itself.
fn provision_oauth_to_skill_vault(
    service_id: &str,
    access_token: &str,
    app_handle: &tauri::AppHandle,
) {
    // Build credential map matching what map_integration_to_skill expects.
    // OAuth services use `access_token` as the primary credential key;
    // the mapping function already checks for it alongside api_key/bot_token.
    let mut creds = std::collections::HashMap::new();
    creds.insert("access_token".to_string(), access_token.to_string());

    let (skill_id, mapped_creds) = map_integration_to_skill(service_id, &creds);

    if mapped_creds.is_empty() {
        info!(
            "[oauth-provision] No skill mapping for '{}' — skipping vault bridge",
            service_id
        );
        return;
    }

    // Get engine state + vault key
    let state = match app_handle.try_state::<EngineState>() {
        Some(s) => s,
        None => {
            warn!("[oauth-provision] Engine state unavailable — skipping vault bridge");
            return;
        }
    };

    let vault_key = match skills::crypto::get_vault_key() {
        Ok(k) => k,
        Err(e) => {
            warn!("[oauth-provision] Vault key error — skipping: {}", e);
            return;
        }
    };

    // Write each mapped credential to the skill vault (encrypted)
    let mut stored = 0;
    for (key, value) in &mapped_creds {
        let encrypted = skills::crypto::encrypt_credential(value, &vault_key);
        match state.store.set_skill_credential(&skill_id, key, &encrypted) {
            Ok(_) => stored += 1,
            Err(e) => {
                warn!(
                    "[oauth-provision] Failed to store {} for skill {}: {}",
                    key, skill_id, e
                );
            }
        }
    }

    // Auto-enable the skill
    if stored > 0 {
        if let Err(e) = state.store.set_skill_enabled(&skill_id, true) {
            warn!(
                "[oauth-provision] Failed to enable skill {}: {}",
                skill_id, e
            );
        } else {
            info!(
                "[oauth-provision] Bridged {} credentials for '{}' → skill '{}', skill enabled",
                stored, service_id, skill_id
            );
        }
    }
}

// ── OAuth → n8n Credential Provisioning ──────────────────────────
//
// After OAuth, push the tokens into the embedded n8n engine as a
// credential so n8n's nodes (Gmail, Drive, Calendar, Sheets, etc.)
// can use them. Then deploy an MCP workflow so the agent discovers
// the tools through the MCP bridge — same path as any n8n node.
//
// The user never sees or touches n8n. They click "Connect" → OAuth
// popup → done → agent has tools.

/// Map an OAuth service ID to the n8n node type and display name
/// for MCP workflow deployment.
fn oauth_service_to_n8n_node(service_id: &str) -> Option<(&'static str, &'static str)> {
    match service_id {
        "gmail" | "google" => Some(("n8n-nodes-base.gmail", "Gmail")),
        "google-drive" => Some(("n8n-nodes-base.googleDrive", "Google Drive")),
        "google-calendar" => Some(("n8n-nodes-base.googleCalendar", "Google Calendar")),
        "google-sheets" => Some(("n8n-nodes-base.googleSheets", "Google Sheets")),
        "google-docs" => Some(("n8n-nodes-base.googleDocs", "Google Docs")),
        "github" => Some(("n8n-nodes-base.github", "GitHub")),
        "slack" => Some(("n8n-nodes-base.slack", "Slack")),
        "discord" => Some(("n8n-nodes-base.discord", "Discord")),
        "notion" => Some(("n8n-nodes-base.notion", "Notion")),
        _ => None,
    }
}

/// Push OAuth tokens into the embedded n8n engine as a credential,
/// then deploy an MCP workflow so the agent can discover the tools.
///
/// Non-fatal: failures are logged but never block the OAuth flow.
/// Runs as a background task so it doesn't slow down the OAuth response.
async fn provision_oauth_to_n8n(
    service_id: &str,
    tokens: &OAuthTokens,
    app_handle: &tauri::AppHandle,
) {
    // Resolve the n8n credential type (e.g. "googleOAuth2Api")
    let n8n_cred_type = match get_n8n_oauth_type(service_id) {
        Some(t) => t,
        None => {
            info!(
                "[oauth→n8n] No n8n credential type for '{}' — skipping",
                service_id
            );
            return;
        }
    };

    // Get the OAuth config for client_id / client_secret
    // Google services all share the same config; other services
    // may not have shipped credentials but could still have n8n types.
    let (client_id, client_secret) = match get_oauth_config(service_id) {
        Some(config) => (
            config.effective_client_id().to_string(),
            config
                .effective_client_secret()
                .unwrap_or_default()
                .to_string(),
        ),
        None => {
            info!(
                "[oauth→n8n] No OAuth config for '{}' — skipping n8n bridge",
                service_id
            );
            return;
        }
    };

    // Get n8n endpoint
    let (base_url, api_key) = match get_n8n_endpoint(app_handle) {
        Ok(ep) => ep,
        Err(e) => {
            info!(
                "[oauth→n8n] n8n not available for '{}': {} — skipping",
                service_id, e
            );
            return;
        }
    };
    let base = base_url.trim_end_matches('/');

    // Build the n8n credential data structure.
    // n8n OAuth2 credentials expect: clientId, clientSecret, accessToken,
    // refreshToken, and an oauthTokenData object.
    let credential_data = serde_json::json!({
        "clientId": client_id,
        "clientSecret": client_secret,
        "accessToken": tokens.access_token,
        "refreshToken": tokens.refresh_token.as_deref().unwrap_or(""),
        "oauthTokenData": {
            "access_token": tokens.access_token,
            "refresh_token": tokens.refresh_token.as_deref().unwrap_or(""),
            "token_type": tokens.token_type,
            "scope": tokens.scope.as_deref().unwrap_or(""),
        }
    });

    let credential_name = format!("OpenPawz — {}", service_id);

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            warn!("[oauth→n8n] HTTP client error: {}", e);
            return;
        }
    };

    // Check if we already have a credential with this name (update vs create)
    let existing_id = find_n8n_credential(&client, base, &api_key, &credential_name).await;

    let cred_result = if let Some(cred_id) = existing_id {
        // Update existing credential with fresh tokens
        info!(
            "[oauth→n8n] Updating existing n8n credential {} for '{}'",
            cred_id, service_id
        );
        let payload = serde_json::json!({
            "name": credential_name,
            "type": n8n_cred_type,
            "data": credential_data,
        });
        client
            .patch(format!("{}/api/v1/credentials/{}", base, cred_id))
            .header("X-N8N-API-KEY", &api_key)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
    } else {
        // Create new credential
        info!(
            "[oauth→n8n] Creating n8n credential for '{}' (type: {})",
            service_id, n8n_cred_type
        );
        let payload = serde_json::json!({
            "name": credential_name,
            "type": n8n_cred_type,
            "data": credential_data,
        });
        client
            .post(format!("{}/api/v1/credentials", base))
            .header("X-N8N-API-KEY", &api_key)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
    };

    match cred_result {
        Ok(resp) if resp.status().is_success() => {
            info!(
                "[oauth→n8n] Credential '{}' provisioned in n8n",
                credential_name
            );
        }
        Ok(resp) => {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            warn!(
                "[oauth→n8n] Credential creation returned HTTP {}: {}",
                status, body
            );
            // Continue regardless — MCP workflow deploy might still work
            // if credential already existed from a previous session.
        }
        Err(e) => {
            warn!("[oauth→n8n] Credential request failed: {}", e);
            return;
        }
    }

    // Deploy MCP workflow for this service so the agent discovers the tools.
    // Uses the same function the integrations hub uses.
    if let Some((node_type, service_name)) = oauth_service_to_n8n_node(service_id) {
        match super::n8n::engine_n8n_deploy_mcp_workflow(
            app_handle.clone(),
            service_id.to_string(),
            service_name.to_string(),
            node_type.to_string(),
        )
        .await
        {
            Ok(wf_id) => {
                info!(
                    "[oauth→n8n] MCP workflow deployed for '{}' (id={})",
                    service_id, wf_id
                );
            }
            Err(e) => {
                warn!(
                    "[oauth→n8n] MCP workflow deploy failed for '{}': {}",
                    service_id, e
                );
            }
        }
    }
}

/// Find an existing n8n credential by name.
async fn find_n8n_credential(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    name: &str,
) -> Option<String> {
    let resp = client
        .get(format!("{}/api/v1/credentials", base_url))
        .header("X-N8N-API-KEY", api_key)
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let body: serde_json::Value = resp.json().await.ok()?;

    // n8n returns either { data: [...] } or just [...]
    let creds = body["data"].as_array().or_else(|| body.as_array())?;
    for cred in creds {
        if cred["name"].as_str() == Some(name) {
            // Handle numeric or string id
            if let Some(n) = cred["id"].as_u64() {
                return Some(n.to_string());
            }
            return cred["id"].as_str().map(|s| s.to_string());
        }
    }
    None
}

// ── Background Token Refresh ─────────────────────────────────────

/// Background task that periodically checks all stored OAuth tokens
/// and refreshes any that are close to expiry (<10 min remaining).
///
/// Designed to be spawned once at app startup via `tauri::async_runtime::spawn`.
/// Runs every 15 minutes, staggered 20 seconds after launch to avoid
/// competing with other startup tasks.
pub async fn oauth_token_refresh_loop(app_handle: tauri::AppHandle) {
    use crate::engine::oauth::oauth_service_ids;

    // Initial delay — let the app stabilize
    tokio::time::sleep(std::time::Duration::from_secs(20)).await;
    log::info!("[oauth-refresh] Background token refresh started (900s interval)");

    loop {
        let mut refreshed = 0u32;
        let mut failed = 0u32;
        let mut skipped = 0u32;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        for service_id in oauth_service_ids() {
            // Try to load tokens — skip if not stored
            let tokens = match load_oauth_tokens(service_id) {
                Ok(Some(t)) => t,
                Ok(None) => continue, // No tokens for this service
                Err(e) => {
                    log::debug!(
                        "[oauth-refresh] Failed to load tokens for '{}': {}",
                        service_id,
                        e
                    );
                    continue;
                }
            };

            // Check if refresh is needed (< 10 minutes to expiry)
            let needs_refresh = match tokens.expires_at {
                Some(exp) => now >= exp.saturating_sub(600), // 10 min buffer
                None => false,                               // No expiry → never-expiring token
            };

            if !needs_refresh {
                skipped += 1;
                continue;
            }

            // Need a refresh token to refresh
            let refresh_token = match &tokens.refresh_token {
                Some(rt) => rt.clone(),
                None => {
                    log::debug!(
                        "[oauth-refresh] Token for '{}' expiring but no refresh_token",
                        service_id
                    );
                    skipped += 1;
                    continue;
                }
            };

            // Attempt refresh
            match refresh_access_token(service_id, &refresh_token).await {
                Ok(new_tokens) => {
                    // Store refreshed tokens
                    if let Err(e) = store_oauth_tokens(service_id, &new_tokens) {
                        log::warn!(
                            "[oauth-refresh] Refreshed '{}' but storage failed: {}",
                            service_id,
                            e
                        );
                        failed += 1;
                        continue;
                    }

                    // Re-provision to skill vault with new access token
                    provision_oauth_to_skill_vault(
                        service_id,
                        &new_tokens.access_token,
                        &app_handle,
                    );

                    // Update n8n credential with refreshed tokens
                    // (no workflow deploy needed — already exists)
                    {
                        let sid = service_id.to_string();
                        let tok = new_tokens.clone();
                        let app = app_handle.clone();
                        tauri::async_runtime::spawn(async move {
                            provision_oauth_to_n8n(&sid, &tok, &app).await;
                        });
                    }

                    refreshed += 1;
                    log::info!(
                        "[oauth-refresh] Refreshed token for '{}' (expires in {}s)",
                        service_id,
                        new_tokens.expires_at.unwrap_or(0).saturating_sub(now)
                    );
                }
                Err(e) => {
                    log::warn!("[oauth-refresh] Failed to refresh '{}': {}", service_id, e);
                    failed += 1;
                }
            }
        }

        if refreshed > 0 || failed > 0 {
            log::info!(
                "[oauth-refresh] Cycle complete: {} refreshed, {} failed, {} ok",
                refreshed,
                failed,
                skipped
            );
        }

        // Sleep 15 minutes before next check
        tokio::time::sleep(std::time::Duration::from_secs(900)).await;
    }
}
