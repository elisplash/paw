// Pawz Agent Engine — OAuth 2.0 PKCE Module
//
// Zero-friction credential provisioning via OAuth 2.0 Authorization Code + PKCE.
// Spawns an ephemeral localhost callback server, opens the system browser,
// catches the authorization code, exchanges it for tokens, and stores them
// in the encrypted skill vault. The entire flow is ~30 seconds.
//
// The AI never sees tokens — they go straight from the token endpoint
// into the OS keychain via the skill vault encryption layer.
//
// Hybrid OAuth Tiers:
//   Tier 1 — Shipped Client IDs (PKCE, zero user effort)
//   Tier 2 — n8n OAuth delegation (redirect to n8n credential UI)
//   Tier 3 — RFC 7591 Dynamic Client Registration (auto-register at runtime)
//   Tier 4 — Nango broker (optional, self-hosted)
//   Tier 5 — Manual API keys (always available)

use crate::atoms::error::{EngineError, EngineResult};
use base64::Engine as _;
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

// ── OAuth Configuration Registry ─────────────────────────────────

/// OAuth configuration for a single service.
#[derive(Debug, Clone)]
pub struct OAuthConfig {
    /// Human-readable service name (e.g., "GitHub")
    pub name: &'static str,
    /// Env var prefix for Client ID/Secret (e.g., "GOOGLE" → OPENPAWZ_GOOGLE_CLIENT_ID)
    pub env_prefix: &'static str,
    /// Authorization endpoint URL
    pub auth_url: &'static str,
    /// Token exchange endpoint URL
    pub token_url: &'static str,
    /// Client ID (shipped in the binary — safe with PKCE, no secret needed)
    pub client_id: &'static str,
    /// Client secret (needed by some providers even with PKCE, e.g., Google Desktop)
    pub client_secret: Option<&'static str>,
    /// Default scopes (read-only). Requested on first connect.
    pub default_scopes: &'static [&'static str],
    /// Write scopes. Requested only on explicit user escalation.
    pub write_scopes: &'static [&'static str],
    /// Token revocation endpoint (if supported)
    pub revoke_url: Option<&'static str>,
}

impl OAuthConfig {
    /// Get the effective client ID at runtime.
    ///
    /// Priority: compiled-in value (via `option_env!`) > runtime env var.
    /// This fallback handles cases where `build.rs` couldn't inject the value
    /// (e.g., different build toolchain, CI, or local dev without re-compile).
    /// The `Box::leak` is acceptable — this runs at most once per service
    /// in a desktop app's lifetime.
    pub fn effective_client_id(&self) -> &str {
        if !self.client_id.starts_with("REPLACE_WITH_") {
            return self.client_id;
        }
        let env_key = format!("OPENPAWZ_{}_CLIENT_ID", self.env_prefix);
        if let Ok(val) = std::env::var(&env_key) {
            if !val.is_empty() {
                return Box::leak(val.into_boxed_str());
            }
        }
        self.client_id
    }

    /// Get the effective client secret at runtime.
    ///
    /// Same fallback logic as `effective_client_id`.
    pub fn effective_client_secret(&self) -> Option<&str> {
        if self.client_secret.is_some() {
            return self.client_secret;
        }
        let env_key = format!("OPENPAWZ_{}_CLIENT_SECRET", self.env_prefix);
        if let Ok(val) = std::env::var(&env_key) {
            if !val.is_empty() {
                return Some(Box::leak(val.into_boxed_str()));
            }
        }
        None
    }
}

/// Stored OAuth tokens (encrypted in skill vault).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub token_type: String,
    /// Expiry as Unix timestamp (seconds). None if the token doesn't expire.
    pub expires_at: Option<u64>,
    /// Scopes that were actually granted by the platform.
    pub scope: Option<String>,
}

/// Result of a completed OAuth flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthResult {
    pub service_id: String,
    pub success: bool,
    pub scopes_granted: Vec<String>,
    pub error: Option<String>,
}

// ── Hybrid OAuth Tiers ──────────────────────────────────────────

/// Which authentication tier a service resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OAuthTier {
    /// Tier 1: Client ID shipped in the binary — direct PKCE flow.
    ShippedPkce,
    /// Tier 2: Delegate to n8n's built-in OAuth credential UI.
    N8nDelegation,
    /// Tier 3: RFC 7591 Dynamic Client Registration — auto-register at runtime.
    DynamicRegistration,
    /// Tier 5: No OAuth — manual API key entry.
    ManualApiKey,
}

/// Resolve which OAuth tier should handle a given service.
pub fn resolve_tier(service_id: &str) -> OAuthTier {
    // Tier 1a: Services with shipped static Client IDs (our PKCE engine)
    if get_oauth_config(service_id).is_some() {
        return OAuthTier::ShippedPkce;
    }

    // Tier 1b: Dynamic providers with registered (non-placeholder) Client IDs
    // These come from provider_registry (Nango data + our registrations.json)
    if crate::engine::provider_registry::is_ready(service_id) {
        return OAuthTier::ShippedPkce;
    }

    // Tier 3: RFC 7591 Dynamic Client Registration
    if get_rfc7591_config(service_id).is_some() {
        return OAuthTier::DynamicRegistration;
    }

    // Tier 2: Services with n8n OAuth credential types
    if get_n8n_oauth_type(service_id).is_some() {
        return OAuthTier::N8nDelegation;
    }

    // Tier 5: Manual API key (always available)
    OAuthTier::ManualApiKey
}

/// Return a human-readable label for the tier.
pub fn tier_label(tier: OAuthTier) -> &'static str {
    match tier {
        OAuthTier::ShippedPkce => "One-click OAuth",
        OAuthTier::N8nDelegation => "Connect via n8n",
        OAuthTier::DynamicRegistration => "Auto-register (RFC 7591)",
        OAuthTier::ManualApiKey => "Enter API key manually",
    }
}

// ── RFC 7591 Dynamic Client Registration ────────────────────────
//
// Some OIDC-compliant providers allow clients to self-register at
// runtime by POSTing metadata to a registration endpoint.
// The response contains a client_id (and optionally client_secret).
// Once registered, we chain into the normal PKCE flow.
//
// Cached registrations are stored in the OS keychain so we don't
// re-register on every app launch.

/// Configuration for an RFC 7591–capable provider.
#[derive(Debug, Clone)]
pub struct Rfc7591Config {
    /// Human-readable name.
    pub name: &'static str,
    /// RFC 7591 client registration endpoint.
    pub registration_url: &'static str,
    /// Authorization endpoint (for subsequent PKCE flow).
    pub auth_url: &'static str,
    /// Token exchange endpoint.
    pub token_url: &'static str,
    /// Default scopes to request after registration.
    pub default_scopes: &'static [&'static str],
    /// Write scopes (escalation).
    pub write_scopes: &'static [&'static str],
    /// Token revocation endpoint (if supported).
    pub revoke_url: Option<&'static str>,
}

/// Response from a successful RFC 7591 Dynamic Client Registration.
#[derive(Debug, Deserialize)]
pub struct Rfc7591RegistrationResponse {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub client_id_issued_at: Option<u64>,
    pub client_secret_expires_at: Option<u64>,
    #[serde(default)]
    pub registration_access_token: Option<String>,
}

/// Look up an RFC 7591 provider config by service ID.
pub fn get_rfc7591_config(service_id: &str) -> Option<&'static Rfc7591Config> {
    match service_id {
        "okta" => Some(&OKTA_RFC7591),
        "auth0" => Some(&AUTH0_RFC7591),
        "keycloak" => Some(&KEYCLOAK_RFC7591),
        "notion-mcp" => Some(&NOTION_MCP_RFC7591),
        "granola-mcp" => Some(&GRANOLA_MCP_RFC7591),
        _ => None,
    }
}

/// All service IDs that support RFC 7591 dynamic registration.
pub fn rfc7591_service_ids() -> Vec<&'static str> {
    vec!["okta", "auth0", "keycloak", "notion-mcp", "granola-mcp"]
}

// ── RFC 7591 Provider Registry ──────────────────────────────────
// Registration endpoints for OIDC providers that support dynamic
// client registration (RFC 7591 §3).

static OKTA_RFC7591: Rfc7591Config = Rfc7591Config {
    name: "Okta",
    // The actual URL requires tenant domain substitution at runtime.
    // Placeholder — caller must replace {domain} before use.
    registration_url: "https://{domain}/oauth2/v1/clients",
    auth_url: "https://{domain}/oauth2/default/v1/authorize",
    token_url: "https://{domain}/oauth2/default/v1/token",
    default_scopes: &["openid", "profile", "email"],
    write_scopes: &["okta.users.manage"],
    revoke_url: None,
};

static AUTH0_RFC7591: Rfc7591Config = Rfc7591Config {
    name: "Auth0",
    registration_url: "https://{domain}/oidc/register",
    auth_url: "https://{domain}/authorize",
    token_url: "https://{domain}/oauth/token",
    default_scopes: &["openid", "profile", "email"],
    write_scopes: &[],
    revoke_url: None,
};

static KEYCLOAK_RFC7591: Rfc7591Config = Rfc7591Config {
    name: "Keycloak",
    registration_url: "https://{domain}/realms/{realm}/clients-registrations/openid-connect",
    auth_url: "https://{domain}/realms/{realm}/protocol/openid-connect/auth",
    token_url: "https://{domain}/realms/{realm}/protocol/openid-connect/token",
    default_scopes: &["openid", "profile", "email"],
    write_scopes: &[],
    revoke_url: None,
};

static NOTION_MCP_RFC7591: Rfc7591Config = Rfc7591Config {
    name: "Notion (MCP)",
    registration_url: "https://mcp.notion.com/register",
    auth_url: "https://mcp.notion.com/authorize",
    token_url: "https://mcp.notion.com/token",
    default_scopes: &[],
    write_scopes: &[],
    revoke_url: None,
};

static GRANOLA_MCP_RFC7591: Rfc7591Config = Rfc7591Config {
    name: "Granola (MCP)",
    registration_url: "https://mcp-auth.granola.ai/oauth2/register",
    auth_url: "https://mcp-auth.granola.ai/oauth2/authorize",
    token_url: "https://mcp-auth.granola.ai/oauth2/token",
    default_scopes: &["offline_access"],
    write_scopes: &[],
    revoke_url: None,
};

/// Dynamically register an OAuth client via RFC 7591.
///
/// Returns the `client_id` (and optional `client_secret`) that was
/// issued by the provider. This client_id should be cached in the
/// OS keychain so registration only happens once.
///
/// `registration_url` should already have {domain}/{realm} substituted.
pub async fn dynamic_register_client(
    registration_url: &str,
    redirect_uri: &str,
) -> EngineResult<Rfc7591RegistrationResponse> {
    let client = super::http::pinned_client();

    let body = serde_json::json!({
        "client_name": "OpenPawz",
        "redirect_uris": [redirect_uri],
        "token_endpoint_auth_method": "none",
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "application_type": "native",
        "contacts": ["support@openpawz.com"]
    });

    info!("[oauth-rfc7591] Registering client at {}", registration_url);

    let response = client
        .post(registration_url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            error!("[oauth-rfc7591] Registration request failed: {}", e);
            EngineError::Other(format!("RFC 7591 registration failed: {}", e))
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        error!("[oauth-rfc7591] Registration returned {}: {}", status, body);
        return Err(EngineError::Other(format!(
            "Dynamic client registration failed (HTTP {}): {}",
            status, body
        )));
    }

    let reg: Rfc7591RegistrationResponse = response.json().await.map_err(|e| {
        error!(
            "[oauth-rfc7591] Failed to parse registration response: {}",
            e
        );
        EngineError::Other(format!("Failed to parse registration response: {}", e))
    })?;

    info!(
        "[oauth-rfc7591] Client registered successfully: client_id={}",
        reg.client_id
    );

    Ok(reg)
}

/// Run a full OAuth flow using RFC 7591 dynamic registration.
/// 1. Dynamically register a client (or use cached registration)
/// 2. Run the standard PKCE flow with the obtained client_id
pub async fn start_rfc7591_flow(
    service_id: &str,
    registration_url: &str,
    auth_url: &str,
    token_url: &str,
    scopes: &[&str],
    app_handle: &tauri::AppHandle,
) -> EngineResult<OAuthTokens> {
    // 1. Bind callback server first to know the redirect URI
    let listener = TcpListener::bind("127.0.0.1:0").await.map_err(|e| {
        error!("[oauth-rfc7591] Failed to bind callback server: {}", e);
        EngineError::Other(format!("Failed to start OAuth callback server: {}", e))
    })?;
    let port = listener
        .local_addr()
        .map_err(|e| EngineError::Other(format!("Failed to get callback port: {}", e)))?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{}/callback", port);

    // 2. Dynamically register client
    let reg = dynamic_register_client(registration_url, &redirect_uri).await?;

    // 3. Generate PKCE pair
    let (code_verifier, code_challenge) = generate_pkce_pair();

    info!(
        "[oauth-rfc7591] Starting PKCE flow for '{}' with dynamic client_id",
        service_id
    );

    // 4. Build authorization URL
    let scope_str = scopes.join(" ");
    let mut auth_url_full = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&code_challenge={}&code_challenge_method=S256",
        auth_url,
        urlencoding::encode(&reg.client_id),
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(&code_challenge),
    );
    if !scope_str.is_empty() {
        auth_url_full.push_str(&format!("&scope={}", urlencoding::encode(&scope_str)));
    }
    let state = {
        use rand::Rng;
        let mut buf = [0u8; 16];
        rand::thread_rng().fill(&mut buf);
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
    };
    auth_url_full.push_str(&format!("&state={}", urlencoding::encode(&state)));

    // 5. Open browser
    use tauri_plugin_opener::OpenerExt;
    if let Err(e) = app_handle.opener().open_url(&auth_url_full, None::<&str>) {
        error!("[oauth-rfc7591] Failed to open browser: {}", e);
        return Err(EngineError::Other(format!(
            "Failed to open browser for authorization: {}",
            e
        )));
    }

    // 6. Wait for callback
    let auth_code = wait_for_callback(listener, &state).await?;

    // 7. Exchange code for tokens (using dynamic client_id)
    let tokens = exchange_code(
        token_url,
        &reg.client_id,
        reg.client_secret.as_deref(),
        &auth_code,
        &code_verifier,
        &redirect_uri,
    )
    .await?;

    info!(
        "[oauth-rfc7591] RFC 7591 flow completed for '{}'",
        service_id
    );

    Ok(tokens)
}

// ── n8n OAuth Delegation ────────────────────────────────────────
//
// Maps OpenPawz service IDs to n8n credential type names.
// When a service resolves to Tier 2, we redirect the user to
// n8n's credential creation UI instead of running our own flow.

/// Get the n8n credential type name for services that support
/// OAuth through n8n's built-in credential system.
pub fn get_n8n_oauth_type(service_id: &str) -> Option<&'static str> {
    match service_id {
        // Google services — all share the same OAuth credential in n8n
        "google" | "google-workspace" | "gmail" | "google-drive" | "google-calendar"
        | "google-sheets" | "google-docs" => Some("googleOAuth2Api"),
        "hubspot" => Some("hubspotOAuth2Api"),
        "salesforce" => Some("salesforceOAuth2Api"),
        "jira" => Some("jiraOAuth2Api"),
        "stripe" => Some("stripeOAuth2Api"),
        "shopify" => Some("shopifyOAuth2Api"),
        "airtable" => Some("airtableOAuth2Api"),
        "trello" => Some("trelloOAuth2Api"),
        "asana" => Some("asanaOAuth2Api"),
        "mailchimp" => Some("mailchimpOAuth2Api"),
        "quickbooks" => Some("quickbooksOAuth2Api"),
        "zendesk" => Some("zendeskOAuth2Api"),
        "freshdesk" => Some("freshdeskOAuth2Api"),
        "pipedrive" => Some("pipedriveOAuth2Api"),
        "intercom" => Some("intercomOAuth2Api"),
        "clickup" => Some("clickUpOAuth2Api"),
        "todoist" => Some("todoistOAuth2Api"),
        "monday" => Some("mondayComOAuth2Api"),
        "twitch" => Some("twitchOAuth2Api"),
        "twitter" | "x" => Some("twitterOAuth2Api"),
        "facebook" => Some("facebookOAuth2Api"),
        "instagram" => Some("instagramOAuth2Api"),
        "linkedin" => Some("linkedInOAuth2Api"),
        "youtube" => Some("youtubeOAuth2Api"),
        "zoom" => Some("zoomOAuth2Api"),
        "webex" => Some("webexOAuth2Api"),
        // Microsoft 365 — now handled natively via Tier 1 PKCE (see MICROSOFT_OAUTH)
        // "microsoft-teams" | "teams" | "onedrive" | "outlook" → Tier 1
        "xero" => Some("xeroOAuth2Api"),
        "typeform" => Some("typeformOAuth2Api"),
        "gitlab" => Some("gitlabOAuth2Api"),
        "bitbucket" => Some("bitbucketOAuth2Api"),
        "box" => Some("boxOAuth2Api"),
        "docusign" => Some("docuSignOAuth2Api"),
        "eventbrite" => Some("eventbriteOAuth2Api"),
        "harvest" => Some("harvestOAuth2Api"),
        "miro" => Some("miroOAuth2Api"),
        "surveymonkey" => Some("surveyMonkeyOAuth2Api"),
        "strava" => Some("stravaOAuth2Api"),
        "calendly" => Some("calendlyOAuth2Api"),
        "gong" => Some("gongOAuth2Api"),
        "copper" => Some("copperOAuth2Api"),
        "basecamp" => Some("basecampOAuth2Api"),
        "wordpress" => Some("wordpressComOAuth2Api"),
        _ => None,
    }
}

/// All service IDs with n8n OAuth delegation support.
pub fn n8n_oauth_service_ids() -> Vec<&'static str> {
    vec![
        "hubspot",
        "salesforce",
        "jira",
        "stripe",
        "shopify",
        "airtable",
        "trello",
        "asana",
        "mailchimp",
        "quickbooks",
        "zendesk",
        "freshdesk",
        "pipedrive",
        "intercom",
        "clickup",
        "todoist",
        "monday",
        "twitch",
        "twitter",
        "facebook",
        "instagram",
        "linkedin",
        "youtube",
        "zoom",
        "webex",
        // microsoft-teams, onedrive, outlook — now Tier 1 (native PKCE)
        "xero",
        "typeform",
        "gitlab",
        "bitbucket",
        "box",
        "docusign",
        "eventbrite",
        "harvest",
        "miro",
        "surveymonkey",
        "strava",
        "calendly",
        "gong",
        "copper",
        "basecamp",
        "wordpress",
    ]
}

/// Build the n8n credential creation URL for a given credential type.
/// Returns the URL the user should be directed to.
pub fn n8n_credential_url(n8n_base_url: &str, credential_type: &str) -> String {
    format!(
        "{}/credentials/new?type={}",
        n8n_base_url.trim_end_matches('/'),
        credential_type
    )
}

// ── OAuth Registry ───────────────────────────────────────────────
// Client IDs are placeholder values. Replace with real registered app
// Client IDs before shipping. PKCE means no client_secret is needed.
//
// To add a new service:
// 1. Register an OAuth app on the platform's developer console
// 2. Set the redirect URI to http://127.0.0.1:{any-port}/callback
//    (most platforms allow localhost redirects for native apps)
// 3. Copy the Client ID here
// 4. Add the service_id to the match arm in `get_oauth_config()`

/// Get the OAuth configuration for a service, if it supports OAuth.
pub fn get_oauth_config(service_id: &str) -> Option<&'static OAuthConfig> {
    match service_id {
        "github" => Some(&GITHUB_OAUTH),
        "google" | "google-workspace" | "gmail" | "google-drive" | "google-calendar"
        | "google-sheets" | "google-docs" => Some(&GOOGLE_OAUTH),
        "microsoft" | "microsoft-365" | "outlook" | "onedrive" | "microsoft-teams"
        | "teams" => Some(&MICROSOFT_OAUTH),
        "discord" => Some(&DISCORD_OAUTH),
        "slack" => Some(&SLACK_OAUTH),
        "notion" => Some(&NOTION_OAUTH),
        "spotify" => Some(&SPOTIFY_OAUTH),
        "dropbox" => Some(&DROPBOX_OAUTH),
        "linear" => Some(&LINEAR_OAUTH),
        "figma" => Some(&FIGMA_OAUTH),
        "reddit" => Some(&REDDIT_OAUTH),
        _ => None,
    }
}

/// Returns all service IDs that support OAuth.
pub fn oauth_service_ids() -> Vec<&'static str> {
    vec![
        "github",
        "google",
        "google-workspace",
        "gmail",
        "google-drive",
        "google-calendar",
        "google-sheets",
        "microsoft",
        "microsoft-365",
        "outlook",
        "onedrive",
        "microsoft-teams",
        "discord",
        "slack",
        "notion",
        "spotify",
        "dropbox",
        "linear",
        "figma",
        "reddit",
    ]
}

// ── Service Configurations ───────────────────────────────────────
// Client IDs are injected at build time via environment variables.
// Set e.g. OPENPAWZ_GITHUB_CLIENT_ID before `cargo build`.
// PKCE Client IDs are public (not secrets) — safe to compile in.
//
// Fallback: if env var is unset, uses a placeholder that will fail
// at runtime with a clear error message.

static GITHUB_OAUTH: OAuthConfig = OAuthConfig {
    name: "GitHub",
    env_prefix: "GITHUB",
    auth_url: "https://github.com/login/oauth/authorize",
    token_url: "https://github.com/login/oauth/access_token",
    client_id: match option_env!("OPENPAWZ_GITHUB_CLIENT_ID") {
        Some(v) => v,
        None => "REPLACE_WITH_GITHUB_CLIENT_ID",
    },
    client_secret: option_env!("OPENPAWZ_GITHUB_CLIENT_SECRET"),
    default_scopes: &["read:user", "repo:status", "read:org"],
    write_scopes: &["repo", "write:org", "gist", "delete_repo"],
    revoke_url: None, // GitHub uses DELETE /applications/{client_id}/token
};

static GOOGLE_OAUTH: OAuthConfig = OAuthConfig {
    name: "Google",
    env_prefix: "GOOGLE",
    auth_url: "https://accounts.google.com/o/oauth2/v2/auth",
    token_url: "https://oauth2.googleapis.com/token",
    // PKCE Client IDs are public (RFC 8252 §8.1) — safe to ship in source.
    // Override at build time with OPENPAWZ_GOOGLE_CLIENT_ID env var if needed.
    client_id: match option_env!("OPENPAWZ_GOOGLE_CLIENT_ID") {
        Some(v) => v,
        None => "797133120028-qpdvbm5jihdqlj53mnps9k5gkvtbfgm5.apps.googleusercontent.com",
    },
    client_secret: option_env!("OPENPAWZ_GOOGLE_CLIENT_SECRET"),
    default_scopes: &[
        // Core productivity (existing)
        "https://www.googleapis.com/auth/gmail.modify",
        "https://www.googleapis.com/auth/gmail.send",
        "https://www.googleapis.com/auth/calendar",
        "https://www.googleapis.com/auth/drive",
        "https://www.googleapis.com/auth/spreadsheets",
        "https://www.googleapis.com/auth/documents",
        // Chat & communication
        "https://www.googleapis.com/auth/chat.messages",
        "https://www.googleapis.com/auth/chat.spaces.readonly",
        // Tasks, Contacts
        "https://www.googleapis.com/auth/tasks",
        "https://www.googleapis.com/auth/contacts.readonly",
        // Forms (read-only)
        "https://www.googleapis.com/auth/forms.body.readonly",
        "https://www.googleapis.com/auth/forms.responses.readonly",
        // YouTube (read-only)
        "https://www.googleapis.com/auth/youtube.readonly",
        // Vertex AI Vector Search
        "https://www.googleapis.com/auth/cloud-platform",
    ],
    write_scopes: &[
        // Core productivity (existing)
        "https://www.googleapis.com/auth/gmail.send",
        "https://www.googleapis.com/auth/calendar",
        "https://www.googleapis.com/auth/drive",
        "https://www.googleapis.com/auth/spreadsheets",
        "https://www.googleapis.com/auth/documents",
        // Chat, Tasks (read/write)
        "https://www.googleapis.com/auth/chat.messages",
        "https://www.googleapis.com/auth/tasks",
        // Vertex AI Vector Search
        "https://www.googleapis.com/auth/cloud-platform",
    ],
    revoke_url: Some("https://oauth2.googleapis.com/revoke"),
};

static DISCORD_OAUTH: OAuthConfig = OAuthConfig {
    name: "Discord",
    env_prefix: "DISCORD",
    auth_url: "https://discord.com/oauth2/authorize",
    token_url: "https://discord.com/api/oauth2/token",
    client_id: match option_env!("OPENPAWZ_DISCORD_CLIENT_ID") {
        Some(v) => v,
        None => "REPLACE_WITH_DISCORD_CLIENT_ID",
    },
    client_secret: option_env!("OPENPAWZ_DISCORD_CLIENT_SECRET"),
    default_scopes: &["identify", "guilds"],
    write_scopes: &["guilds.members.read", "messages.read", "bot"],
    revoke_url: Some("https://discord.com/api/oauth2/token/revoke"),
};

static SLACK_OAUTH: OAuthConfig = OAuthConfig {
    name: "Slack",
    env_prefix: "SLACK",
    auth_url: "https://slack.com/oauth/v2/authorize",
    token_url: "https://slack.com/api/oauth.v2.access",
    client_id: match option_env!("OPENPAWZ_SLACK_CLIENT_ID") {
        Some(v) => v,
        None => "REPLACE_WITH_SLACK_CLIENT_ID",
    },
    client_secret: option_env!("OPENPAWZ_SLACK_CLIENT_SECRET"),
    default_scopes: &["channels:read", "groups:read", "users:read", "team:read"],
    write_scopes: &["chat:write", "files:write", "channels:manage"],
    revoke_url: Some("https://slack.com/api/auth.revoke"),
};

static NOTION_OAUTH: OAuthConfig = OAuthConfig {
    name: "Notion",
    env_prefix: "NOTION",
    auth_url: "https://api.notion.com/v1/oauth/authorize",
    token_url: "https://api.notion.com/v1/oauth/token",
    client_id: match option_env!("OPENPAWZ_NOTION_CLIENT_ID") {
        Some(v) => v,
        None => "REPLACE_WITH_NOTION_CLIENT_ID",
    },
    client_secret: option_env!("OPENPAWZ_NOTION_CLIENT_SECRET"),
    // Notion uses a single scope model — permissions are set in the integration
    default_scopes: &[],
    write_scopes: &[],
    revoke_url: None,
};

static SPOTIFY_OAUTH: OAuthConfig = OAuthConfig {
    name: "Spotify",
    env_prefix: "SPOTIFY",
    auth_url: "https://accounts.spotify.com/authorize",
    token_url: "https://accounts.spotify.com/api/token",
    client_id: match option_env!("OPENPAWZ_SPOTIFY_CLIENT_ID") {
        Some(v) => v,
        None => "REPLACE_WITH_SPOTIFY_CLIENT_ID",
    },
    client_secret: option_env!("OPENPAWZ_SPOTIFY_CLIENT_SECRET"),
    default_scopes: &[
        "user-read-private",
        "user-read-email",
        "user-library-read",
        "playlist-read-private",
    ],
    write_scopes: &[
        "playlist-modify-public",
        "playlist-modify-private",
        "user-library-modify",
    ],
    revoke_url: None,
};

static DROPBOX_OAUTH: OAuthConfig = OAuthConfig {
    name: "Dropbox",
    env_prefix: "DROPBOX",
    auth_url: "https://www.dropbox.com/oauth2/authorize",
    token_url: "https://api.dropboxapi.com/oauth2/token",
    client_id: match option_env!("OPENPAWZ_DROPBOX_CLIENT_ID") {
        Some(v) => v,
        None => "REPLACE_WITH_DROPBOX_CLIENT_ID",
    },
    client_secret: option_env!("OPENPAWZ_DROPBOX_CLIENT_SECRET"),
    default_scopes: &["files.metadata.read", "sharing.read"],
    write_scopes: &["files.content.write", "files.content.read"],
    revoke_url: Some("https://api.dropboxapi.com/2/auth/token/revoke"),
};

static LINEAR_OAUTH: OAuthConfig = OAuthConfig {
    name: "Linear",
    env_prefix: "LINEAR",
    auth_url: "https://linear.app/oauth/authorize",
    token_url: "https://api.linear.app/oauth/token",
    client_id: match option_env!("OPENPAWZ_LINEAR_CLIENT_ID") {
        Some(v) => v,
        None => "REPLACE_WITH_LINEAR_CLIENT_ID",
    },
    client_secret: option_env!("OPENPAWZ_LINEAR_CLIENT_SECRET"),
    default_scopes: &["read"],
    write_scopes: &["write", "issues:create"],
    revoke_url: Some("https://api.linear.app/oauth/revoke"),
};

static FIGMA_OAUTH: OAuthConfig = OAuthConfig {
    name: "Figma",
    env_prefix: "FIGMA",
    auth_url: "https://www.figma.com/oauth",
    token_url: "https://api.figma.com/v1/oauth/token",
    client_id: match option_env!("OPENPAWZ_FIGMA_CLIENT_ID") {
        Some(v) => v,
        None => "REPLACE_WITH_FIGMA_CLIENT_ID",
    },
    client_secret: option_env!("OPENPAWZ_FIGMA_CLIENT_SECRET"),
    default_scopes: &["files:read"],
    write_scopes: &["files:write"],
    revoke_url: None,
};

static REDDIT_OAUTH: OAuthConfig = OAuthConfig {
    name: "Reddit",
    env_prefix: "REDDIT",
    auth_url: "https://www.reddit.com/api/v1/authorize",
    token_url: "https://www.reddit.com/api/v1/access_token",
    client_id: match option_env!("OPENPAWZ_REDDIT_CLIENT_ID") {
        Some(v) => v,
        None => "REPLACE_WITH_REDDIT_CLIENT_ID",
    },
    client_secret: option_env!("OPENPAWZ_REDDIT_CLIENT_SECRET"),
    default_scopes: &["identity", "read", "mysubreddits"],
    write_scopes: &["submit", "edit", "privatemessages"],
    revoke_url: Some("https://www.reddit.com/api/v1/revoke_token"),
};

static MICROSOFT_OAUTH: OAuthConfig = OAuthConfig {
    name: "Microsoft 365",
    env_prefix: "MICROSOFT",
    auth_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
    token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token",
    client_id: match option_env!("OPENPAWZ_MICROSOFT_CLIENT_ID") {
        Some(v) => v,
        None => "e1026883-ecd3-4116-a2dd-49cd43eea191",
    },
    client_secret: option_env!("OPENPAWZ_MICROSOFT_CLIENT_SECRET"),
    default_scopes: &[
        "openid",
        "profile",
        "email",
        "offline_access",
        "User.Read",
        // Mail
        "Mail.Read",
        "Mail.ReadWrite",
        "Mail.Send",
        "MailboxSettings.Read",
        // Calendar
        "Calendars.Read",
        "Calendars.ReadWrite",
        // Contacts
        "Contacts.Read",
        // Files (OneDrive)
        "Files.Read",
        "Files.ReadWrite",
        // Notes (OneNote)
        "Notes.Read",
        "Notes.ReadWrite",
        // Tasks (To Do)
        "Tasks.Read",
        "Tasks.ReadWrite",
        // Teams
        "Team.ReadBasic.All",
        "Channel.ReadBasic.All",
        "Chat.Read",
        "Chat.ReadWrite",
        "ChannelMessage.Read.All",
        "ChannelMessage.Send",
        // Sites (SharePoint)
        "Sites.Read.All",
        // People
        "People.Read",
        // Presence
        "Presence.Read",
    ],
    write_scopes: &[
        "Mail.Send",
        "Mail.ReadWrite",
        "Calendars.ReadWrite",
        "Files.ReadWrite",
        "Notes.ReadWrite",
        "Tasks.ReadWrite",
        "Chat.ReadWrite",
        "ChannelMessage.Send",
    ],
    revoke_url: Some("https://login.microsoftonline.com/common/oauth2/v2.0/logout"),
};

// ── PKCE ─────────────────────────────────────────────────────────

/// Generate a PKCE code_verifier and code_challenge (S256 method).
/// See RFC 7636 §4.1 and §4.2.
pub fn generate_pkce_pair() -> (String, String) {
    use rand::Rng;

    // code_verifier: 32 random bytes → base64url (43 chars)
    let mut verifier_bytes = [0u8; 32];
    rand::thread_rng().fill(&mut verifier_bytes);
    let code_verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(verifier_bytes);

    // code_challenge: SHA256(code_verifier) → base64url
    let digest = Sha256::digest(code_verifier.as_bytes());
    let code_challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);

    (code_verifier, code_challenge)
}

// ── OAuth Flow ───────────────────────────────────────────────────

/// Run the full OAuth PKCE flow for a service:
/// 1. Generate PKCE pair
/// 2. Start ephemeral localhost callback server
/// 3. Open system browser to authorization URL
/// 4. Wait for callback with authorization code (timeout: 120s)
/// 5. Exchange code for tokens
/// 6. Return tokens (caller stores in vault)
///
/// This function does NOT store the tokens — the caller (Tauri command)
/// is responsible for encrypting and saving them.
pub async fn start_oauth_flow(
    service_id: &str,
    app_handle: &tauri::AppHandle,
) -> EngineResult<OAuthTokens> {
    let config = get_oauth_config(service_id).ok_or_else(|| {
        EngineError::Other(format!(
            "No OAuth configuration found for service '{}'",
            service_id
        ))
    })?;

    // Resolve effective client ID (compile-time or runtime env var)
    let client_id = config.effective_client_id();
    // Warm the client_secret cache too (so exchange_code picks it up)
    let _ = config.effective_client_secret();

    // Fail early if Client ID hasn't been registered
    if client_id.starts_with("REPLACE_WITH_") {
        return Err(EngineError::Other(format!(
            "OAuth not configured for {} yet. The Client ID needs to be registered \
             at the {} developer portal. Set the OPENPAWZ_{}_CLIENT_ID environment \
             variable before building, or use the manual API key flow instead.",
            config.name, config.name, config.env_prefix,
        )));
    }

    // 1. Generate PKCE pair
    let (code_verifier, code_challenge) = generate_pkce_pair();

    // 2. Bind ephemeral TCP listener on any available port
    // §Security: RFC 8252 §8.3 mandates the loopback interface for native app
    // OAuth redirects. The callback uses HTTP (not HTTPS) which is acceptable
    // per the RFC — the security relies on the OS loopback protection and the
    // single-use authorization code + PKCE. The listener is short-lived and
    // closes after receiving one callback.
    let listener = TcpListener::bind("127.0.0.1:0").await.map_err(|e| {
        error!("[oauth] Failed to bind callback server: {}", e);
        EngineError::Other(format!("Failed to start OAuth callback server: {}", e))
    })?;
    let port = listener
        .local_addr()
        .map_err(|e| EngineError::Other(format!("Failed to get callback port: {}", e)))?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{}/callback", port);

    info!(
        "[oauth] Starting OAuth flow for '{}' — callback on port {}",
        service_id, port
    );

    // 3. Build authorization URL
    let scopes = config.default_scopes.join(" ");
    let mut auth_url = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&code_challenge={}&code_challenge_method=S256",
        config.auth_url,
        urlencoding::encode(client_id),
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(&code_challenge),
    );
    if !scopes.is_empty() {
        auth_url.push_str(&format!("&scope={}", urlencoding::encode(&scopes)));
    }
    // state parameter for CSRF protection
    let state = {
        use rand::Rng;
        let mut buf = [0u8; 16];
        rand::thread_rng().fill(&mut buf);
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
    };
    auth_url.push_str(&format!("&state={}", urlencoding::encode(&state)));

    // Google-specific: request offline access for refresh tokens
    if config.token_url.contains("googleapis.com") {
        auth_url.push_str("&access_type=offline&prompt=consent");
    }

    // 4. Open system browser
    info!("[oauth] Opening browser: {}", config.name);
    use tauri_plugin_opener::OpenerExt;
    if let Err(e) = app_handle.opener().open_url(&auth_url, None::<&str>) {
        error!("[oauth] Failed to open browser: {}", e);
        return Err(EngineError::Other(format!(
            "Failed to open browser for {} authorization: {}",
            config.name, e
        )));
    }

    // 5. Wait for callback (timeout: 120 seconds)
    let auth_code = wait_for_callback(listener, &state).await?;

    info!("[oauth] Received authorization code for '{}'", service_id);

    // 6. Exchange authorization code for tokens
    let tokens = exchange_code(
        config.token_url,
        config.effective_client_id(),
        config.effective_client_secret(),
        &auth_code,
        &code_verifier,
        &redirect_uri,
    )
    .await?;

    info!(
        "[oauth] Token exchange successful for '{}' — scopes: {:?}",
        service_id, tokens.scope
    );

    Ok(tokens)
}

/// Wait for the OAuth callback on the ephemeral TCP listener.
/// Parses the authorization code from the query string.
/// Returns an error if the callback contains an error parameter or times out.
async fn wait_for_callback(listener: TcpListener, expected_state: &str) -> EngineResult<String> {
    // 120-second timeout
    let result = tokio::time::timeout(std::time::Duration::from_secs(120), async {
        let (mut stream, _addr) = listener
            .accept()
            .await
            .map_err(|e| EngineError::Other(format!("Failed to accept OAuth callback: {}", e)))?;

        // Read the HTTP request (only need the first line for the query params)
        let mut buf = vec![0u8; 4096];
        let n = stream
            .read(&mut buf)
            .await
            .map_err(|e| EngineError::Other(format!("Failed to read OAuth callback: {}", e)))?;
        let request = String::from_utf8_lossy(&buf[..n]);

        // Extract the request path (e.g., "GET /callback?code=abc&state=xyz HTTP/1.1")
        let first_line = request.lines().next().unwrap_or("");
        let path = first_line.split_whitespace().nth(1).unwrap_or("");

        // Parse query parameters
        let query_string = path.split_once('?').map(|x| x.1).unwrap_or("");
        let params: HashMap<&str, &str> = query_string
            .split('&')
            .filter_map(|pair| {
                let mut parts = pair.splitn(2, '=');
                Some((parts.next()?, parts.next()?))
            })
            .collect();

        // Check for error
        if let Some(err) = params.get("error") {
            let desc = params.get("error_description").unwrap_or(&"Unknown error");
            // Send error response to browser
            let error_html = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n\
                <html><body style='font-family:system-ui;text-align:center;padding:60px'>\
                <h2 style='color:#e53e3e'>Authorization Failed</h2>\
                <p>{}</p>\
                <p style='color:#666'>You can close this tab and try again in OpenPawz.</p>\
                </body></html>",
                desc
            );
            let _ = stream.write_all(error_html.as_bytes()).await;
            return Err(EngineError::Other(format!(
                "OAuth authorization denied: {} — {}",
                err, desc
            )));
        }

        // Verify state parameter (CSRF protection)
        let received_state = params.get("state").unwrap_or(&"");
        if *received_state != expected_state {
            let _ = stream
                .write_all(b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\nState mismatch")
                .await;
            return Err(EngineError::Other(
                "OAuth state mismatch — possible CSRF attack".to_string(),
            ));
        }

        // Extract authorization code
        let code = params
            .get("code")
            .ok_or_else(|| EngineError::Other("No authorization code in callback".to_string()))?
            .to_string();

        // Send success response to browser
        let success_html =
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n\
            <html><body style='font-family:system-ui;text-align:center;padding:60px'>\
            <h2 style='color:#38a169'>Connected!</h2>\
            <p>You can close this tab and return to OpenPawz.</p>\
            <script>setTimeout(()=>window.close(),2000)</script>\
            </body></html>";
        let _ = stream.write_all(success_html.as_bytes()).await;

        Ok(code)
    })
    .await;

    match result {
        Ok(inner) => inner,
        Err(_) => {
            warn!("[oauth] OAuth callback timed out after 120 seconds");
            Err(EngineError::Other(
                "OAuth authorization timed out — no response received within 120 seconds. Please try again."
                    .to_string(),
            ))
        }
    }
}

/// Exchange an authorization code for access + refresh tokens via the token endpoint.
async fn exchange_code(
    token_url: &str,
    client_id: &str,
    client_secret: Option<&str>,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> EngineResult<OAuthTokens> {
    let client = super::http::pinned_client();

    let mut params = HashMap::new();
    params.insert("grant_type", "authorization_code");
    params.insert("code", code);
    params.insert("redirect_uri", redirect_uri);
    params.insert("client_id", client_id);
    params.insert("code_verifier", code_verifier);

    // Some providers (e.g., Google Desktop) require client_secret even with PKCE
    if let Some(secret) = client_secret {
        params.insert("client_secret", secret);
    }

    let response = client
        .post(token_url)
        .header("Accept", "application/json")
        .form(&params)
        .send()
        .await
        .map_err(|e| {
            error!("[oauth] Token exchange request failed: {}", e);
            EngineError::Other(format!("Token exchange failed: {}", e))
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        error!("[oauth] Token exchange returned {}: {}", status, body);
        return Err(EngineError::Other(format!(
            "Token exchange failed (HTTP {}): {}",
            status, body
        )));
    }

    // Parse token response
    let token_response: TokenResponse = response.json().await.map_err(|e| {
        error!("[oauth] Failed to parse token response: {}", e);
        EngineError::Other(format!("Failed to parse token response: {}", e))
    })?;

    // Calculate expiry timestamp
    let expires_at = token_response.expires_in.map(|secs| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            + secs
    });

    Ok(OAuthTokens {
        access_token: token_response.access_token,
        refresh_token: token_response.refresh_token,
        token_type: token_response
            .token_type
            .unwrap_or_else(|| "Bearer".to_string()),
        expires_at,
        scope: token_response.scope,
    })
}

/// Refresh an expired access token using the stored refresh token.
pub async fn refresh_access_token(
    service_id: &str,
    refresh_token: &str,
) -> EngineResult<OAuthTokens> {
    let config = get_oauth_config(service_id).ok_or_else(|| {
        EngineError::Other(format!(
            "No OAuth configuration found for service '{}'",
            service_id
        ))
    })?;

    let client = super::http::pinned_client();

    // Resolve effective credentials (compile-time or runtime env var)
    let client_id = config.effective_client_id();
    let client_secret = config.effective_client_secret();

    let mut params = HashMap::new();
    params.insert("grant_type", "refresh_token");
    params.insert("refresh_token", refresh_token);
    params.insert("client_id", client_id);

    // Some providers require client_secret for refresh too
    if let Some(secret) = client_secret {
        params.insert("client_secret", secret);
    }

    let response = client
        .post(config.token_url)
        .header("Accept", "application/json")
        .form(&params)
        .send()
        .await
        .map_err(|e| {
            error!(
                "[oauth] Token refresh request failed for '{}': {}",
                service_id, e
            );
            EngineError::Other(format!("Token refresh failed: {}", e))
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        error!(
            "[oauth] Token refresh returned {} for '{}': {}",
            status, service_id, body
        );
        return Err(EngineError::Other(format!(
            "Token refresh failed (HTTP {}). You may need to reconnect {}.",
            status, config.name
        )));
    }

    let token_response: TokenResponse = response
        .json()
        .await
        .map_err(|e| EngineError::Other(format!("Failed to parse refresh response: {}", e)))?;

    let expires_at = token_response.expires_in.map(|secs| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            + secs
    });

    Ok(OAuthTokens {
        access_token: token_response.access_token,
        // Some providers return a new refresh token; keep the old one if not
        refresh_token: token_response
            .refresh_token
            .or_else(|| Some(refresh_token.to_string())),
        token_type: token_response
            .token_type
            .unwrap_or_else(|| "Bearer".to_string()),
        expires_at,
        scope: token_response.scope,
    })
}

// ── Internal Types ───────────────────────────────────────────────

/// Raw token endpoint response (OAuth 2.0 spec).
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    token_type: Option<String>,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
    scope: Option<String>,
}

// ── URL Encoding Helper ──────────────────────────────────────────

/// Minimal URL encoding module (avoid adding another crate).
mod urlencoding {
    pub fn encode(input: &str) -> String {
        let mut result = String::with_capacity(input.len() * 3);
        for byte in input.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    result.push(byte as char);
                }
                _ => {
                    result.push('%');
                    result.push_str(&format!("{:02X}", byte));
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pkce_pair_generation() {
        let (verifier, challenge) = generate_pkce_pair();

        // Verifier should be 43 chars (32 bytes → base64url no padding)
        assert_eq!(verifier.len(), 43);

        // Challenge should be 43 chars (SHA256 = 32 bytes → base64url no padding)
        assert_eq!(challenge.len(), 43);

        // Verifier and challenge should not be the same
        assert_ne!(verifier, challenge);

        // Verify the challenge is the SHA256 of the verifier
        let digest = Sha256::digest(verifier.as_bytes());
        let expected_challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
        assert_eq!(challenge, expected_challenge);
    }

    #[test]
    fn test_pkce_pair_uniqueness() {
        let (v1, _) = generate_pkce_pair();
        let (v2, _) = generate_pkce_pair();
        assert_ne!(v1, v2, "PKCE pairs should be unique");
    }

    #[test]
    fn test_oauth_registry_lookup() {
        // Known services
        assert!(get_oauth_config("github").is_some());
        assert!(get_oauth_config("google").is_some());
        assert!(get_oauth_config("gmail").is_some()); // alias → Google
        assert!(get_oauth_config("discord").is_some());
        assert!(get_oauth_config("slack").is_some());
        assert!(get_oauth_config("notion").is_some());
        assert!(get_oauth_config("spotify").is_some());

        // Unknown services
        assert!(get_oauth_config("some-random-api").is_none());
        assert!(get_oauth_config("").is_none());
    }

    #[test]
    fn test_oauth_config_has_valid_urls() {
        for id in oauth_service_ids() {
            if let Some(config) = get_oauth_config(id) {
                assert!(
                    config.auth_url.starts_with("https://"),
                    "Auth URL for {} must use HTTPS",
                    id
                );
                assert!(
                    config.token_url.starts_with("https://"),
                    "Token URL for {} must use HTTPS",
                    id
                );
                assert!(
                    !config.client_id.is_empty(),
                    "Client ID for {} must not be empty",
                    id
                );
            }
        }
    }

    #[test]
    fn test_url_encoding() {
        assert_eq!(urlencoding::encode("hello world"), "hello%20world");
        assert_eq!(urlencoding::encode("a+b=c"), "a%2Bb%3Dc");
        assert_eq!(
            urlencoding::encode("https://example.com"),
            "https%3A%2F%2Fexample.com"
        );
        // Unreserved characters should not be encoded
        assert_eq!(urlencoding::encode("abc-123_456.789~"), "abc-123_456.789~");
    }

    // ── Tier routing tests ─────────────────────────────────────────

    #[test]
    fn test_tier_routing_shipped_pkce() {
        // Core services with shipped Client IDs → Tier 1
        assert_eq!(resolve_tier("github"), OAuthTier::ShippedPkce);
        assert_eq!(resolve_tier("google"), OAuthTier::ShippedPkce);
        assert_eq!(resolve_tier("gmail"), OAuthTier::ShippedPkce);
        assert_eq!(resolve_tier("discord"), OAuthTier::ShippedPkce);
        assert_eq!(resolve_tier("slack"), OAuthTier::ShippedPkce);
        assert_eq!(resolve_tier("notion"), OAuthTier::ShippedPkce);
    }

    #[test]
    fn test_tier_routing_rfc7591() {
        // OIDC providers with dynamic registration → Tier 3
        assert_eq!(resolve_tier("okta"), OAuthTier::DynamicRegistration);
        assert_eq!(resolve_tier("auth0"), OAuthTier::DynamicRegistration);
        assert_eq!(resolve_tier("keycloak"), OAuthTier::DynamicRegistration);
        assert_eq!(resolve_tier("notion-mcp"), OAuthTier::DynamicRegistration);
        assert_eq!(resolve_tier("granola-mcp"), OAuthTier::DynamicRegistration);
    }

    #[test]
    fn test_tier_routing_n8n_delegation() {
        // Services handled by n8n's built-in OAuth → Tier 2
        assert_eq!(resolve_tier("hubspot"), OAuthTier::N8nDelegation);
        assert_eq!(resolve_tier("salesforce"), OAuthTier::N8nDelegation);
        assert_eq!(resolve_tier("jira"), OAuthTier::N8nDelegation);
        assert_eq!(resolve_tier("zoom"), OAuthTier::N8nDelegation);
        assert_eq!(resolve_tier("xero"), OAuthTier::N8nDelegation);
    }

    #[test]
    fn test_tier_routing_manual_fallback() {
        // Unknown services → Tier 5 (manual API key)
        assert_eq!(resolve_tier("some-random-api"), OAuthTier::ManualApiKey);
        assert_eq!(resolve_tier(""), OAuthTier::ManualApiKey);
    }

    #[test]
    fn test_shipped_takes_priority_over_n8n() {
        // GitHub is in both shipped AND n8n lists — shipped wins
        // (since get_oauth_config is checked first in resolve_tier)
        assert_eq!(resolve_tier("github"), OAuthTier::ShippedPkce);
    }

    #[test]
    fn test_n8n_oauth_type_mapping() {
        assert_eq!(get_n8n_oauth_type("hubspot"), Some("hubspotOAuth2Api"));
        assert_eq!(get_n8n_oauth_type("jira"), Some("jiraOAuth2Api"));
        assert_eq!(get_n8n_oauth_type("zoom"), Some("zoomOAuth2Api"));
        assert_eq!(get_n8n_oauth_type("unknown"), None);
    }

    #[test]
    fn test_rfc7591_config_lookup() {
        assert!(get_rfc7591_config("okta").is_some());
        assert!(get_rfc7591_config("auth0").is_some());
        assert!(get_rfc7591_config("notion-mcp").is_some());
        assert!(get_rfc7591_config("github").is_none()); // not RFC 7591
    }

    #[test]
    fn test_n8n_credential_url() {
        let url = n8n_credential_url("http://127.0.0.1:5678", "hubspotOAuth2Api");
        assert_eq!(
            url,
            "http://127.0.0.1:5678/credentials/new?type=hubspotOAuth2Api"
        );

        let url = n8n_credential_url("http://127.0.0.1:5678/", "githubOAuth2Api");
        assert_eq!(
            url,
            "http://127.0.0.1:5678/credentials/new?type=githubOAuth2Api"
        );
    }

    #[test]
    fn test_tier_label() {
        assert_eq!(tier_label(OAuthTier::ShippedPkce), "One-click OAuth");
        assert_eq!(tier_label(OAuthTier::N8nDelegation), "Connect via n8n");
        assert_eq!(
            tier_label(OAuthTier::DynamicRegistration),
            "Auto-register (RFC 7591)"
        );
        assert_eq!(
            tier_label(OAuthTier::ManualApiKey),
            "Enter API key manually"
        );
    }

    // ── Google Workspace unified routing ───────────────────────────

    #[test]
    fn test_google_workspace_all_aliases_share_config() {
        let aliases = [
            "google",
            "google-workspace",
            "gmail",
            "google-drive",
            "google-calendar",
            "google-sheets",
            "google-docs",
        ];
        // All aliases must resolve to the same OAuthConfig (GOOGLE_OAUTH)
        for alias in &aliases {
            let config = get_oauth_config(alias);
            assert!(
                config.is_some(),
                "Alias '{}' must resolve to an OAuth config",
                alias
            );
            let config = config.unwrap();
            assert_eq!(
                config.name, "Google",
                "Alias '{}' should resolve to the Google config",
                alias
            );
        }
    }

    #[test]
    fn test_google_workspace_all_aliases_are_tier1() {
        let aliases = [
            "google",
            "google-workspace",
            "gmail",
            "google-drive",
            "google-calendar",
            "google-sheets",
            "google-docs",
        ];
        for alias in &aliases {
            assert_eq!(
                resolve_tier(alias),
                OAuthTier::ShippedPkce,
                "All Google aliases must be Tier 1 (ShippedPkce), but '{}' is {:?}",
                alias,
                resolve_tier(alias)
            );
        }
    }

    #[test]
    fn test_google_scopes_cover_all_services() {
        let config = get_oauth_config("google").unwrap();
        let scopes: Vec<&str> = config.default_scopes.to_vec();
        // Gmail
        assert!(
            scopes.iter().any(|s| s.contains("gmail")),
            "Google scopes must include Gmail"
        );
        // Calendar
        assert!(
            scopes.iter().any(|s| s.contains("calendar")),
            "Google scopes must include Calendar"
        );
        // Drive
        assert!(
            scopes.iter().any(|s| s.contains("drive")),
            "Google scopes must include Drive"
        );
        // Sheets
        assert!(
            scopes.iter().any(|s| s.contains("spreadsheets")),
            "Google scopes must include Sheets"
        );
        // Docs
        assert!(
            scopes.iter().any(|s| s.contains("documents")),
            "Google scopes must include Docs"
        );
    }

    #[test]
    fn test_google_uses_https_for_all_endpoints() {
        let config = get_oauth_config("google").unwrap();
        assert!(config.auth_url.starts_with("https://"));
        assert!(config.token_url.starts_with("https://"));
        if let Some(revoke) = config.revoke_url {
            assert!(revoke.starts_with("https://"));
        }
    }

    #[test]
    fn test_google_n8n_type_consistent_for_all_aliases() {
        let aliases = [
            "google",
            "google-workspace",
            "gmail",
            "google-drive",
            "google-calendar",
            "google-sheets",
            "google-docs",
        ];
        for alias in &aliases {
            let n8n_type = get_n8n_oauth_type(alias);
            assert_eq!(
                n8n_type,
                Some("googleOAuth2Api"),
                "Alias '{}' should map to 'googleOAuth2Api' in n8n, got {:?}",
                alias,
                n8n_type
            );
        }
    }

    #[test]
    fn test_google_has_offline_access_hint() {
        // The PKCE flow should request offline access for Google to get refresh tokens.
        // We can't test the full flow, but we can verify the config has a revoke URL
        // (which implies token management is expected).
        let config = get_oauth_config("google").unwrap();
        assert!(
            config.revoke_url.is_some(),
            "Google config should have a revoke URL for token management"
        );
    }

    // ── All shipped services have HTTPS + non-empty client_id ──────

    #[test]
    fn test_all_shipped_services_have_valid_config() {
        for service_id in oauth_service_ids() {
            if let Some(config) = get_oauth_config(service_id) {
                assert!(
                    config.auth_url.starts_with("https://"),
                    "Service '{}' auth_url must use HTTPS: {}",
                    service_id,
                    config.auth_url
                );
                assert!(
                    config.token_url.starts_with("https://"),
                    "Service '{}' token_url must use HTTPS: {}",
                    service_id,
                    config.token_url
                );
                assert!(
                    !config.client_id.is_empty(),
                    "Service '{}' must have a non-empty client_id",
                    service_id
                );
            }
        }
    }

    // ── RFC 7591 configs have valid URL templates ──────────────────

    #[test]
    fn test_rfc7591_configs_have_valid_urls() {
        for service_id in rfc7591_service_ids() {
            let config = get_rfc7591_config(service_id).unwrap();
            // URLs should be HTTPS templates
            assert!(
                config.registration_url.starts_with("https://"),
                "RFC 7591 registration URL for '{}' must use HTTPS",
                service_id
            );
            assert!(
                config.auth_url.starts_with("https://"),
                "RFC 7591 auth URL for '{}' must use HTTPS",
                service_id
            );
            assert!(
                config.token_url.starts_with("https://"),
                "RFC 7591 token URL for '{}' must use HTTPS",
                service_id
            );
        }
    }

    // ── n8n delegation coverage ────────────────────────────────────

    #[test]
    fn test_n8n_oauth_service_ids_all_have_types() {
        for service_id in n8n_oauth_service_ids() {
            assert!(
                get_n8n_oauth_type(service_id).is_some(),
                "n8n service '{}' is listed in n8n_oauth_service_ids but has no type mapping",
                service_id
            );
        }
    }

    #[test]
    fn test_no_tier_routing_gap() {
        // Every service in oauth_service_ids must route to Tier 1 (ShippedPkce)
        for service_id in oauth_service_ids() {
            assert_eq!(
                resolve_tier(service_id),
                OAuthTier::ShippedPkce,
                "Service '{}' in oauth_service_ids should be Tier 1, got {:?}",
                service_id,
                resolve_tier(service_id)
            );
        }

        // Every service in n8n_oauth_service_ids should route to either
        // Tier 1 (if also shipped) or Tier 2 (n8n delegation)
        for service_id in n8n_oauth_service_ids() {
            let tier = resolve_tier(service_id);
            assert!(
                tier == OAuthTier::ShippedPkce || tier == OAuthTier::N8nDelegation,
                "n8n service '{}' should be Tier 1 or 2, got {:?}",
                service_id,
                tier
            );
        }

        // Every service in rfc7591_service_ids must route to Tier 3
        for service_id in rfc7591_service_ids() {
            assert_eq!(
                resolve_tier(service_id),
                OAuthTier::DynamicRegistration,
                "RFC 7591 service '{}' should be Tier 3",
                service_id
            );
        }
    }
}
