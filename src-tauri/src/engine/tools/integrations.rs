// Paw Agent Engine — Integration tools
// rest_api_call, webhook_send, image_generate

use crate::atoms::error::EngineResult;
use crate::atoms::types::*;
use crate::engine::util::safe_truncate;
use log::info;
use std::time::Duration;

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "rest_api_call".into(),
                description: "Make an authenticated API call using stored credentials. The API key is injected automatically — never include credentials in the request. Use the 'service' parameter to specify which connected service to call (e.g., 'linear', 'stripe', 'todoist').".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "service": { "type": "string", "description": "Which connected service to call (e.g., 'linear', 'stripe', 'jira'). Required when multiple services are connected." },
                        "path": { "type": "string", "description": "API endpoint path (appended to the stored base URL)" },
                        "method": { "type": "string", "enum": ["GET", "POST", "PUT", "PATCH", "DELETE"], "description": "HTTP method (default: GET)" },
                        "headers": { "type": "object", "description": "Additional headers (auth is added automatically)" },
                        "body": { "type": "string", "description": "Request body (JSON string)" }
                    },
                    "required": ["path"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "webhook_send".into(),
                description: "Send a JSON payload to a stored webhook URL (Zapier, IFTTT, n8n, custom). The URL is stored securely.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "payload": { "type": "object", "description": "JSON payload to send to the webhook" }
                    },
                    "required": ["payload"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "image_generate".into(),
                description: "Generate an image from a text description using AI. Returns the file path of the saved image. Use detailed, descriptive prompts for best results.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "prompt": { "type": "string", "description": "Detailed text description of the image to generate." },
                        "filename": { "type": "string", "description": "Optional filename for the output image (without extension)." }
                    },
                    "required": ["prompt"]
                }),
            },
        },
    ]
}

/// Services that use the rest_api_call tool with per-service credential vaults.
/// These were previously all mapped to "rest_api" causing credential collisions.
const REST_API_SERVICES: &[&str] = &[
    "linear",
    "stripe",
    "todoist",
    "clickup",
    "airtable",
    "sendgrid",
    "jira",
    "zendesk",
    "hubspot",
    "twilio",
    "shopify",
    "pagerduty",
    "microsoft_teams",
];

/// Return definitions only for the given skill_id ("rest_api", "webhook", "image_gen",
/// or a per-service ID like "linear", "stripe", etc.)
pub fn definitions_for(skill_id: &str) -> Vec<ToolDefinition> {
    definitions()
        .into_iter()
        .filter(|d| match skill_id {
            "rest_api" => d.function.name == "rest_api_call",
            "webhook" => d.function.name == "webhook_send",
            "image_gen" => d.function.name == "image_generate",
            s if REST_API_SERVICES.contains(&s) => d.function.name == "rest_api_call",
            _ => false,
        })
        .collect()
}

pub async fn execute(
    name: &str,
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
) -> Option<Result<String, String>> {
    let skill_id = match name {
        "rest_api_call" => {
            // Resolve per-service vault: check `service` param, fall back to "rest_api"
            if let Some(svc) = args.get("service").and_then(|v| v.as_str()) {
                let normalized = svc.replace('-', "_");
                if REST_API_SERVICES.contains(&normalized.as_str()) {
                    normalized
                } else {
                    // Unknown service — try it as skill_id directly (forward compat)
                    normalized
                }
            } else {
                "rest_api".to_string()
            }
        }
        "webhook_send" => "webhook".to_string(),
        "image_generate" => "image_gen".to_string(),
        _ => return None,
    };
    let creds = match super::get_skill_creds(&skill_id, app_handle) {
        Ok(c) => c,
        Err(e) => {
            // If the skill isn't found, list available services for a helpful error
            if name == "rest_api_call" {
                let available = find_connected_rest_services(app_handle);
                if !available.is_empty() {
                    return Some(Err(format!(
                        "{}. Available services: {}. Specify the service name with the 'service' parameter.",
                        e, available.join(", ")
                    )));
                }
            }
            return Some(Err(e.to_string()));
        }
    };
    Some(match name {
        "rest_api_call" => execute_rest_api_call(args, &creds)
            .await
            .map_err(|e| e.to_string()),
        "webhook_send" => execute_webhook_send(args, &creds)
            .await
            .map_err(|e| e.to_string()),
        "image_generate" => execute_image_generate(args, &creds)
            .await
            .map_err(|e| e.to_string()),
        _ => unreachable!(),
    })
}

/// Find which REST API services the user has connected (enabled with credentials).
fn find_connected_rest_services(app_handle: &tauri::AppHandle) -> Vec<String> {
    use crate::engine::state::EngineState;
    use tauri::Manager;

    let state = match app_handle.try_state::<EngineState>() {
        Some(s) => s,
        None => return vec![],
    };
    let mut connected = Vec::new();
    for svc in REST_API_SERVICES {
        if state.store.is_skill_enabled(svc).unwrap_or(false) {
            // Use SERVICE_NAME if stored, else the raw skill_id
            let svc_name = crate::engine::skills::get_skill_credentials(&state.store, svc)
                .ok()
                .and_then(|c| c.get("SERVICE_NAME").cloned())
                .unwrap_or_else(|| svc.to_string());
            connected.push(svc_name);
        }
    }
    // Also check legacy "rest_api"
    if state.store.is_skill_enabled("rest_api").unwrap_or(false) {
        connected.push("rest_api (legacy)".into());
    }
    connected
}

async fn execute_rest_api_call(
    args: &serde_json::Value,
    creds: &std::collections::HashMap<String, String>,
) -> EngineResult<String> {
    let path = args["path"]
        .as_str()
        .ok_or("rest_api_call: missing 'path'")?;
    let method = args["method"].as_str().unwrap_or("GET");
    let base_url = creds.get("API_BASE_URL").ok_or("Missing API_BASE_URL")?;
    let api_key = creds.get("API_KEY").ok_or("Missing API_KEY")?;
    let auth_header = creds
        .get("API_AUTH_HEADER")
        .map(|s| s.as_str())
        .unwrap_or("Authorization");
    let auth_prefix = creds
        .get("API_AUTH_PREFIX")
        .map(|s| s.as_str())
        .unwrap_or("Bearer");

    let url = format!(
        "{}{}",
        base_url.trim_end_matches('/'),
        if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{}", path)
        }
    );
    info!("[skill:rest_api] {} {}", method, url);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    let mut request = match method.to_uppercase().as_str() {
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        "PATCH" => client.patch(&url),
        "DELETE" => client.delete(&url),
        _ => client.get(&url),
    };

    request = request.header(auth_header, format!("{} {}", auth_prefix, api_key));

    if let Some(headers) = args["headers"].as_object() {
        for (key, value) in headers {
            if let Some(v) = value.as_str() {
                request = request.header(key.as_str(), v);
            }
        }
    }

    if let Some(body) = args["body"].as_str() {
        request = request
            .header("Content-Type", "application/json")
            .body(body.to_string());
    }

    let resp = request.send().await?;
    let status = resp.status().as_u16();
    let body = resp.text().await?;
    let truncated = if body.len() > 30_000 {
        format!(
            "{}...\n[truncated, {} total bytes]",
            &body[..30_000],
            body.len()
        )
    } else {
        body
    };

    Ok(format!(
        "API {} {} → {}\n\n{}",
        method, path, status, truncated
    ))
}

async fn execute_webhook_send(
    args: &serde_json::Value,
    creds: &std::collections::HashMap<String, String>,
) -> EngineResult<String> {
    let payload = args
        .get("payload")
        .ok_or("webhook_send: missing 'payload'")?;
    let url = creds.get("WEBHOOK_URL").ok_or("Missing WEBHOOK_URL")?;
    info!("[skill:webhook] POST {}", url);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?;

    let mut request = client
        .post(url.as_str())
        .header("Content-Type", "application/json")
        .json(payload);

    if let Some(secret) = creds.get("WEBHOOK_SECRET") {
        if !secret.is_empty() {
            let payload_str = serde_json::to_string(payload).unwrap_or_default();
            let signature = format!("sha256={}", simple_hmac_hex(secret, &payload_str));
            request = request.header("X-Signature-256", &signature);
        }
    }

    let resp = request.send().await?;
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();

    if status < 400 {
        Ok(format!(
            "Webhook delivered (HTTP {}). Response: {}",
            status,
            safe_truncate(&body, 1000)
        ))
    } else {
        Err(format!(
            "Webhook failed (HTTP {}): {}",
            status,
            safe_truncate(&body, 1000)
        )
        .into())
    }
}

fn simple_hmac_hex(key: &str, data: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    data.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

async fn execute_image_generate(
    args: &serde_json::Value,
    creds: &std::collections::HashMap<String, String>,
) -> EngineResult<String> {
    let prompt = args["prompt"]
        .as_str()
        .ok_or("image_generate: missing 'prompt'")?;
    let filename = args["filename"].as_str().unwrap_or("");
    let api_key = creds
        .get("GEMINI_API_KEY")
        .ok_or("Missing GEMINI_API_KEY credential")?;

    info!(
        "[skill:image_gen] Generating image for prompt: {}",
        safe_truncate(prompt, 80)
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()?;

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.0-flash-exp:generateContent?key={}",
        api_key
    );

    let body = serde_json::json!({
        "contents": [{ "parts": [{ "text": prompt }] }],
        "generationConfig": { "responseModalities": ["TEXT", "IMAGE"] }
    });

    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    let status = resp.status().as_u16();
    let resp_text = resp.text().await?;

    if status >= 400 {
        return Err(format!(
            "Gemini API error (HTTP {}): {}",
            status,
            safe_truncate(&resp_text, 500)
        )
        .into());
    }

    let resp_json: serde_json::Value = serde_json::from_str(&resp_text)?;

    let parts = resp_json
        .get("candidates")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("content"))
        .and_then(|c| c.get("parts"))
        .and_then(|p| p.as_array())
        .ok_or("Gemini response missing candidates/content/parts")?;

    let mut image_data: Option<(String, String)> = None;
    let mut text_response: Option<String> = None;

    for part in parts {
        if let Some(inline) = part.get("inlineData") {
            let mime = inline["mimeType"].as_str().unwrap_or("image/png");
            let data = inline["data"].as_str().unwrap_or("");
            if !data.is_empty() {
                image_data = Some((mime.to_string(), data.to_string()));
            }
        }
        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
            text_response = Some(text.to_string());
        }
    }

    let (mime_type, base64_data) = image_data
        .ok_or("Gemini did not return an image. The model may not support image generation for this prompt. Try a more descriptive prompt.")?;

    let ext = match mime_type.as_str() {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => "png",
    };

    let output_name = if filename.is_empty() {
        let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
        let slug: String = prompt
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == ' ')
            .take(30)
            .collect::<String>()
            .trim()
            .replace(' ', "_")
            .to_lowercase();
        format!("generated_{}_{}", ts, slug)
    } else {
        filename.to_string()
    };

    let output_dir = std::env::var("HOME")
        .map(|h| std::path::PathBuf::from(h).join("Pictures").join("paw"))
        .unwrap_or_else(|_| std::env::temp_dir().join("paw_images"));

    std::fs::create_dir_all(&output_dir)?;

    let output_path = output_dir.join(format!("{}.{}", output_name, ext));

    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&base64_data)
        .map_err(|e| crate::atoms::error::EngineError::Other(e.to_string()))?;

    std::fs::write(&output_path, &bytes)?;

    let path_str = output_path.to_string_lossy().to_string();
    let size_kb = bytes.len() / 1024;

    info!(
        "[skill:image_gen] Saved {} ({} KB) to {}",
        mime_type, size_kb, path_str
    );

    let mut result = format!(
        "Image generated and saved to: {}\nSize: {} KB | Format: {}",
        path_str,
        size_kb,
        ext.to_uppercase()
    );
    if let Some(text) = text_response {
        result.push_str(&format!("\n\nModel notes: {}", text));
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definitions_for_rest_api() {
        let defs = definitions_for("rest_api");
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].function.name, "rest_api_call");
    }

    #[test]
    fn definitions_for_per_service_returns_rest_api_call() {
        // Each per-service skill should enable the rest_api_call tool
        for service in REST_API_SERVICES {
            let defs = definitions_for(service);
            assert_eq!(
                defs.len(),
                1,
                "Service '{}' should have exactly 1 tool definition",
                service
            );
            assert_eq!(
                defs[0].function.name, "rest_api_call",
                "Service '{}' should provide rest_api_call tool",
                service
            );
        }
    }

    #[test]
    fn definitions_for_webhook() {
        let defs = definitions_for("webhook");
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].function.name, "webhook_send");
    }

    #[test]
    fn definitions_for_image_gen() {
        let defs = definitions_for("image_gen");
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].function.name, "image_generate");
    }

    #[test]
    fn definitions_for_unknown_returns_empty() {
        let defs = definitions_for("nonexistent_skill");
        assert!(defs.is_empty());
    }

    #[test]
    fn rest_api_call_has_service_parameter() {
        let defs = definitions_for("rest_api");
        let params = &defs[0].function.parameters;
        let props = params.get("properties").unwrap();
        assert!(
            props.get("service").is_some(),
            "rest_api_call must have a 'service' parameter"
        );
    }
}
