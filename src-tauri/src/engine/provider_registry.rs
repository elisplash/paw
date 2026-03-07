// Pawz Agent Engine — Dynamic Provider Registry
//
// Loads OAuth provider configurations from Nango's open-source providers
// database (providers.json) and merges with our client ID registrations
// (registrations.json). Together these allow adding a new OAuth service
// by dropping in one client_id + scopes entry — no Rust code needed.
//
// Data flow:
//   providers.json    → auth_url, token_url, base_url, quirks (from Nango OSS)
//   registrations.json → client_id, scopes (registered by OpenPawz team)
//   Provider Registry  → merged DynamicOAuthConfig ready for PKCE flow

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

// ── Embedded Data ──────────────────────────────────────────────────────
// Both files are embedded at compile time via include_str!

const PROVIDERS_JSON: &str = include_str!("providers.json");
const REGISTRATIONS_JSON: &str = include_str!("registrations.json");

// ── Provider Config (from Nango) ───────────────────────────────────────

/// OAuth provider configuration sourced from Nango's open-source database.
/// Contains auth endpoints, API base URLs, and platform-specific quirks.
#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    pub display_name: String,
    pub auth_url: String,
    pub token_url: String,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub revoke_url: Option<String>,
    #[serde(default)]
    pub disable_pkce: bool,
    #[serde(default)]
    pub scope_separator: Option<String>,
    #[serde(default)]
    pub body_format: Option<String>,
    #[serde(default)]
    pub authorization_method: Option<String>,
    #[serde(default)]
    pub authorization_params: Option<HashMap<String, String>>,
    #[serde(default)]
    pub token_params: Option<HashMap<String, String>>,
    #[serde(default)]
    pub default_scopes: Option<Vec<String>>,
    #[serde(default)]
    pub proxy_headers: Option<HashMap<String, String>>,
    #[serde(default)]
    pub categories: Option<Vec<String>>,
    #[serde(default)]
    pub pagination: Option<serde_json::Value>,
    #[serde(default)]
    pub retry: Option<serde_json::Value>,
}

// ── Registration Config (our client IDs) ───────────────────────────────

/// Our OAuth app registration for a service — just client_id + scopes.
#[derive(Debug, Clone, Deserialize)]
pub struct Registration {
    pub client_id: String,
    #[serde(default)]
    pub scopes: Vec<String>,
}

// ── Merged Config ──────────────────────────────────────────────────────

/// Fully resolved provider config: Nango endpoint data + our client ID.
/// Ready to feed into the PKCE flow.
#[derive(Debug, Clone)]
pub struct DynamicOAuthConfig {
    pub service_id: String,
    pub display_name: String,
    pub auth_url: String,
    pub token_url: String,
    pub client_id: String,
    pub scopes: Vec<String>,
    pub base_url: Option<String>,
    pub revoke_url: Option<String>,
    pub disable_pkce: bool,
    pub scope_separator: String,
    pub body_format: String,
    pub authorization_method: Option<String>,
    pub authorization_params: Option<HashMap<String, String>>,
    pub token_params: Option<HashMap<String, String>>,
    pub proxy_headers: Option<HashMap<String, String>>,
}

// ── Registry ───────────────────────────────────────────────────────────

struct Registry {
    providers: HashMap<String, ProviderConfig>,
    registrations: HashMap<String, Registration>,
}

static REGISTRY: OnceLock<Registry> = OnceLock::new();

fn registry() -> &'static Registry {
    REGISTRY.get_or_init(|| {
        let providers: HashMap<String, ProviderConfig> =
            serde_json::from_str(PROVIDERS_JSON).unwrap_or_default();
        // Filter out _comment key from registrations
        let raw: serde_json::Value =
            serde_json::from_str(REGISTRATIONS_JSON).unwrap_or_default();
        let registrations: HashMap<String, Registration> = if let Some(obj) = raw.as_object() {
            obj.iter()
                .filter(|(k, _)| !k.starts_with('_'))
                .filter_map(|(k, v)| {
                    serde_json::from_value::<Registration>(v.clone())
                        .ok()
                        .map(|r| (k.clone(), r))
                })
                .collect()
        } else {
            HashMap::new()
        };

        log::info!(
            "[provider-registry] Loaded {} providers, {} registrations",
            providers.len(),
            registrations.len(),
        );

        Registry {
            providers,
            registrations,
        }
    })
}

// ── Public API ─────────────────────────────────────────────────────────

/// Get the merged OAuth config for a dynamic provider.
/// Returns None if the service has no provider config or no registration.
pub fn get_dynamic_config(service_id: &str) -> Option<DynamicOAuthConfig> {
    let reg = registry();
    let provider = reg.providers.get(service_id)?;
    let registration = reg.registrations.get(service_id)?;

    // Check for runtime env var override of client_id
    let env_key = format!(
        "OPENPAWZ_{}_CLIENT_ID",
        service_id.to_uppercase().replace('-', "_")
    );
    let client_id = std::env::var(&env_key)
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| registration.client_id.clone());

    Some(DynamicOAuthConfig {
        service_id: service_id.to_string(),
        display_name: provider.display_name.clone(),
        auth_url: provider.auth_url.clone(),
        token_url: provider.token_url.clone(),
        client_id,
        scopes: registration.scopes.clone(),
        base_url: provider.base_url.clone(),
        revoke_url: provider.revoke_url.clone(),
        disable_pkce: provider.disable_pkce,
        scope_separator: provider
            .scope_separator
            .clone()
            .unwrap_or_else(|| " ".to_string()),
        body_format: provider
            .body_format
            .clone()
            .unwrap_or_else(|| "form".to_string()),
        authorization_method: provider.authorization_method.clone(),
        authorization_params: provider.authorization_params.clone(),
        token_params: provider.token_params.clone(),
        proxy_headers: provider.proxy_headers.clone(),
    })
}

/// Check if a service has a dynamic provider config (regardless of registration).
pub fn has_provider(service_id: &str) -> bool {
    registry().providers.contains_key(service_id)
}

/// Check if a service has a registered client ID (may still be placeholder).
pub fn has_registration(service_id: &str) -> bool {
    registry().registrations.contains_key(service_id)
}

/// Check if a service has a real (non-placeholder) client ID ready to use.
pub fn is_ready(service_id: &str) -> bool {
    if let Some(config) = get_dynamic_config(service_id) {
        !config.client_id.starts_with("REPLACE_WITH_")
    } else {
        false
    }
}

/// Get the API base URL for a connected service.
pub fn get_base_url(service_id: &str) -> Option<String> {
    registry()
        .providers
        .get(service_id)
        .and_then(|p| p.base_url.clone())
}

/// Get proxy headers that should be sent with API requests.
pub fn get_proxy_headers(service_id: &str) -> Option<HashMap<String, String>> {
    registry()
        .providers
        .get(service_id)
        .and_then(|p| p.proxy_headers.clone())
}

/// List all service IDs that have both a provider config AND a registration.
/// These are the services that can be offered to users (even if placeholder).
pub fn registered_service_ids() -> Vec<String> {
    let reg = registry();
    reg.registrations
        .keys()
        .filter(|id| reg.providers.contains_key(id.as_str()))
        .cloned()
        .collect()
}

/// List all service IDs that are ready (non-placeholder client ID).
pub fn ready_service_ids() -> Vec<String> {
    registered_service_ids()
        .into_iter()
        .filter(|id| is_ready(id))
        .collect()
}

/// Get the display name for a provider.
pub fn display_name(service_id: &str) -> Option<String> {
    registry()
        .providers
        .get(service_id)
        .map(|p| p.display_name.clone())
}

/// Total number of OAuth2 providers available from Nango's database.
pub fn total_providers() -> usize {
    registry().providers.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_providers_load() {
        let count = total_providers();
        assert!(count > 200, "Expected 200+ providers, got {}", count);
    }

    #[test]
    fn test_registrations_load() {
        let ids = registered_service_ids();
        assert!(ids.len() > 10, "Expected 10+ registrations, got {}", ids.len());
    }

    #[test]
    fn test_hubspot_config() {
        let config = get_dynamic_config("hubspot");
        assert!(config.is_some(), "HubSpot should have a dynamic config");
        let config = config.unwrap();
        assert_eq!(config.display_name, "HubSpot");
        assert!(config.auth_url.contains("hubspot.com"));
        assert!(config.token_url.contains("hubapi.com"));
    }

    #[test]
    fn test_placeholder_not_ready() {
        // All registrations currently have placeholder IDs
        assert!(!is_ready("hubspot"));
    }

    #[test]
    fn test_has_provider_without_registration() {
        // Nango has providers we haven't registered yet
        assert!(has_provider("gitlab"));
    }
}
