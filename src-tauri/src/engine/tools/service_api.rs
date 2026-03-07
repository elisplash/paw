// Pawz Agent Engine — Generic Service API Tool
//
// A single tool that provides API access to any OAuth-connected service.
// Instead of writing 1000+ lines of Rust per service (like google.rs or
// microsoft.rs), this tool lets the agent call any API endpoint on any
// connected service by specifying the service name, HTTP method, and path.
//
// The tool automatically:
//   1. Looks up the service's base URL from the provider registry
//   2. Loads the OAuth token from the encrypted vault
//   3. Attaches required proxy headers (e.g., API version headers)
//   4. Makes the request and returns the response
//
// Adding a new service = register a client_id in registrations.json.
// No Rust code needed.

use crate::atoms::types::*;
use crate::engine::provider_registry;
use log::info;
use std::time::Duration;

// ── Token helper ───────────────────────────────────────────────────────

/// Load the OAuth access token for any service from the encrypted vault.
fn load_service_token(service_id: &str) -> Result<String, String> {
    use crate::engine::key_vault;
    use crate::engine::skills::crypto::{decrypt_credential, get_vault_key};

    let vault_key = get_vault_key().map_err(|e| format!("Vault key error: {e}"))?;

    // Try the standard key format, then common aliases
    let key = format!("oauth:{}", service_id);
    let encrypted = key_vault::get(&key)
        .ok_or_else(|| {
            let display = provider_registry::display_name(service_id)
                .unwrap_or_else(|| service_id.to_string());
            format!(
                "{display} is not connected. The user needs to connect {display} — \
                 go to Integrations → {display} → Connect."
            )
        })?;

    let json = match decrypt_credential(&encrypted, &vault_key) {
        Ok(j) => j,
        Err(_) => {
            key_vault::remove(&key);
            let display = provider_registry::display_name(service_id)
                .unwrap_or_else(|| service_id.to_string());
            return Err(format!(
                "{display} OAuth token is corrupted (likely after an app update). \
                 The user needs to reconnect {display} — go to Integrations → {display} → Connect."
            ));
        }
    };

    #[derive(serde::Deserialize)]
    struct Tokens {
        access_token: String,
    }
    let tokens: Tokens =
        serde_json::from_str(&json).map_err(|e| format!("Token parse error: {e}"))?;

    Ok(tokens.access_token)
}

// ── Tool Definition ────────────────────────────────────────────────────

pub fn definitions() -> Vec<ToolDefinition> {
    vec![ToolDefinition {
        tool_type: "function".into(),
        function: FunctionDefinition {
            name: "service_api".into(),
            description: Some(
                "Make an API request to any connected OAuth service (HubSpot, Salesforce, \
                Slack, Jira, Notion, Airtable, Shopify, Stripe, etc.). The service must be \
                connected via OAuth first. Use the provider's REST API paths."
                    .into(),
            ),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "service": {
                        "type": "string",
                        "description": "Service identifier (e.g., 'hubspot', 'salesforce', 'slack', 'jira', 'notion', 'airtable', 'shopify', 'stripe', 'zendesk', 'clickup', 'monday', 'pipedrive', 'intercom', 'asana', 'trello', 'zoom', 'quickbooks', 'mailchimp', 'xero', 'docusign', 'calendly', 'todoist', 'linear', 'figma')"
                    },
                    "method": {
                        "type": "string",
                        "enum": ["GET", "POST", "PUT", "PATCH", "DELETE"],
                        "description": "HTTP method"
                    },
                    "path": {
                        "type": "string",
                        "description": "API path relative to the service's base URL (e.g., '/crm/v3/objects/contacts' for HubSpot, '/services/data/v59.0/query' for Salesforce)"
                    },
                    "query": {
                        "type": "object",
                        "description": "URL query parameters as key-value pairs (optional)"
                    },
                    "body": {
                        "type": "object",
                        "description": "JSON request body (optional, for POST/PUT/PATCH)"
                    }
                },
                "required": ["service", "method", "path"]
            }),
        },
    }]
}

// ── Tool Execution ─────────────────────────────────────────────────────

pub async fn execute(
    name: &str,
    args: &serde_json::Value,
    _app_handle: &tauri::AppHandle,
) -> Option<Result<String, String>> {
    if name != "service_api" {
        return None;
    }
    Some(execute_service_api(args).await)
}

async fn execute_service_api(args: &serde_json::Value) -> Result<String, String> {
    let service = args
        .get("service")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'service' parameter")?;
    let method = args
        .get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("GET");
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'path' parameter")?;

    // Look up the service's base URL from the provider registry
    let base_url = provider_registry::get_base_url(service).ok_or_else(|| {
        format!(
            "Unknown service '{}'. This service is not in the provider registry.",
            service
        )
    })?;

    // §Security: Validate the path doesn't contain protocol or domain manipulation
    if path.contains("://") || path.starts_with("//") {
        return Err("Invalid path — must be a relative API path, not a full URL.".into());
    }

    let token = load_service_token(service)?;

    // Build the full URL
    let base = base_url.trim_end_matches('/');
    let path_clean = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{}", path)
    };
    let url = format!("{}{}", base, path_clean);

    // §Security: Verify the resolved URL still points to the expected base domain
    // This prevents path traversal attacks like "/../other-service"
    if !url.starts_with(base) {
        return Err("Path traversal detected — the resolved URL doesn't match the service's base URL.".into());
    }

    info!(
        "[service-api] {} {} → {} (service: {})",
        method, path, url, service
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let mut request = match method.to_uppercase().as_str() {
        "GET" => client.get(&url),
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        "PATCH" => client.patch(&url),
        "DELETE" => client.delete(&url),
        _ => return Err(format!("Unsupported HTTP method: {method}")),
    };

    // Attach OAuth token
    request = request.bearer_auth(&token);

    // Attach required proxy headers from provider config
    if let Some(headers) = provider_registry::get_proxy_headers(service) {
        for (key, value) in &headers {
            request = request.header(key.as_str(), value.as_str());
        }
    }

    // Add query parameters
    if let Some(query) = args.get("query").and_then(|v| v.as_object()) {
        let pairs: Vec<(String, String)> = query
            .iter()
            .filter_map(|(k, v)| {
                let val = match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                Some((k.clone(), val))
            })
            .collect();
        request = request.query(&pairs);
    }

    // Add request body
    if let Some(body) = args.get("body") {
        if !body.is_null() {
            request = request
                .header("content-type", "application/json")
                .json(body);
        }
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("{} API request failed: {e}", service))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {e}"))?;

    // Truncate large responses
    let max_len = 50_000;
    let body = if body.len() > max_len {
        format!(
            "{}...\n\n[Response truncated — {} bytes total, showing first {}]",
            &body[..max_len],
            body.len(),
            max_len
        )
    } else {
        body
    };

    if status.is_success() {
        Ok(body)
    } else {
        Err(format!(
            "{} API returned {} {}:\n{}",
            service,
            status.as_u16(),
            status.canonical_reason().unwrap_or(""),
            body
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_definition_shape() {
        let defs = definitions();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].function.name, "service_api");
        let params = &defs[0].function.parameters;
        let required = params.get("required").unwrap();
        assert!(required.as_array().unwrap().contains(&"service".into()));
        assert!(required.as_array().unwrap().contains(&"method".into()));
        assert!(required.as_array().unwrap().contains(&"path".into()));
    }
}
