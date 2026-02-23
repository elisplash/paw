// Paw Agent Engine — Google Workspace Tools
//
// Service account authentication + direct REST API calls to:
//   Gmail, Calendar, Drive, Sheets, Docs
//
// Auth flow:
//   1. Service account JSON key stored as skill credential (SERVICE_ACCOUNT_JSON)
//   2. Generate JWT signed with the service account private key
//   3. Exchange JWT for access token via Google's token endpoint
//   4. Cache token (1 hour lifetime) to avoid re-auth per tool call
//
// Setup: user creates a Google Cloud project, enables APIs, creates a service
// account with domain-wide delegation, downloads the JSON key, and pastes it
// into the Pawz skill credential field.

use crate::atoms::types::*;
use crate::atoms::error::EngineResult;
use log::{info, warn};
use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use parking_lot::Mutex;
use base64::Engine as _;
use serde::{Serialize, Deserialize};

// ── Token cache ────────────────────────────────────────────────────────────

struct CachedToken {
    access_token: String,
    expires_at: u64,
}

static TOKEN_CACHE: LazyLock<Mutex<Option<CachedToken>>> =
    LazyLock::new(|| Mutex::new(None));

/// Clear the cached access token (used by google_oauth disconnect).
pub fn clear_token_cache() {
    *TOKEN_CACHE.lock() = None;
}

// ── Tool definitions ───────────────────────────────────────────────────────

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        // ── Gmail ────────────────────────────────────────────────
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "google_gmail_list".into(),
                description: "List recent emails from Google Gmail. Returns subject, from, date, snippet for each message.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Gmail search query (e.g. 'from:user@example.com', 'is:unread', 'subject:invoice'). Uses Gmail search operators." },
                        "max_results": { "type": "integer", "description": "Maximum messages to return (default: 10, max: 50)" }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "google_gmail_read".into(),
                description: "Read the full content of a specific Gmail message by its message ID.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "message_id": { "type": "string", "description": "The Gmail message ID to read (from google_gmail_list results)" }
                    },
                    "required": ["message_id"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "google_gmail_send".into(),
                description: "Send an email via Gmail. Can send to any email address.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "to": { "type": "string", "description": "Recipient email address" },
                        "subject": { "type": "string", "description": "Email subject line" },
                        "body": { "type": "string", "description": "Email body (plain text or HTML)" },
                        "cc": { "type": "string", "description": "CC recipients (comma-separated)" },
                        "html": { "type": "boolean", "description": "If true, body is treated as HTML (default: false)" }
                    },
                    "required": ["to", "subject", "body"]
                }),
            },
        },

        // ── Calendar ─────────────────────────────────────────────
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "google_calendar_list".into(),
                description: "List upcoming events from Google Calendar.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "time_min": { "type": "string", "description": "Start datetime in RFC3339 format (default: now). Example: 2026-02-23T00:00:00Z" },
                        "time_max": { "type": "string", "description": "End datetime in RFC3339 format (default: 7 days from now)" },
                        "max_results": { "type": "integer", "description": "Maximum events to return (default: 20)" },
                        "calendar_id": { "type": "string", "description": "Calendar ID (default: 'primary')" }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "google_calendar_create".into(),
                description: "Create a new Google Calendar event.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "summary": { "type": "string", "description": "Event title" },
                        "start": { "type": "string", "description": "Start datetime in RFC3339 format (e.g. 2026-02-24T10:00:00-05:00)" },
                        "end": { "type": "string", "description": "End datetime in RFC3339 format" },
                        "description": { "type": "string", "description": "Event description" },
                        "attendees": { "type": "array", "items": { "type": "string" }, "description": "List of attendee email addresses" },
                        "location": { "type": "string", "description": "Event location" },
                        "calendar_id": { "type": "string", "description": "Calendar ID (default: 'primary')" }
                    },
                    "required": ["summary", "start", "end"]
                }),
            },
        },

        // ── Drive ────────────────────────────────────────────────
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "google_drive_list".into(),
                description: "List files in Google Drive. Returns file name, type, size, last modified.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Drive search query (e.g. \"name contains 'report'\", \"mimeType='application/pdf'\"). Uses Drive query syntax." },
                        "folder_id": { "type": "string", "description": "ID of a specific folder to list (default: root)" },
                        "max_results": { "type": "integer", "description": "Maximum files to return (default: 20)" }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "google_drive_read".into(),
                description: "Read the content of a Google Drive file (Docs, Sheets exports as text, or download small files).".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_id": { "type": "string", "description": "The Drive file ID to read" },
                        "export_format": { "type": "string", "description": "For Google Docs/Sheets, export format: 'text/plain', 'text/csv', 'application/pdf' (default: 'text/plain')" }
                    },
                    "required": ["file_id"]
                }),
            },
        },

        // ── Sheets ───────────────────────────────────────────────
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "google_sheets_read".into(),
                description: "Read data from a Google Sheets spreadsheet.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "spreadsheet_id": { "type": "string", "description": "The spreadsheet ID (from the URL)" },
                        "range": { "type": "string", "description": "Cell range in A1 notation (e.g. 'Sheet1!A1:D10', 'Sheet1')" }
                    },
                    "required": ["spreadsheet_id", "range"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "google_sheets_append".into(),
                description: "Append rows to a Google Sheets spreadsheet.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "spreadsheet_id": { "type": "string", "description": "The spreadsheet ID" },
                        "range": { "type": "string", "description": "Target range in A1 notation (e.g. 'Sheet1!A1')" },
                        "values": { "type": "array", "items": { "type": "array", "items": { "type": "string" } }, "description": "2D array of values to append (each inner array is a row)" }
                    },
                    "required": ["spreadsheet_id", "range", "values"]
                }),
            },
        },

        // ── Generic Google API ───────────────────────────────────
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "google_api".into(),
                description: "Make an authenticated call to any Google REST API. Use this for operations not covered by the specific Google tools (Docs, Admin, Contacts, etc).".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "url": { "type": "string", "description": "Full Google API URL (e.g. 'https://www.googleapis.com/drive/v3/files')" },
                        "method": { "type": "string", "enum": ["GET", "POST", "PUT", "PATCH", "DELETE"], "description": "HTTP method (default: GET)" },
                        "body": { "type": "object", "description": "JSON body for POST/PUT/PATCH requests" }
                    },
                    "required": ["url"]
                }),
            },
        },
    ]
}

// ── Execute dispatcher ─────────────────────────────────────────────────────

pub async fn execute(
    name: &str,
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
) -> Option<Result<String, String>> {
    let creds = match super::get_skill_creds("google_workspace", app_handle) {
        Ok(c) => c,
        Err(e) => return if name.starts_with("google_") { Some(Err(e.to_string())) } else { None },
    };

    Some(match name {
        "google_gmail_list"       => exec_gmail_list(args, &creds).await.map_err(|e| e.to_string()),
        "google_gmail_read"       => exec_gmail_read(args, &creds).await.map_err(|e| e.to_string()),
        "google_gmail_send"       => exec_gmail_send(args, &creds).await.map_err(|e| e.to_string()),
        "google_calendar_list"    => exec_calendar_list(args, &creds).await.map_err(|e| e.to_string()),
        "google_calendar_create"  => exec_calendar_create(args, &creds).await.map_err(|e| e.to_string()),
        "google_drive_list"       => exec_drive_list(args, &creds).await.map_err(|e| e.to_string()),
        "google_drive_read"       => exec_drive_read(args, &creds).await.map_err(|e| e.to_string()),
        "google_sheets_read"      => exec_sheets_read(args, &creds).await.map_err(|e| e.to_string()),
        "google_sheets_append"    => exec_sheets_append(args, &creds).await.map_err(|e| e.to_string()),
        "google_api"              => exec_google_api(args, &creds).await.map_err(|e| e.to_string()),
        _ => return None,
    })
}

// ── Service Account JWT Auth ───────────────────────────────────────────────

/// JWT claims for Google service account token exchange.
#[derive(Debug, Serialize, Deserialize)]
struct GoogleJwtClaims {
    iss: String,
    sub: String,
    scope: String,
    aud: String,
    iat: u64,
    exp: u64,
}

/// Get a valid access token, using the cache if available.
/// Supports two auth methods:
///   1. OAuth2 (preferred) — if GOOGLE_REFRESH_TOKEN is set, use it with auto-refresh
///   2. Service Account — if SERVICE_ACCOUNT_JSON is set, use JWT flow
async fn get_access_token(creds: &HashMap<String, String>) -> EngineResult<String> {
    // Check cache first
    {
        let cache = TOKEN_CACHE.lock();
        if let Some(ref cached) = *cache {
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
            if now < cached.expires_at.saturating_sub(60) {
                return Ok(cached.access_token.clone());
            }
        }
    }

    // Route to the right auth method
    if creds.contains_key("GOOGLE_REFRESH_TOKEN") {
        get_access_token_oauth2(creds).await
    } else if creds.contains_key("SERVICE_ACCOUNT_JSON") {
        get_access_token_service_account(creds).await
    } else {
        Err("Google not connected. Click 'Connect with Google' in Skills → Google Workspace, or paste a Service Account JSON for enterprise setup.".into())
    }
}

/// OAuth2 flow: refresh the access token using the stored refresh token.
async fn get_access_token_oauth2(creds: &HashMap<String, String>) -> EngineResult<String> {
    let refresh_token = creds.get("GOOGLE_REFRESH_TOKEN")
        .ok_or("Missing GOOGLE_REFRESH_TOKEN")?;

    // Check if we have a non-expired access token stored
    if let (Some(access_token), Some(expires_at_str)) = (
        creds.get("GOOGLE_ACCESS_TOKEN"),
        creds.get("GOOGLE_TOKEN_EXPIRES_AT"),
    ) {
        if let Ok(expires_at) = expires_at_str.parse::<u64>() {
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
            if now < expires_at.saturating_sub(60) {
                // Cache it and return
                let mut cache = TOKEN_CACHE.lock();
                *cache = Some(CachedToken {
                    access_token: access_token.clone(),
                    expires_at,
                });
                return Ok(access_token.clone());
            }
        }
    }

    // Need to refresh — try bundled creds first, then user-provided from creds map
    let (client_id, client_secret) = {
        if let (Some(id), Some(secret)) = (
            option_env!("PAW_GOOGLE_CLIENT_ID"),
            option_env!("PAW_GOOGLE_CLIENT_SECRET"),
        ) {
            if !id.is_empty() && !secret.is_empty() {
                (id.to_string(), secret.to_string())
            } else {
                (creds.get("GOOGLE_CLIENT_ID").ok_or("Missing GOOGLE_CLIENT_ID")?.clone(),
                 creds.get("GOOGLE_CLIENT_SECRET").ok_or("Missing GOOGLE_CLIENT_SECRET")?.clone())
            }
        } else {
            (creds.get("GOOGLE_CLIENT_ID").ok_or("Missing GOOGLE_CLIENT_ID")?.clone(),
             creds.get("GOOGLE_CLIENT_SECRET").ok_or("Missing GOOGLE_CLIENT_SECRET")?.clone())
        }
    };

    let (access_token, expires_at) = super::google_oauth::refresh_access_token(
        refresh_token, &client_id, &client_secret,
    ).await?;

    info!("[google] Access token refreshed via OAuth2");

    // Cache the new token
    {
        let mut cache = TOKEN_CACHE.lock();
        *cache = Some(CachedToken {
            access_token: access_token.clone(),
            expires_at,
        });
    }

    // NOTE: We don't write the refreshed token back to the vault here because
    // we don't have the app_handle. The cache handles it for this session.
    // The refresh_token is long-lived and doesn't change.

    Ok(access_token)
}

/// Service account JWT flow (for Google Workspace enterprise users).
async fn get_access_token_service_account(creds: &HashMap<String, String>) -> EngineResult<String> {
    let sa_json_str = creds.get("SERVICE_ACCOUNT_JSON")
        .ok_or("Missing SERVICE_ACCOUNT_JSON credential. Add your Google service account JSON key in Skills → Google Workspace.")?;

    let sa: serde_json::Value = serde_json::from_str(sa_json_str)
        .map_err(|e| format!("Invalid SERVICE_ACCOUNT_JSON: {}", e))?;

    let client_email = sa["client_email"].as_str()
        .ok_or("SERVICE_ACCOUNT_JSON missing 'client_email'")?;
    let private_key_pem = sa["private_key"].as_str()
        .ok_or("SERVICE_ACCOUNT_JSON missing 'private_key'")?;
    let token_uri = sa["token_uri"].as_str()
        .unwrap_or("https://oauth2.googleapis.com/token");

    // The impersonation email — use DELEGATE_EMAIL if set, otherwise fall back to client_email
    let delegate_email = creds.get("DELEGATE_EMAIL")
        .map(|s| s.as_str())
        .unwrap_or(client_email);

    // Scopes for Gmail, Calendar, Drive, Sheets, Docs
    let scopes = [
        "https://www.googleapis.com/auth/gmail.modify",
        "https://www.googleapis.com/auth/gmail.send",
        "https://www.googleapis.com/auth/calendar",
        "https://www.googleapis.com/auth/drive",
        "https://www.googleapis.com/auth/spreadsheets",
        "https://www.googleapis.com/auth/documents",
    ].join(" ");

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let exp = now + 3600;

    // Build and sign JWT using the jsonwebtoken crate (RS256)
    let claims = GoogleJwtClaims {
        iss: client_email.to_string(),
        sub: delegate_email.to_string(),
        scope: scopes,
        aud: token_uri.to_string(),
        iat: now,
        exp,
    };

    let encoding_key = jsonwebtoken::EncodingKey::from_rsa_pem(private_key_pem.as_bytes())
        .map_err(|e| format!("Failed to parse service account private key: {}", e))?;

    let header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    let jwt = jsonwebtoken::encode(&header, &claims, &encoding_key)
        .map_err(|e| format!("Failed to sign JWT: {}", e))?;

    // Exchange JWT for access token
    let client = reqwest::Client::new();
    let resp = client.post(token_uri)
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("assertion", &jwt),
        ])
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("Token exchange request failed: {}", e))?;

    let status = resp.status();
    let body = resp.text().await.map_err(|e| format!("Token response read error: {}", e))?;

    if !status.is_success() {
        return Err(format!("Google token exchange failed ({}): {}", status, body).into());
    }

    let token_resp: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| format!("Invalid token response JSON: {}", e))?;
    let access_token = token_resp["access_token"].as_str()
        .ok_or("Token response missing 'access_token'")?
        .to_string();

    info!("[google] Access token obtained for {}", delegate_email);

    // Cache the token
    {
        let mut cache = TOKEN_CACHE.lock();
        *cache = Some(CachedToken {
            access_token: access_token.clone(),
            expires_at: exp,
        });
    }

    Ok(access_token)
}

fn base64_url_decode(s: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(s)
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(s))
        .map_err(|e| format!("base64 decode error: {}", e))
}

fn base64_url_encode(data: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

fn url_encode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

// ── Shared HTTP helper ─────────────────────────────────────────────────────

async fn google_request(
    method: &str,
    url: &str,
    token: &str,
    body: Option<&serde_json::Value>,
) -> EngineResult<(u16, serde_json::Value)> {
    let client = reqwest::Client::new();
    let mut req = match method {
        "POST"   => client.post(url),
        "PUT"    => client.put(url),
        "PATCH"  => client.patch(url),
        "DELETE" => client.delete(url),
        _        => client.get(url),
    };

    req = req
        .header("Authorization", format!("Bearer {}", token))
        .header("Accept", "application/json")
        .timeout(Duration::from_secs(30));

    if let Some(b) = body {
        req = req.header("Content-Type", "application/json").json(b);
    }

    let resp = req.send().await.map_err(|e| format!("Google API request failed: {}", e))?;
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();

    let json: serde_json::Value = serde_json::from_str(&text)
        .unwrap_or(serde_json::json!({ "raw": text }));

    if status >= 400 {
        let error_msg = json["error"]["message"].as_str()
            .unwrap_or(&text);
        return Err(format!("Google API error ({}): {}", status, error_msg).into());
    }

    Ok((status, json))
}

// ── Gmail implementations ──────────────────────────────────────────────────

async fn exec_gmail_list(args: &serde_json::Value, creds: &HashMap<String, String>) -> EngineResult<String> {
    let token = get_access_token(creds).await?;
    let query = args["query"].as_str().unwrap_or("");
    let max_results = args["max_results"].as_u64().unwrap_or(10).min(50);
    let delegate = creds.get("DELEGATE_EMAIL").map(|s| s.as_str()).unwrap_or("me");
    let user = if delegate == "me" || delegate.is_empty() { "me" } else { delegate };

    let mut url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/{}/messages?maxResults={}",
        user, max_results
    );
    if !query.is_empty() {
        url.push_str(&format!("&q={}", url_encode(query)));
    }

    let (_, list_resp) = google_request("GET", &url, &token, None).await?;

    let messages = list_resp["messages"].as_array();
    if messages.is_none() || messages.unwrap().is_empty() {
        return Ok("No messages found.".into());
    }

    let msg_ids: Vec<&str> = messages.unwrap().iter()
        .filter_map(|m| m["id"].as_str())
        .collect();

    let mut output = format!("# Gmail — {} messages\n\n", msg_ids.len());

    // Fetch metadata for each message
    for msg_id in &msg_ids {
        let msg_url = format!(
            "https://gmail.googleapis.com/gmail/v1/users/{}/messages/{}?format=metadata&metadataHeaders=From&metadataHeaders=Subject&metadataHeaders=Date",
            user, msg_id
        );
        match google_request("GET", &msg_url, &token, None).await {
            Ok((_, msg)) => {
                let headers = msg["payload"]["headers"].as_array();
                let get_header = |name: &str| -> String {
                    headers.and_then(|h| h.iter()
                        .find(|hdr| hdr["name"].as_str() == Some(name))
                        .and_then(|hdr| hdr["value"].as_str())
                        .map(|s| s.to_string())
                    ).unwrap_or_default()
                };
                let subject = get_header("Subject");
                let from = get_header("From");
                let date = get_header("Date");
                let snippet = msg["snippet"].as_str().unwrap_or("");
                let labels = msg["labelIds"].as_array()
                    .map(|l| l.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(", "))
                    .unwrap_or_default();

                output.push_str(&format!(
                    "**{}** (id: `{}`)\n  From: {} | Date: {}\n  Labels: {}\n  > {}\n\n",
                    subject, msg_id, from, date, labels, snippet
                ));
            }
            Err(e) => {
                warn!("[google] Failed to fetch message {}: {}", msg_id, e);
            }
        }
    }

    Ok(output)
}

async fn exec_gmail_read(args: &serde_json::Value, creds: &HashMap<String, String>) -> EngineResult<String> {
    let token = get_access_token(creds).await?;
    let message_id = args["message_id"].as_str().ok_or("Missing 'message_id'")?;
    let delegate = creds.get("DELEGATE_EMAIL").map(|s| s.as_str()).unwrap_or("me");
    let user = if delegate == "me" || delegate.is_empty() { "me" } else { delegate };

    let url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/{}/messages/{}?format=full",
        user, message_id
    );

    let (_, msg) = google_request("GET", &url, &token, None).await?;

    let headers = msg["payload"]["headers"].as_array();
    let get_header = |name: &str| -> String {
        headers.and_then(|h| h.iter()
            .find(|hdr| hdr["name"].as_str() == Some(name))
            .and_then(|hdr| hdr["value"].as_str())
            .map(|s| s.to_string())
        ).unwrap_or_default()
    };

    let subject = get_header("Subject");
    let from = get_header("From");
    let to = get_header("To");
    let date = get_header("Date");

    // Extract body — try plain text first, then HTML
    let body_text = extract_message_body(&msg["payload"]);

    let truncated = if body_text.len() > 20_000 {
        format!("{}...\n[truncated, {} total chars]", &body_text[..20_000], body_text.len())
    } else {
        body_text
    };

    Ok(format!(
        "# {}\n\nFrom: {}\nTo: {}\nDate: {}\nMessage ID: {}\n\n---\n\n{}",
        subject, from, to, date, message_id, truncated
    ))
}

/// Recursively extract plain text body from Gmail message payload.
fn extract_message_body(payload: &serde_json::Value) -> String {
    // Try direct body data
    if let Some(body_data) = payload["body"]["data"].as_str() {
        if let Ok(decoded) = base64_url_decode(body_data) {
            if let Ok(text) = String::from_utf8(decoded) {
                return text;
            }
        }
    }

    // Try parts — prefer text/plain, fall back to text/html
    if let Some(parts) = payload["parts"].as_array() {
        // First pass: look for text/plain
        for part in parts {
            let mime = part["mimeType"].as_str().unwrap_or("");
            if mime == "text/plain" {
                if let Some(body_data) = part["body"]["data"].as_str() {
                    if let Ok(decoded) = base64_url_decode(body_data) {
                        if let Ok(text) = String::from_utf8(decoded) {
                            return text;
                        }
                    }
                }
            }
            // Recurse into multipart
            let nested = extract_message_body(part);
            if !nested.is_empty() {
                return nested;
            }
        }
        // Second pass: try text/html
        for part in parts {
            let mime = part["mimeType"].as_str().unwrap_or("");
            if mime == "text/html" {
                if let Some(body_data) = part["body"]["data"].as_str() {
                    if let Ok(decoded) = base64_url_decode(body_data) {
                        if let Ok(text) = String::from_utf8(decoded) {
                            return format!("[HTML content]\n{}", text);
                        }
                    }
                }
            }
        }
    }

    String::new()
}

async fn exec_gmail_send(args: &serde_json::Value, creds: &HashMap<String, String>) -> EngineResult<String> {
    let token = get_access_token(creds).await?;
    let to = args["to"].as_str().ok_or("Missing 'to' address")?;
    let subject = args["subject"].as_str().ok_or("Missing 'subject'")?;
    let body = args["body"].as_str().ok_or("Missing 'body'")?;
    let cc = args["cc"].as_str().unwrap_or("");
    let is_html = args["html"].as_bool().unwrap_or(false);
    let delegate = creds.get("DELEGATE_EMAIL").map(|s| s.as_str()).unwrap_or("me");
    let user = if delegate == "me" || delegate.is_empty() { "me" } else { delegate };

    let content_type = if is_html { "text/html" } else { "text/plain" };

    // Build RFC 2822 message
    let mut raw_msg = format!(
        "To: {}\r\nSubject: {}\r\nContent-Type: {}; charset=utf-8\r\n",
        to, subject, content_type
    );
    if !cc.is_empty() {
        raw_msg.push_str(&format!("Cc: {}\r\n", cc));
    }
    raw_msg.push_str(&format!("\r\n{}", body));

    let encoded = base64_url_encode(raw_msg.as_bytes());

    let url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/{}/messages/send",
        user
    );

    let send_body = serde_json::json!({ "raw": encoded });
    let (status, resp) = google_request("POST", &url, &token, Some(&send_body)).await?;

    let msg_id = resp["id"].as_str().unwrap_or("unknown");
    info!("[google] Gmail sent: {} → {} (msg_id={})", user, to, msg_id);

    Ok(format!(
        "✅ Email sent successfully!\n  To: {}\n  Subject: {}\n  Message ID: {}\n  Status: {}",
        to, subject, msg_id, status
    ))
}

// ── Calendar implementations ───────────────────────────────────────────────

async fn exec_calendar_list(args: &serde_json::Value, creds: &HashMap<String, String>) -> EngineResult<String> {
    let token = get_access_token(creds).await?;
    let calendar_id = args["calendar_id"].as_str().unwrap_or("primary");
    let max_results = args["max_results"].as_u64().unwrap_or(20).min(100);

    let now_str = chrono::Utc::now().to_rfc3339();
    let time_min = args["time_min"].as_str().unwrap_or(&now_str);
    let default_max = (chrono::Utc::now() + chrono::Duration::days(7)).to_rfc3339();
    let time_max = args["time_max"].as_str().unwrap_or(&default_max);

    let url = format!(
        "https://www.googleapis.com/calendar/v3/calendars/{}/events?timeMin={}&timeMax={}&maxResults={}&singleEvents=true&orderBy=startTime",
        url_encode(calendar_id), url_encode(time_min),
        url_encode(time_max), max_results
    );

    let (_, resp) = google_request("GET", &url, &token, None).await?;
    let events = resp["items"].as_array();

    if events.is_none() || events.unwrap().is_empty() {
        return Ok("No upcoming events found.".into());
    }

    let mut output = format!("# Calendar — {} events\n\n", events.unwrap().len());
    for event in events.unwrap() {
        let summary = event["summary"].as_str().unwrap_or("(No title)");
        let start = event["start"]["dateTime"].as_str()
            .or_else(|| event["start"]["date"].as_str())
            .unwrap_or("?");
        let end = event["end"]["dateTime"].as_str()
            .or_else(|| event["end"]["date"].as_str())
            .unwrap_or("?");
        let location = event["location"].as_str().unwrap_or("");
        let event_id = event["id"].as_str().unwrap_or("");
        let attendees = event["attendees"].as_array()
            .map(|a| a.iter()
                .filter_map(|att| att["email"].as_str())
                .collect::<Vec<_>>()
                .join(", "))
            .unwrap_or_default();

        output.push_str(&format!(
            "**{}** (id: `{}`)\n  {} → {}\n",
            summary, event_id, start, end
        ));
        if !location.is_empty() {
            output.push_str(&format!("  📍 {}\n", location));
        }
        if !attendees.is_empty() {
            output.push_str(&format!("  👥 {}\n", attendees));
        }
        output.push('\n');
    }

    Ok(output)
}

async fn exec_calendar_create(args: &serde_json::Value, creds: &HashMap<String, String>) -> EngineResult<String> {
    let token = get_access_token(creds).await?;
    let calendar_id = args["calendar_id"].as_str().unwrap_or("primary");

    let summary = args["summary"].as_str().ok_or("Missing 'summary'")?;
    let start = args["start"].as_str().ok_or("Missing 'start' datetime")?;
    let end = args["end"].as_str().ok_or("Missing 'end' datetime")?;

    let mut event_body = serde_json::json!({
        "summary": summary,
        "start": { "dateTime": start },
        "end": { "dateTime": end },
    });

    if let Some(desc) = args["description"].as_str() {
        event_body["description"] = serde_json::json!(desc);
    }
    if let Some(loc) = args["location"].as_str() {
        event_body["location"] = serde_json::json!(loc);
    }
    if let Some(attendees) = args["attendees"].as_array() {
        let att: Vec<serde_json::Value> = attendees.iter()
            .filter_map(|a| a.as_str().map(|email| serde_json::json!({"email": email})))
            .collect();
        event_body["attendees"] = serde_json::json!(att);
    }

    let url = format!(
        "https://www.googleapis.com/calendar/v3/calendars/{}/events?sendUpdates=all",
        url_encode(calendar_id)
    );

    let (_, resp) = google_request("POST", &url, &token, Some(&event_body)).await?;
    let event_id = resp["id"].as_str().unwrap_or("unknown");
    let html_link = resp["htmlLink"].as_str().unwrap_or("");

    info!("[google] Calendar event created: {} ({})", summary, event_id);

    Ok(format!(
        "✅ Event created!\n  Title: {}\n  Start: {}\n  End: {}\n  ID: {}\n  Link: {}",
        summary, start, end, event_id, html_link
    ))
}

// ── Drive implementations ──────────────────────────────────────────────────

async fn exec_drive_list(args: &serde_json::Value, creds: &HashMap<String, String>) -> EngineResult<String> {
    let token = get_access_token(creds).await?;
    let max_results = args["max_results"].as_u64().unwrap_or(20).min(100);

    let mut q_parts: Vec<String> = Vec::new();
    if let Some(query) = args["query"].as_str() {
        q_parts.push(query.to_string());
    }
    if let Some(folder_id) = args["folder_id"].as_str() {
        q_parts.push(format!("'{}' in parents", folder_id));
    }
    q_parts.push("trashed = false".to_string());

    let q = q_parts.join(" and ");
    let url = format!(
        "https://www.googleapis.com/drive/v3/files?q={}&pageSize={}&fields=files(id,name,mimeType,size,modifiedTime,webViewLink)&orderBy=modifiedTime desc",
        url_encode(&q), max_results
    );

    let (_, resp) = google_request("GET", &url, &token, None).await?;
    let files = resp["files"].as_array();

    if files.is_none() || files.unwrap().is_empty() {
        return Ok("No files found.".into());
    }

    let mut output = format!("# Drive — {} files\n\n", files.unwrap().len());
    for file in files.unwrap() {
        let name = file["name"].as_str().unwrap_or("?");
        let id = file["id"].as_str().unwrap_or("?");
        let mime = file["mimeType"].as_str().unwrap_or("?");
        let modified = file["modifiedTime"].as_str().unwrap_or("?");
        let size = file["size"].as_str()
            .and_then(|s| s.parse::<u64>().ok())
            .map(format_bytes)
            .unwrap_or_else(|| "-".to_string());
        let link = file["webViewLink"].as_str().unwrap_or("");

        let kind = match mime {
            "application/vnd.google-apps.document" => "📄 Doc",
            "application/vnd.google-apps.spreadsheet" => "📊 Sheet",
            "application/vnd.google-apps.presentation" => "📽️ Slides",
            "application/vnd.google-apps.folder" => "📁 Folder",
            "application/pdf" => "📕 PDF",
            _ if mime.starts_with("image/") => "🖼️ Image",
            _ => "📎 File",
        };

        output.push_str(&format!(
            "{} **{}** (id: `{}`)\n  Type: {} | Size: {} | Modified: {}\n",
            kind, name, id, mime, size, modified
        ));
        if !link.is_empty() {
            output.push_str(&format!("  Link: {}\n", link));
        }
        output.push('\n');
    }

    Ok(output)
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 { return format!("{} B", bytes); }
    if bytes < 1_048_576 { return format!("{:.1} KB", bytes as f64 / 1024.0); }
    if bytes < 1_073_741_824 { return format!("{:.1} MB", bytes as f64 / 1_048_576.0); }
    format!("{:.2} GB", bytes as f64 / 1_073_741_824.0)
}

async fn exec_drive_read(args: &serde_json::Value, creds: &HashMap<String, String>) -> EngineResult<String> {
    let token = get_access_token(creds).await?;
    let file_id = args["file_id"].as_str().ok_or("Missing 'file_id'")?;
    let export_format = args["export_format"].as_str().unwrap_or("text/plain");

    // First get file metadata to determine type
    let meta_url = format!(
        "https://www.googleapis.com/drive/v3/files/{}?fields=name,mimeType,size",
        file_id
    );
    let (_, meta) = google_request("GET", &meta_url, &token, None).await?;
    let name = meta["name"].as_str().unwrap_or("unknown");
    let mime = meta["mimeType"].as_str().unwrap_or("");

    // Google Workspace documents need to be exported, regular files can be downloaded
    let is_google_doc = mime.starts_with("application/vnd.google-apps.");

    let content = if is_google_doc {
        // Export Google Docs/Sheets/Slides
        let url = format!(
            "https://www.googleapis.com/drive/v3/files/{}/export?mimeType={}",
            file_id, url_encode(export_format)
        );
        let client = reqwest::Client::new();
        let resp = client.get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| format!("Drive export failed: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("Drive export error ({})", resp.status()).into());
        }
        resp.text().await.unwrap_or_default()
    } else {
        // Download regular file content
        let url = format!(
            "https://www.googleapis.com/drive/v3/files/{}?alt=media",
            file_id
        );
        let client = reqwest::Client::new();
        let resp = client.get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| format!("Drive download failed: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("Drive download error ({})", resp.status()).into());
        }
        resp.text().await.unwrap_or_default()
    };

    let truncated = if content.len() > 30_000 {
        format!("{}...\n[truncated, {} total chars]", &content[..30_000], content.len())
    } else {
        content
    };

    Ok(format!("# {} ({})\n\n{}", name, mime, truncated))
}

// ── Sheets implementations ─────────────────────────────────────────────────

async fn exec_sheets_read(args: &serde_json::Value, creds: &HashMap<String, String>) -> EngineResult<String> {
    let token = get_access_token(creds).await?;
    let spreadsheet_id = args["spreadsheet_id"].as_str().ok_or("Missing 'spreadsheet_id'")?;
    let range = args["range"].as_str().ok_or("Missing 'range'")?;

    let url = format!(
        "https://sheets.googleapis.com/v4/spreadsheets/{}/values/{}",
        spreadsheet_id, url_encode(range)
    );

    let (_, resp) = google_request("GET", &url, &token, None).await?;
    let values = resp["values"].as_array();

    if values.is_none() || values.unwrap().is_empty() {
        return Ok(format!("No data found in range '{}'.", range));
    }

    let rows = values.unwrap();
    let mut output = format!("# Spreadsheet data — {} rows from '{}'\n\n", rows.len(), range);

    // Format as markdown table if we have a header row
    if let Some(header) = rows.first() {
        let headers: Vec<&str> = header.as_array()
            .map(|h| h.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();

        if !headers.is_empty() {
            output.push_str(&format!("| {} |\n", headers.join(" | ")));
            output.push_str(&format!("| {} |\n", headers.iter().map(|_| "---").collect::<Vec<_>>().join(" | ")));

            for row in rows.iter().skip(1) {
                let cells: Vec<String> = row.as_array()
                    .map(|r| r.iter()
                        .map(|v| v.as_str().unwrap_or("").to_string())
                        .collect())
                    .unwrap_or_default();
                // Pad to header length
                let padded: Vec<String> = (0..headers.len())
                    .map(|i| cells.get(i).cloned().unwrap_or_default())
                    .collect();
                output.push_str(&format!("| {} |\n", padded.join(" | ")));
            }
        }
    }

    Ok(output)
}

async fn exec_sheets_append(args: &serde_json::Value, creds: &HashMap<String, String>) -> EngineResult<String> {
    let token = get_access_token(creds).await?;
    let spreadsheet_id = args["spreadsheet_id"].as_str().ok_or("Missing 'spreadsheet_id'")?;
    let range = args["range"].as_str().ok_or("Missing 'range'")?;
    let values = args.get("values").ok_or("Missing 'values' array")?;

    let url = format!(
        "https://sheets.googleapis.com/v4/spreadsheets/{}/values/{}:append?valueInputOption=USER_ENTERED&insertDataOption=INSERT_ROWS",
        spreadsheet_id, url_encode(range)
    );

    let body = serde_json::json!({
        "values": values
    });

    let (_, resp) = google_request("POST", &url, &token, Some(&body)).await?;
    let updated_range = resp["updates"]["updatedRange"].as_str().unwrap_or("?");
    let updated_rows = resp["updates"]["updatedRows"].as_u64().unwrap_or(0);

    Ok(format!(
        "✅ Appended {} row(s) to range '{}'",
        updated_rows, updated_range
    ))
}

// ── Public Gmail API for mail UI ────────────────────────────────────────────

/// Structured Gmail message for the mail UI (not agent output).
#[derive(Debug, Serialize)]
pub struct GmailMessage {
    pub id: String,
    pub from: String,
    pub subject: String,
    pub snippet: String,
    pub date: String,
    pub read: bool,
    pub labels: Vec<String>,
}

/// List Gmail messages — returns structured JSON for the mail UI.
pub async fn gmail_list_json(
    app_handle: &tauri::AppHandle,
    query: Option<&str>,
    max_results: u32,
) -> EngineResult<Vec<GmailMessage>> {
    let creds = super::get_skill_creds("google_workspace", app_handle)?;
    let token = get_access_token(&creds).await?;
    let q = query.unwrap_or("");

    let mut url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/me/messages?maxResults={}",
        max_results.min(50)
    );
    if !q.is_empty() {
        url.push_str(&format!("&q={}", url_encode(q)));
    }

    let (_, list_resp) = google_request("GET", &url, &token, None).await?;

    let msg_ids: Vec<String> = list_resp["messages"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|m| m["id"].as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();

    if msg_ids.is_empty() {
        return Ok(vec![]);
    }

    let mut messages = Vec::with_capacity(msg_ids.len());

    for msg_id in &msg_ids {
        let msg_url = format!(
            "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}?format=metadata&metadataHeaders=From&metadataHeaders=Subject&metadataHeaders=Date",
            msg_id
        );
        if let Ok((_, msg)) = google_request("GET", &msg_url, &token, None).await {
            let headers = msg["payload"]["headers"].as_array();
            let get_hdr = |name: &str| -> String {
                headers.and_then(|h| h.iter()
                    .find(|hdr| hdr["name"].as_str() == Some(name))
                    .and_then(|hdr| hdr["value"].as_str())
                    .map(|s| s.to_string()))
                .unwrap_or_default()
            };

            let label_ids: Vec<String> = msg["labelIds"].as_array()
                .map(|l| l.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();

            let is_read = !label_ids.iter().any(|l| l == "UNREAD");

            messages.push(GmailMessage {
                id: msg_id.clone(),
                from: get_hdr("From"),
                subject: get_hdr("Subject"),
                snippet: msg["snippet"].as_str().unwrap_or("").to_string(),
                date: get_hdr("Date"),
                read: is_read,
                labels: label_ids,
            });
        }
    }

    Ok(messages)
}

/// Read a single Gmail message — returns structured content for the mail UI.
pub async fn gmail_read_json(
    app_handle: &tauri::AppHandle,
    message_id: &str,
) -> EngineResult<serde_json::Value> {
    let creds = super::get_skill_creds("google_workspace", app_handle)?;
    let token = get_access_token(&creds).await?;

    let url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}?format=full",
        message_id
    );

    let (_, msg) = google_request("GET", &url, &token, None).await?;

    let headers = msg["payload"]["headers"].as_array();
    let get_hdr = |name: &str| -> String {
        headers.and_then(|h| h.iter()
            .find(|hdr| hdr["name"].as_str() == Some(name))
            .and_then(|hdr| hdr["value"].as_str())
            .map(|s| s.to_string()))
        .unwrap_or_default()
    };

    let body = extract_message_body(&msg["payload"]);

    Ok(serde_json::json!({
        "id": message_id,
        "from": get_hdr("From"),
        "to": get_hdr("To"),
        "subject": get_hdr("Subject"),
        "date": get_hdr("Date"),
        "body": body,
        "snippet": msg["snippet"].as_str().unwrap_or(""),
        "labels": msg["labelIds"],
    }))
}

/// Send a Gmail message — used by the mail UI compose.
pub async fn gmail_send_json(
    app_handle: &tauri::AppHandle,
    to: &str,
    subject: &str,
    body: &str,
) -> EngineResult<String> {
    let creds = super::get_skill_creds("google_workspace", app_handle)?;
    let token = get_access_token(&creds).await?;

    let raw_msg = format!(
        "To: {}\r\nSubject: {}\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{}",
        to, subject, body
    );

    let encoded = base64_url_encode(raw_msg.as_bytes());
    let url = "https://gmail.googleapis.com/gmail/v1/users/me/messages/send";
    let send_body = serde_json::json!({ "raw": encoded });
    let (_, resp) = google_request("POST", url, &token, Some(&send_body)).await?;

    let msg_id = resp["id"].as_str().unwrap_or("unknown").to_string();
    info!("[google] Gmail sent via UI → {} (msg_id={})", to, msg_id);
    Ok(msg_id)
}

// ── Generic Google API ─────────────────────────────────────────────────────

async fn exec_google_api(args: &serde_json::Value, creds: &HashMap<String, String>) -> EngineResult<String> {
    let token = get_access_token(creds).await?;
    let url = args["url"].as_str().ok_or("Missing 'url'")?;
    let method = args["method"].as_str().unwrap_or("GET");
    let body = args.get("body").filter(|b| !b.is_null());

    info!("[google] API call: {} {}", method, url);

    let (status, resp) = google_request(method, url, &token, body).await?;

    let text = serde_json::to_string_pretty(&resp).unwrap_or_default();
    let truncated = if text.len() > 30_000 {
        format!("{}...\n[truncated, {} total bytes]", &text[..30_000], text.len())
    } else {
        text
    };

    Ok(format!("Google API {} {} → {}\n\n{}", method, url, status, truncated))
}
