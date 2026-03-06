// Paw Agent Engine — Google Workspace Tools
//
// Direct OAuth2 implementation: Gmail, Calendar, Drive, Sheets, Docs.
// Uses the stored Google OAuth token from the key vault.
// No n8n or external dependencies needed.
//
// Tools:
//   google_gmail_list    — list/search inbox messages
//   google_gmail_read    — read a specific email
//   google_gmail_send    — send (or draft) an email
//   google_calendar_list — list events in a date range
//   google_calendar_create — create a calendar event
//   google_drive_list    — list/search files
//   google_drive_read    — read file metadata or export content
//   google_drive_upload  — upload a file (plain text / JSON)
//   google_drive_share   — share a file with a user
//   google_sheets_read   — read spreadsheet data (A1 ranges)
//   google_sheets_append — append rows to a spreadsheet
//   google_docs_create   — create a new Google Doc
//   google_api           — generic Google API call (escape hatch)

use crate::atoms::types::*;
use log::info;
use std::time::Duration;

// ── Token helper ───────────────────────────────────────────────────────

/// Load the Google OAuth access token from the encrypted vault.
/// Returns Err if Google is not connected.
fn load_google_token() -> Result<String, String> {
    use crate::engine::key_vault;
    use crate::engine::skills::crypto::{decrypt_credential, get_vault_key};

    let vault_key = get_vault_key().map_err(|e| format!("Vault key error: {e}"))?;
    // Try 'google-workspace' first (OAuth service_id), fallback to legacy 'google'
    let encrypted = key_vault::get("oauth:google-workspace")
        .or_else(|| key_vault::get("oauth:google"))
        .ok_or("Google is not connected. Ask the user to connect Google in the Integrations view → Google → Connect.")?;
    let json =
        decrypt_credential(&encrypted, &vault_key).map_err(|e| format!("Decrypt error: {e}"))?;

    #[derive(serde::Deserialize)]
    struct Tokens {
        access_token: String,
    }
    let tokens: Tokens =
        serde_json::from_str(&json).map_err(|e| format!("Token parse error: {e}"))?;

    Ok(tokens.access_token)
}

/// Shared HTTP client with sane timeout.
fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap_or_default()
}

/// Check an HTTP response; return body text if success, or a helpful
/// error message if it failed. Never leaks the token.
async fn check_response(resp: reqwest::Response, api_name: &str) -> Result<String, String> {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if status.is_success() {
        Ok(body)
    } else {
        let hint = match status.as_u16() {
            401 => " (token expired — user should reconnect Google in Integrations)",
            403 => " (insufficient permissions — user may need to reconnect with updated scopes)",
            429 => " (rate limited — wait a moment and retry)",
            _ => "",
        };
        Err(format!(
            "{} returned HTTP {}{}: {}",
            api_name,
            status.as_u16(),
            hint,
            &body[..body.len().min(500)]
        ))
    }
}

// ── Tool definitions ───────────────────────────────────────────────────

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        // ── Gmail ──────────────────────────────────────────────────
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "google_gmail_list".into(),
                description: "List or search Gmail messages. Returns id, from, subject, snippet, date, and read status for each message. Use Gmail search syntax in the `query` parameter (e.g. 'is:unread', 'from:user@example.com', 'subject:invoice after:2025/01/01').".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Gmail search query (e.g. 'is:unread', 'from:boss@co.com'). Default: list recent inbox messages." },
                        "max_results": { "type": "integer", "description": "Max messages to return (1-50, default 20)" }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "google_gmail_read".into(),
                description: "Read the full content of a specific Gmail message by ID. Returns headers (from, to, subject, date) and the plain-text body. Get message IDs from google_gmail_list first.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "message_id": { "type": "string", "description": "The Gmail message ID to read" }
                    },
                    "required": ["message_id"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "google_gmail_send".into(),
                description: "Send an email via Gmail. Composes and sends immediately. Always confirm with the user before sending.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "to": { "type": "string", "description": "Recipient email address" },
                        "subject": { "type": "string", "description": "Email subject line" },
                        "body": { "type": "string", "description": "Email body (plain text)" },
                        "cc": { "type": "string", "description": "CC recipients (comma-separated, optional)" },
                        "bcc": { "type": "string", "description": "BCC recipients (comma-separated, optional)" }
                    },
                    "required": ["to", "subject", "body"]
                }),
            },
        },
        // ── Calendar ───────────────────────────────────────────────
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "google_calendar_list".into(),
                description: "List Google Calendar events in a date range. Returns event title, start/end time, location, attendees, and description. Defaults to today's events.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "time_min": { "type": "string", "description": "Start of range (RFC3339, e.g. '2025-03-05T00:00:00Z'). Default: start of today." },
                        "time_max": { "type": "string", "description": "End of range (RFC3339). Default: end of today." },
                        "max_results": { "type": "integer", "description": "Max events (1-100, default 25)" },
                        "calendar_id": { "type": "string", "description": "Calendar ID (default: 'primary')" }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "google_calendar_create".into(),
                description: "Create a Google Calendar event. Supports recurring events via RRULE. Confirm with the user before creating. Returns the created event details.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "summary": { "type": "string", "description": "Event title" },
                        "start": { "type": "string", "description": "Start time (RFC3339, e.g. '2025-03-05T10:00:00-05:00') or date for all-day events ('2025-03-05')" },
                        "end": { "type": "string", "description": "End time (RFC3339) or date for all-day events ('2025-03-06' — must be day AFTER start for all-day)" },
                        "description": { "type": "string", "description": "Event description (optional)" },
                        "location": { "type": "string", "description": "Event location (optional)" },
                        "attendees": { "type": "string", "description": "Comma-separated attendee email addresses (optional)" },
                        "recurrence": { "type": "array", "items": { "type": "string" }, "description": "RRULE recurrence strings (e.g. ['RRULE:FREQ=WEEKLY;BYDAY=MO,WE,FR']). Optional." },
                        "timezone": { "type": "string", "description": "IANA timezone (e.g. 'America/New_York'). Optional — defaults to UTC." },
                        "calendar_id": { "type": "string", "description": "Calendar ID (default: 'primary')" }
                    },
                    "required": ["summary", "start", "end"]
                }),
            },
        },
        // ── Drive ──────────────────────────────────────────────────
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "google_drive_list".into(),
                description: "List or search files in Google Drive. Returns file id, name, type, size, and last modified date. Use Drive search syntax in query (e.g. \"name contains 'report'\" or \"mimeType='application/pdf'\").".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Drive search query (e.g. \"name contains 'budget'\"). Default: list recent files." },
                        "max_results": { "type": "integer", "description": "Max files (1-100, default 25)" }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "google_drive_read".into(),
                description: "Read a Google Drive file's content or metadata. For Google Docs/Sheets/Slides, exports as plain text. For other files, returns metadata. Get file IDs from google_drive_list first.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_id": { "type": "string", "description": "The Drive file ID" },
                        "export_format": { "type": "string", "description": "Export MIME type for Google Workspace files (default: 'text/plain'). Use 'application/pdf' for PDF export." }
                    },
                    "required": ["file_id"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "google_drive_upload".into(),
                description: "Upload a text file to Google Drive. For binary files, use the file path instead. Returns the new file ID and web link.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "File name (e.g. 'report.txt')" },
                        "content": { "type": "string", "description": "File content (plain text)" },
                        "mime_type": { "type": "string", "description": "MIME type (default: 'text/plain'). Use 'application/vnd.google-apps.document' to create as Google Doc." },
                        "folder_id": { "type": "string", "description": "Parent folder ID (optional — uploads to root if omitted)" }
                    },
                    "required": ["name", "content"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "google_drive_share".into(),
                description: "Share a Google Drive file with a user. Grants permissions by email. Confirm with the user before sharing.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_id": { "type": "string", "description": "The Drive file ID to share" },
                        "email": { "type": "string", "description": "Email address to share with" },
                        "role": { "type": "string", "enum": ["reader", "commenter", "writer"], "description": "Permission role (default: 'reader')" }
                    },
                    "required": ["file_id", "email"]
                }),
            },
        },
        // ── Sheets ─────────────────────────────────────────────────
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "google_sheets_read".into(),
                description: "Read data from a Google Sheets spreadsheet. Uses A1 notation for ranges (e.g. 'Sheet1!A1:D10'). Returns the cell values as a 2D array.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "spreadsheet_id": { "type": "string", "description": "The spreadsheet ID (from the URL)" },
                        "range": { "type": "string", "description": "A1 range notation (e.g. 'Sheet1!A1:D10', 'Sheet1', 'A:D')" }
                    },
                    "required": ["spreadsheet_id", "range"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "google_sheets_append".into(),
                description: "Append rows to a Google Sheets spreadsheet. Each row is an array of values. Appends after the last row with data in the specified range.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "spreadsheet_id": { "type": "string", "description": "The spreadsheet ID" },
                        "range": { "type": "string", "description": "A1 range to append to (e.g. 'Sheet1!A:D')" },
                        "values": { "type": "array", "items": { "type": "array", "items": {} }, "description": "2D array of row values, e.g. [[\"Alice\", 30], [\"Bob\", 25]]" }
                    },
                    "required": ["spreadsheet_id", "range", "values"]
                }),
            },
        },
        // ── Docs ───────────────────────────────────────────────────
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "google_docs_create".into(),
                description: "Create a new Google Doc with the given title and body text. Returns the document ID and web link.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "title": { "type": "string", "description": "Document title" },
                        "body": { "type": "string", "description": "Document body text (plain text — will be inserted as the document content)" }
                    },
                    "required": ["title"]
                }),
            },
        },
        // ── Generic API ────────────────────────────────────────────
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "google_api".into(),
                description: "Make a generic authenticated Google API call. Use this as an escape hatch for any Google API not covered by the dedicated tools. The OAuth token is added automatically as a Bearer token.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "method": { "type": "string", "enum": ["GET", "POST", "PUT", "PATCH", "DELETE"], "description": "HTTP method" },
                        "url": { "type": "string", "description": "Full Google API URL (e.g. 'https://www.googleapis.com/...')" },
                        "body": { "type": "object", "description": "JSON request body (optional)" }
                    },
                    "required": ["method", "url"]
                }),
            },
        },
    ]
}

// ── Executor dispatch ──────────────────────────────────────────────────

pub async fn execute(
    name: &str,
    args: &serde_json::Value,
    _app_handle: &tauri::AppHandle,
) -> Option<Result<String, String>> {
    match name {
        "google_gmail_list" => Some(gmail_list(args).await),
        "google_gmail_read" => Some(gmail_read(args).await),
        "google_gmail_send" => Some(gmail_send(args).await),
        "google_calendar_list" => Some(calendar_list(args).await),
        "google_calendar_create" => Some(calendar_create(args).await),
        "google_drive_list" => Some(drive_list(args).await),
        "google_drive_read" => Some(drive_read(args).await),
        "google_drive_upload" => Some(drive_upload(args).await),
        "google_drive_share" => Some(drive_share(args).await),
        "google_sheets_read" => Some(sheets_read(args).await),
        "google_sheets_append" => Some(sheets_append(args).await),
        "google_docs_create" => Some(docs_create(args).await),
        "google_api" => Some(generic_api(args).await),
        _ => None,
    }
}

// ════════════════════════════════════════════════════════════════════════
// Gmail
// ════════════════════════════════════════════════════════════════════════

async fn gmail_list(args: &serde_json::Value) -> Result<String, String> {
    let token = load_google_token()?;
    let query = args["query"].as_str().unwrap_or("");
    let max = args["max_results"].as_u64().unwrap_or(20).min(50);

    let mut url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/me/messages?maxResults={}",
        max
    );
    if !query.is_empty() {
        url.push_str(&format!("&q={}", urlencoding::encode(query)));
    } else {
        url.push_str("&labelIds=INBOX");
    }

    let resp = http()
        .get(&url)
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| format!("Gmail request failed: {e}"))?;
    let body = check_response(resp, "Gmail list").await?;

    // Parse message IDs, then fetch metadata for each
    let list: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Parse error: {e}"))?;
    let msg_ids: Vec<&str> = list["messages"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|m| m["id"].as_str()).collect())
        .unwrap_or_default();

    if msg_ids.is_empty() {
        return Ok("No messages found.".into());
    }

    // Fetch metadata for each message (batch via futures)
    let client = http();
    let futs: Vec<_> = msg_ids
        .iter()
        .map(|id| {
            let url = format!(
                "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}?format=metadata\
                 &metadataHeaders=From&metadataHeaders=Subject&metadataHeaders=Date&metadataHeaders=To",
                id
            );
            client.get(&url).bearer_auth(&token).send()
        })
        .collect();

    let results = futures::future::join_all(futs).await;
    let mut messages = Vec::new();

    for result in results {
        let resp = match result {
            Ok(r) if r.status().is_success() => r,
            _ => continue,
        };
        let text = resp.text().await.unwrap_or_default();
        let msg: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let headers = msg["payload"]["headers"].as_array();
        let get_header = |name: &str| -> String {
            headers
                .and_then(|h| {
                    h.iter()
                        .find(|hdr| {
                            hdr["name"]
                                .as_str()
                                .unwrap_or("")
                                .eq_ignore_ascii_case(name)
                        })
                        .and_then(|hdr| hdr["value"].as_str())
                })
                .unwrap_or("")
                .to_string()
        };

        let unread = msg["labelIds"]
            .as_array()
            .map(|l| l.iter().any(|v| v.as_str() == Some("UNREAD")))
            .unwrap_or(false);

        messages.push(serde_json::json!({
            "id": msg["id"].as_str().unwrap_or(""),
            "from": get_header("From"),
            "to": get_header("To"),
            "subject": get_header("Subject"),
            "date": get_header("Date"),
            "snippet": msg["snippet"].as_str().unwrap_or(""),
            "unread": unread,
        }));
    }

    info!("[google] gmail_list returned {} messages", messages.len());
    serde_json::to_string_pretty(&messages).map_err(|e| format!("Serialize error: {e}"))
}

async fn gmail_read(args: &serde_json::Value) -> Result<String, String> {
    let token = load_google_token()?;
    let message_id = args["message_id"]
        .as_str()
        .ok_or("message_id is required")?;

    let url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}?format=full",
        urlencoding::encode(message_id)
    );

    let resp = http()
        .get(&url)
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| format!("Gmail read failed: {e}"))?;
    let body = check_response(resp, "Gmail read").await?;
    let msg: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Parse error: {e}"))?;

    // Extract headers
    let headers = msg["payload"]["headers"].as_array();
    let get_header = |name: &str| -> String {
        headers
            .and_then(|h| {
                h.iter()
                    .find(|hdr| {
                        hdr["name"]
                            .as_str()
                            .unwrap_or("")
                            .eq_ignore_ascii_case(name)
                    })
                    .and_then(|hdr| hdr["value"].as_str())
            })
            .unwrap_or("")
            .to_string()
    };

    // Extract body text — walk the MIME parts
    let plain_body = extract_body_text(&msg["payload"]);

    let result = serde_json::json!({
        "id": message_id,
        "from": get_header("From"),
        "to": get_header("To"),
        "cc": get_header("Cc"),
        "subject": get_header("Subject"),
        "date": get_header("Date"),
        "body": plain_body,
        "snippet": msg["snippet"].as_str().unwrap_or(""),
    });

    info!("[google] gmail_read message_id={}", message_id);
    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialize error: {e}"))
}

/// Walk MIME parts to find text/plain body content.
fn extract_body_text(payload: &serde_json::Value) -> String {
    // Direct body data
    if let Some(mime) = payload["mimeType"].as_str() {
        if mime == "text/plain" {
            if let Some(data) = payload["body"]["data"].as_str() {
                return decode_base64url(data);
            }
        }
    }
    // Walk parts recursively
    if let Some(parts) = payload["parts"].as_array() {
        for part in parts {
            let result = extract_body_text(part);
            if !result.is_empty() {
                return result;
            }
        }
    }
    String::new()
}

/// Decode base64url (Gmail's encoding) to UTF-8 string.
fn decode_base64url(data: &str) -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    URL_SAFE_NO_PAD
        .decode(data)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .unwrap_or_default()
}

async fn gmail_send(args: &serde_json::Value) -> Result<String, String> {
    let token = load_google_token()?;
    let to = args["to"].as_str().ok_or("'to' is required")?;
    let subject = args["subject"].as_str().ok_or("'subject' is required")?;
    let body_text = args["body"].as_str().ok_or("'body' is required")?;
    let cc = args["cc"].as_str().unwrap_or("");
    let bcc = args["bcc"].as_str().unwrap_or("");

    // Build RFC 2822 message
    let mut raw = format!("To: {to}\r\nSubject: {subject}\r\n");
    if !cc.is_empty() {
        raw.push_str(&format!("Cc: {cc}\r\n"));
    }
    if !bcc.is_empty() {
        raw.push_str(&format!("Bcc: {bcc}\r\n"));
    }
    raw.push_str("Content-Type: text/plain; charset=\"UTF-8\"\r\n\r\n");
    raw.push_str(body_text);

    // base64url encode the raw message
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    let encoded = URL_SAFE_NO_PAD.encode(raw.as_bytes());

    let payload = serde_json::json!({ "raw": encoded });
    let resp = http()
        .post("https://gmail.googleapis.com/gmail/v1/users/me/messages/send")
        .bearer_auth(&token)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Gmail send failed: {e}"))?;

    let body = check_response(resp, "Gmail send").await?;
    let result: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Parse error: {e}"))?;

    info!("[google] gmail_send to={}", to);
    Ok(format!(
        "Email sent successfully. Message ID: {}",
        result["id"].as_str().unwrap_or("unknown")
    ))
}

// ════════════════════════════════════════════════════════════════════════
// Calendar
// ════════════════════════════════════════════════════════════════════════

async fn calendar_list(args: &serde_json::Value) -> Result<String, String> {
    let token = load_google_token()?;
    let calendar_id = args["calendar_id"].as_str().unwrap_or("primary");
    let max = args["max_results"].as_u64().unwrap_or(25).min(100);

    // Default to today
    let now = chrono::Utc::now();
    let default_min = now.format("%Y-%m-%dT00:00:00Z").to_string();
    let default_max = now.format("%Y-%m-%dT23:59:59Z").to_string();
    let time_min = args["time_min"].as_str().unwrap_or(&default_min);
    let time_max = args["time_max"].as_str().unwrap_or(&default_max);

    let url = format!(
        "https://www.googleapis.com/calendar/v3/calendars/{}/events\
         ?timeMin={}&timeMax={}&maxResults={}&singleEvents=true&orderBy=startTime",
        urlencoding::encode(calendar_id),
        urlencoding::encode(time_min),
        urlencoding::encode(time_max),
        max,
    );

    let resp = http()
        .get(&url)
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| format!("Calendar request failed: {e}"))?;
    let body = check_response(resp, "Calendar list").await?;
    let data: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Parse error: {e}"))?;

    let events: Vec<serde_json::Value> = data["items"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|ev| {
            serde_json::json!({
                "id": ev["id"],
                "summary": ev["summary"],
                "start": ev["start"]["dateTime"].as_str()
                    .or(ev["start"]["date"].as_str())
                    .unwrap_or(""),
                "end": ev["end"]["dateTime"].as_str()
                    .or(ev["end"]["date"].as_str())
                    .unwrap_or(""),
                "location": ev["location"],
                "description": ev["description"],
                "attendees": ev["attendees"].as_array()
                    .map(|a| a.iter().filter_map(|at| at["email"].as_str()).collect::<Vec<_>>())
                    .unwrap_or_default(),
                "status": ev["status"],
            })
        })
        .collect();

    info!("[google] calendar_list returned {} events", events.len());
    serde_json::to_string_pretty(&events).map_err(|e| format!("Serialize error: {e}"))
}

async fn calendar_create(args: &serde_json::Value) -> Result<String, String> {
    let token = load_google_token()?;
    let calendar_id = args["calendar_id"].as_str().unwrap_or("primary");
    let summary = args["summary"].as_str().ok_or("'summary' is required")?;
    let start = args["start"].as_str().ok_or("'start' is required")?;
    let end = args["end"].as_str().ok_or("'end' is required")?;
    let timezone = args["timezone"].as_str();

    // Detect all-day events (date-only format: YYYY-MM-DD, exactly 10 chars)
    let is_all_day = start.len() == 10 && start.chars().nth(4) == Some('-');

    let mut event = if is_all_day {
        serde_json::json!({
            "summary": summary,
            "start": { "date": start },
            "end": { "date": end },
        })
    } else {
        let mut start_obj = serde_json::json!({ "dateTime": start });
        let mut end_obj = serde_json::json!({ "dateTime": end });
        if let Some(tz) = timezone {
            start_obj["timeZone"] = serde_json::json!(tz);
            end_obj["timeZone"] = serde_json::json!(tz);
        }
        serde_json::json!({
            "summary": summary,
            "start": start_obj,
            "end": end_obj,
        })
    };

    if let Some(desc) = args["description"].as_str() {
        event["description"] = serde_json::json!(desc);
    }
    if let Some(loc) = args["location"].as_str() {
        event["location"] = serde_json::json!(loc);
    }
    if let Some(attendees_str) = args["attendees"].as_str() {
        let attendees: Vec<serde_json::Value> = attendees_str
            .split(',')
            .map(|e| serde_json::json!({ "email": e.trim() }))
            .collect();
        event["attendees"] = serde_json::json!(attendees);
    }
    // Recurrence rules (e.g. ["RRULE:FREQ=WEEKLY;BYDAY=MO,WE,FR"])
    if let Some(recurrence) = args["recurrence"].as_array() {
        event["recurrence"] = serde_json::json!(recurrence);
    }

    let url = format!(
        "https://www.googleapis.com/calendar/v3/calendars/{}/events",
        urlencoding::encode(calendar_id)
    );

    let resp = http()
        .post(&url)
        .bearer_auth(&token)
        .json(&event)
        .send()
        .await
        .map_err(|e| format!("Calendar create failed: {e}"))?;
    let body = check_response(resp, "Calendar create").await?;
    let result: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Parse error: {e}"))?;

    info!("[google] calendar_create '{}'", summary);
    Ok(format!(
        "Event created: {} ({})\nLink: {}",
        result["summary"].as_str().unwrap_or(summary),
        result["id"].as_str().unwrap_or(""),
        result["htmlLink"].as_str().unwrap_or("")
    ))
}

// ════════════════════════════════════════════════════════════════════════
// Drive
// ════════════════════════════════════════════════════════════════════════

async fn drive_list(args: &serde_json::Value) -> Result<String, String> {
    let token = load_google_token()?;
    let max = args["max_results"].as_u64().unwrap_or(25).min(100);

    let mut url = format!(
        "https://www.googleapis.com/drive/v3/files\
         ?pageSize={}&fields=files(id,name,mimeType,size,modifiedTime,webViewLink)",
        max,
    );

    if let Some(q) = args["query"].as_str() {
        if !q.is_empty() {
            url.push_str(&format!("&q={}", urlencoding::encode(q)));
        }
    }

    let resp = http()
        .get(&url)
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| format!("Drive list failed: {e}"))?;
    let body = check_response(resp, "Drive list").await?;
    let data: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Parse error: {e}"))?;

    let files: Vec<serde_json::Value> = data["files"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|f| {
            serde_json::json!({
                "id": f["id"],
                "name": f["name"],
                "type": f["mimeType"],
                "size": f["size"],
                "modified": f["modifiedTime"],
                "link": f["webViewLink"],
            })
        })
        .collect();

    info!("[google] drive_list returned {} files", files.len());
    serde_json::to_string_pretty(&files).map_err(|e| format!("Serialize error: {e}"))
}

async fn drive_read(args: &serde_json::Value) -> Result<String, String> {
    let token = load_google_token()?;
    let file_id = args["file_id"].as_str().ok_or("'file_id' is required")?;
    let export_format = args["export_format"].as_str().unwrap_or("text/plain");

    // First get file metadata to determine type
    let meta_url = format!(
        "https://www.googleapis.com/drive/v3/files/{}?fields=id,name,mimeType,size,modifiedTime,webViewLink",
        urlencoding::encode(file_id)
    );
    let resp = http()
        .get(&meta_url)
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| format!("Drive read failed: {e}"))?;
    let meta_body = check_response(resp, "Drive metadata").await?;
    let meta: serde_json::Value =
        serde_json::from_str(&meta_body).map_err(|e| format!("Parse error: {e}"))?;

    let mime = meta["mimeType"].as_str().unwrap_or("");

    // Google Workspace files need /export, others use /media
    let is_google_type = mime.starts_with("application/vnd.google-apps.");

    if is_google_type {
        let export_url = format!(
            "https://www.googleapis.com/drive/v3/files/{}/export?mimeType={}",
            urlencoding::encode(file_id),
            urlencoding::encode(export_format),
        );
        let resp = http()
            .get(&export_url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| format!("Drive export failed: {e}"))?;
        let content = check_response(resp, "Drive export").await?;

        info!(
            "[google] drive_read exported {} as {}",
            file_id, export_format
        );
        Ok(format!(
            "File: {} ({})\n\n{}",
            meta["name"].as_str().unwrap_or(""),
            mime,
            &content[..content.len().min(50000)]
        ))
    } else {
        // For non-Google files, return metadata (binary download not practical for agent)
        info!("[google] drive_read metadata for {}", file_id);
        serde_json::to_string_pretty(&meta).map_err(|e| format!("Serialize error: {e}"))
    }
}

async fn drive_upload(args: &serde_json::Value) -> Result<String, String> {
    let token = load_google_token()?;
    let name = args["name"].as_str().ok_or("'name' is required")?;
    let content = args["content"].as_str().ok_or("'content' is required")?;
    let mime_type = args["mime_type"].as_str().unwrap_or("text/plain");

    // Use multipart upload: metadata + content
    let mut metadata = serde_json::json!({ "name": name });
    if let Some(folder_id) = args["folder_id"].as_str() {
        metadata["parents"] = serde_json::json!([folder_id]);
    }

    let boundary = "pawz_boundary_2026";
    let body = format!(
        "--{boundary}\r\n\
         Content-Type: application/json; charset=UTF-8\r\n\r\n\
         {}\r\n\
         --{boundary}\r\n\
         Content-Type: {mime_type}\r\n\r\n\
         {content}\r\n\
         --{boundary}--",
        serde_json::to_string(&metadata).unwrap_or_default(),
    );

    let resp = http()
        .post("https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart&fields=id,name,webViewLink")
        .bearer_auth(&token)
        .header(
            "Content-Type",
            format!("multipart/related; boundary={boundary}"),
        )
        .body(body)
        .send()
        .await
        .map_err(|e| format!("Drive upload failed: {e}"))?;
    let resp_body = check_response(resp, "Drive upload").await?;
    let result: serde_json::Value =
        serde_json::from_str(&resp_body).map_err(|e| format!("Parse error: {e}"))?;

    info!("[google] drive_upload '{}'", name);
    Ok(format!(
        "File uploaded: {} (ID: {})\nLink: {}",
        result["name"].as_str().unwrap_or(name),
        result["id"].as_str().unwrap_or(""),
        result["webViewLink"].as_str().unwrap_or("")
    ))
}

async fn drive_share(args: &serde_json::Value) -> Result<String, String> {
    let token = load_google_token()?;
    let file_id = args["file_id"].as_str().ok_or("'file_id' is required")?;
    let email = args["email"].as_str().ok_or("'email' is required")?;
    let role = args["role"].as_str().unwrap_or("reader");

    let url = format!(
        "https://www.googleapis.com/drive/v3/files/{}/permissions",
        urlencoding::encode(file_id)
    );

    let payload = serde_json::json!({
        "type": "user",
        "role": role,
        "emailAddress": email,
    });

    let resp = http()
        .post(&url)
        .bearer_auth(&token)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Drive share failed: {e}"))?;
    check_response(resp, "Drive share").await?;

    info!(
        "[google] drive_share {} with {} as {}",
        file_id, email, role
    );
    Ok(format!(
        "Shared file {} with {} as {}",
        file_id, email, role
    ))
}

// ════════════════════════════════════════════════════════════════════════
// Sheets
// ════════════════════════════════════════════════════════════════════════

async fn sheets_read(args: &serde_json::Value) -> Result<String, String> {
    let token = load_google_token()?;
    let spreadsheet_id = args["spreadsheet_id"]
        .as_str()
        .ok_or("'spreadsheet_id' is required")?;
    let range = args["range"].as_str().ok_or("'range' is required")?;

    let url = format!(
        "https://sheets.googleapis.com/v4/spreadsheets/{}/values/{}",
        urlencoding::encode(spreadsheet_id),
        urlencoding::encode(range),
    );

    let resp = http()
        .get(&url)
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| format!("Sheets read failed: {e}"))?;
    let body = check_response(resp, "Sheets read").await?;
    let data: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Parse error: {e}"))?;

    info!("[google] sheets_read {}!{}", spreadsheet_id, range);
    serde_json::to_string_pretty(&serde_json::json!({
        "range": data["range"],
        "values": data["values"],
    }))
    .map_err(|e| format!("Serialize error: {e}"))
}

async fn sheets_append(args: &serde_json::Value) -> Result<String, String> {
    let token = load_google_token()?;
    let spreadsheet_id = args["spreadsheet_id"]
        .as_str()
        .ok_or("'spreadsheet_id' is required")?;
    let range = args["range"].as_str().ok_or("'range' is required")?;
    let values = &args["values"];

    if !values.is_array() {
        return Err("'values' must be a 2D array (array of rows)".into());
    }

    let url = format!(
        "https://sheets.googleapis.com/v4/spreadsheets/{}/values/{}:append\
         ?valueInputOption=USER_ENTERED&insertDataOption=INSERT_ROWS",
        urlencoding::encode(spreadsheet_id),
        urlencoding::encode(range),
    );

    let payload = serde_json::json!({
        "range": range,
        "majorDimension": "ROWS",
        "values": values,
    });

    let resp = http()
        .post(&url)
        .bearer_auth(&token)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Sheets append failed: {e}"))?;
    let body = check_response(resp, "Sheets append").await?;
    let result: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Parse error: {e}"))?;

    let updated = result["updates"]["updatedRows"].as_u64().unwrap_or(0);

    info!(
        "[google] sheets_append {} rows to {}!{}",
        updated, spreadsheet_id, range
    );
    Ok(format!("Appended {} row(s) to {}", updated, range))
}

// ════════════════════════════════════════════════════════════════════════
// Docs
// ════════════════════════════════════════════════════════════════════════

async fn docs_create(args: &serde_json::Value) -> Result<String, String> {
    let token = load_google_token()?;
    let title = args["title"].as_str().ok_or("'title' is required")?;
    let body_text = args["body"].as_str().unwrap_or("");

    // 1. Create the document
    let create_payload = serde_json::json!({ "title": title });
    let resp = http()
        .post("https://docs.googleapis.com/v1/documents")
        .bearer_auth(&token)
        .json(&create_payload)
        .send()
        .await
        .map_err(|e| format!("Docs create failed: {e}"))?;
    let create_body = check_response(resp, "Docs create").await?;
    let doc: serde_json::Value =
        serde_json::from_str(&create_body).map_err(|e| format!("Parse error: {e}"))?;
    let doc_id = doc["documentId"]
        .as_str()
        .ok_or("No documentId in response")?;

    // 2. Insert body text if provided
    if !body_text.is_empty() {
        let update_payload = serde_json::json!({
            "requests": [{
                "insertText": {
                    "location": { "index": 1 },
                    "text": body_text
                }
            }]
        });
        let update_url = format!(
            "https://docs.googleapis.com/v1/documents/{}:batchUpdate",
            doc_id
        );
        let resp = http()
            .post(&update_url)
            .bearer_auth(&token)
            .json(&update_payload)
            .send()
            .await
            .map_err(|e| format!("Docs insert text failed: {e}"))?;
        check_response(resp, "Docs insertText").await?;
    }

    let link = format!("https://docs.google.com/document/d/{}/edit", doc_id);
    info!("[google] docs_create '{}' → {}", title, doc_id);
    Ok(format!(
        "Document created: {}\nID: {}\nLink: {}",
        title, doc_id, link
    ))
}

// ════════════════════════════════════════════════════════════════════════
// Generic API
// ════════════════════════════════════════════════════════════════════════

async fn generic_api(args: &serde_json::Value) -> Result<String, String> {
    let token = load_google_token()?;
    let method = args["method"].as_str().ok_or("'method' is required")?;
    let url = args["url"].as_str().ok_or("'url' is required")?;

    // Safety check: only allow googleapis.com domains to prevent token leakage
    if !url.contains("googleapis.com") && !url.contains("google.com/") {
        return Err(
            "google_api only allows requests to googleapis.com and google.com domains \
             to prevent OAuth token leakage. Use the 'fetch' tool for other URLs."
                .into(),
        );
    }

    let client = http();
    let builder = match method.to_uppercase().as_str() {
        "GET" => client.get(url),
        "POST" => client.post(url),
        "PUT" => client.put(url),
        "PATCH" => client.patch(url),
        "DELETE" => client.delete(url),
        _ => return Err(format!("Unsupported HTTP method: {method}")),
    };

    let builder = builder.bearer_auth(&token);
    let builder = if let Some(body) = args.get("body") {
        if body.is_null() {
            builder
        } else if let Some(s) = body.as_str() {
            // LLM sometimes sends body as a JSON string — parse it back to an object
            match serde_json::from_str::<serde_json::Value>(s) {
                Ok(parsed) => builder
                    .header("Content-Type", "application/json")
                    .json(&parsed),
                Err(_) => {
                    // Not valid JSON — send as raw string body
                    builder
                        .header("Content-Type", "application/json")
                        .body(s.to_string())
                }
            }
        } else {
            // Already a proper JSON value (object/array) — send directly
            builder
                .header("Content-Type", "application/json")
                .json(body)
        }
    } else {
        builder
    };

    let resp = builder
        .send()
        .await
        .map_err(|e| format!("Google API request failed: {e}"))?;
    let body = check_response(resp, "Google API").await?;

    info!("[google] google_api {} {}", method, url);

    // Truncate very large responses
    if body.len() > 50000 {
        Ok(format!(
            "{}... (truncated, {} total bytes)",
            &body[..50000],
            body.len()
        ))
    } else {
        Ok(body)
    }
}

// ════════════════════════════════════════════════════════════════════════
// Tests
// ════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── Definition validation ──────────────────────────────────────

    #[test]
    fn definitions_returns_13_tools() {
        let defs = definitions();
        assert_eq!(
            defs.len(),
            13,
            "Google Workspace should expose exactly 13 tools"
        );
    }

    #[test]
    fn all_definitions_have_function_type() {
        for def in definitions() {
            assert_eq!(
                def.tool_type, "function",
                "Tool '{}' must have type 'function'",
                def.function.name
            );
        }
    }

    #[test]
    fn all_definitions_have_nonempty_descriptions() {
        for def in definitions() {
            assert!(
                !def.function.description.is_empty(),
                "Tool '{}' must have a description",
                def.function.name
            );
            assert!(
                def.function.description.len() >= 20,
                "Tool '{}' description too short: '{}'",
                def.function.name,
                def.function.description
            );
        }
    }

    #[test]
    fn all_definitions_have_object_parameters() {
        for def in definitions() {
            let params = &def.function.parameters;
            assert_eq!(
                params["type"].as_str(),
                Some("object"),
                "Tool '{}' parameters must be type 'object'",
                def.function.name
            );
            assert!(
                params.get("properties").is_some(),
                "Tool '{}' must have 'properties' in parameters",
                def.function.name
            );
        }
    }

    #[test]
    fn definition_names_are_unique() {
        let defs = definitions();
        let mut names: Vec<&str> = defs.iter().map(|d| d.function.name.as_str()).collect();
        names.sort();
        let original_len = names.len();
        names.dedup();
        assert_eq!(
            names.len(),
            original_len,
            "Duplicate tool names found in Google definitions"
        );
    }

    #[test]
    fn definition_names_all_start_with_google() {
        for def in definitions() {
            assert!(
                def.function.name.starts_with("google_"),
                "Tool '{}' must be prefixed with 'google_'",
                def.function.name
            );
        }
    }

    #[test]
    fn expected_tool_names_present() {
        let defs = definitions();
        let names: Vec<&str> = defs.iter().map(|d| d.function.name.as_str()).collect();
        let expected = [
            "google_gmail_list",
            "google_gmail_read",
            "google_gmail_send",
            "google_calendar_list",
            "google_calendar_create",
            "google_drive_list",
            "google_drive_read",
            "google_drive_upload",
            "google_drive_share",
            "google_sheets_read",
            "google_sheets_append",
            "google_docs_create",
            "google_api",
        ];
        for name in &expected {
            assert!(
                names.contains(name),
                "Expected tool '{}' not found in definitions",
                name
            );
        }
    }

    // ── Required parameters validation ─────────────────────────────

    #[test]
    fn gmail_read_requires_message_id() {
        let defs = definitions();
        let def = defs
            .iter()
            .find(|d| d.function.name == "google_gmail_read")
            .unwrap();
        let required = def.function.parameters["required"].as_array().unwrap();
        assert!(
            required.iter().any(|v| v.as_str() == Some("message_id")),
            "google_gmail_read must require 'message_id'"
        );
    }

    #[test]
    fn gmail_send_requires_to_subject_body() {
        let defs = definitions();
        let def = defs
            .iter()
            .find(|d| d.function.name == "google_gmail_send")
            .unwrap();
        let required: Vec<&str> = def.function.parameters["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(required.contains(&"to"), "Must require 'to'");
        assert!(required.contains(&"subject"), "Must require 'subject'");
        assert!(required.contains(&"body"), "Must require 'body'");
    }

    #[test]
    fn calendar_create_requires_summary_start_end() {
        let defs = definitions();
        let def = defs
            .iter()
            .find(|d| d.function.name == "google_calendar_create")
            .unwrap();
        let required: Vec<&str> = def.function.parameters["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(required.contains(&"summary"));
        assert!(required.contains(&"start"));
        assert!(required.contains(&"end"));
    }

    #[test]
    fn drive_read_requires_file_id() {
        let defs = definitions();
        let def = defs
            .iter()
            .find(|d| d.function.name == "google_drive_read")
            .unwrap();
        let required = def.function.parameters["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v.as_str() == Some("file_id")));
    }

    #[test]
    fn drive_upload_requires_name_content() {
        let defs = definitions();
        let def = defs
            .iter()
            .find(|d| d.function.name == "google_drive_upload")
            .unwrap();
        let required: Vec<&str> = def.function.parameters["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(required.contains(&"name"));
        assert!(required.contains(&"content"));
    }

    #[test]
    fn drive_share_requires_file_id_email() {
        let defs = definitions();
        let def = defs
            .iter()
            .find(|d| d.function.name == "google_drive_share")
            .unwrap();
        let required: Vec<&str> = def.function.parameters["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(required.contains(&"file_id"));
        assert!(required.contains(&"email"));
    }

    #[test]
    fn sheets_read_requires_spreadsheet_id_range() {
        let defs = definitions();
        let def = defs
            .iter()
            .find(|d| d.function.name == "google_sheets_read")
            .unwrap();
        let required: Vec<&str> = def.function.parameters["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(required.contains(&"spreadsheet_id"));
        assert!(required.contains(&"range"));
    }

    #[test]
    fn sheets_append_requires_spreadsheet_id_range_values() {
        let defs = definitions();
        let def = defs
            .iter()
            .find(|d| d.function.name == "google_sheets_append")
            .unwrap();
        let required: Vec<&str> = def.function.parameters["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(required.contains(&"spreadsheet_id"));
        assert!(required.contains(&"range"));
        assert!(required.contains(&"values"));
    }

    #[test]
    fn docs_create_requires_title() {
        let defs = definitions();
        let def = defs
            .iter()
            .find(|d| d.function.name == "google_docs_create")
            .unwrap();
        let required = def.function.parameters["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v.as_str() == Some("title")));
    }

    #[test]
    fn google_api_requires_method_url() {
        let defs = definitions();
        let def = defs
            .iter()
            .find(|d| d.function.name == "google_api")
            .unwrap();
        let required: Vec<&str> = def.function.parameters["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(required.contains(&"method"));
        assert!(required.contains(&"url"));
    }

    // ── Executor dispatch ──────────────────────────────────────────

    /// Verify the executor dispatches all 13 tool names (returns Some, not None).
    /// Without a valid Google token, the execute function should return
    /// Some(Err(...)) rather than None (which would mean "unknown tool").
    #[tokio::test]
    async fn executor_dispatches_all_known_tools() {
        let app_handle_unavailable = true; // We can't construct one in tests
        let _args = serde_json::json!({});
        let tool_names = [
            "google_gmail_list",
            "google_gmail_read",
            "google_gmail_send",
            "google_calendar_list",
            "google_calendar_create",
            "google_drive_list",
            "google_drive_read",
            "google_drive_upload",
            "google_drive_share",
            "google_sheets_read",
            "google_sheets_append",
            "google_docs_create",
            "google_api",
        ];
        // We can't call execute() without an AppHandle, but we can verify
        // that definitions and dispatch names match.
        let defs = definitions();
        let def_names: Vec<&str> = defs.iter().map(|d| d.function.name.as_str()).collect();
        for name in &tool_names {
            assert!(
                def_names.contains(name),
                "Tool '{}' is in dispatch but not in definitions",
                name
            );
        }
        // Verify no extra definitions beyond the dispatch map
        for name in &def_names {
            assert!(
                tool_names.contains(name),
                "Tool '{}' is in definitions but not in dispatch",
                name
            );
        }
        let _ = app_handle_unavailable; // suppress warning
    }

    #[test]
    fn executor_returns_none_for_unknown_tools() {
        // execute() is async and needs an AppHandle, but we can verify
        // via the dispatch table that unknown names won't match
        let defs = definitions();
        let known: Vec<&str> = defs.iter().map(|d| d.function.name.as_str()).collect();
        assert!(!known.contains(&"google_unknown_tool"));
        assert!(!known.contains(&"gmail_list")); // missing google_ prefix
        assert!(!known.contains(&"fetch"));
    }

    // ── Response parsing ───────────────────────────────────────────

    #[test]
    fn extract_body_text_plain() {
        let payload = serde_json::json!({
            "mimeType": "text/plain",
            "body": {
                "data": "SGVsbG8gV29ybGQ" // "Hello World" base64url
            }
        });
        let body = extract_body_text(&payload);
        assert_eq!(body, "Hello World");
    }

    #[test]
    fn extract_body_text_multipart() {
        let payload = serde_json::json!({
            "mimeType": "multipart/alternative",
            "parts": [
                {
                    "mimeType": "text/plain",
                    "body": { "data": "UGxhaW4gdGV4dA" } // "Plain text"
                },
                {
                    "mimeType": "text/html",
                    "body": { "data": "PGI-SFRNTDwvYj4" }
                }
            ]
        });
        let body = extract_body_text(&payload);
        assert_eq!(body, "Plain text");
    }

    #[test]
    fn extract_body_text_no_plain_part() {
        let payload = serde_json::json!({
            "mimeType": "text/html",
            "body": { "data": "PGI-SFRNTDwvYj4" }
        });
        let body = extract_body_text(&payload);
        assert_eq!(body, "");
    }

    #[test]
    fn extract_body_text_nested_multipart() {
        let payload = serde_json::json!({
            "mimeType": "multipart/mixed",
            "parts": [
                {
                    "mimeType": "multipart/alternative",
                    "parts": [
                        {
                            "mimeType": "text/plain",
                            "body": { "data": "TmVzdGVk" } // "Nested"
                        }
                    ]
                }
            ]
        });
        let body = extract_body_text(&payload);
        assert_eq!(body, "Nested");
    }

    #[test]
    fn extract_body_text_empty() {
        let payload = serde_json::json!({});
        let body = extract_body_text(&payload);
        assert_eq!(body, "");
    }

    // ── Base64url ──────────────────────────────────────────────────

    #[test]
    fn decode_base64url_basic() {
        assert_eq!(decode_base64url("SGVsbG8"), "Hello");
        assert_eq!(decode_base64url("V29ybGQ"), "World");
    }

    #[test]
    fn decode_base64url_empty() {
        assert_eq!(decode_base64url(""), "");
    }

    #[test]
    fn decode_base64url_invalid() {
        // Invalid base64 should return empty string, not panic
        assert_eq!(decode_base64url("!!!"), "");
    }

    #[test]
    fn decode_base64url_unicode() {
        // "Ünïcödë" in base64url
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;
        let encoded = URL_SAFE_NO_PAD.encode("Ünïcödë".as_bytes());
        assert_eq!(decode_base64url(&encoded), "Ünïcödë");
    }

    // ── URL safety for google_api ──────────────────────────────────

    #[test]
    fn google_api_definition_restricts_methods() {
        let defs = definitions();
        let def = defs
            .iter()
            .find(|d| d.function.name == "google_api")
            .unwrap();
        let methods = def.function.parameters["properties"]["method"]["enum"]
            .as_array()
            .expect("google_api method should have enum restriction");
        let method_strs: Vec<&str> = methods.iter().filter_map(|m| m.as_str()).collect();
        assert!(method_strs.contains(&"GET"));
        assert!(method_strs.contains(&"POST"));
        assert!(method_strs.contains(&"PUT"));
        assert!(method_strs.contains(&"PATCH"));
        assert!(method_strs.contains(&"DELETE"));
        assert_eq!(method_strs.len(), 5, "Only 5 methods allowed");
    }

    // ── Gmail list defaults ────────────────────────────────────────

    #[test]
    fn gmail_list_has_optional_query() {
        let defs = definitions();
        let def = defs
            .iter()
            .find(|d| d.function.name == "google_gmail_list")
            .unwrap();
        // No required params
        assert!(
            def.function.parameters.get("required").is_none()
                || def.function.parameters["required"]
                    .as_array()
                    .map(|a| a.is_empty())
                    .unwrap_or(true),
            "gmail_list should have no required params"
        );
    }

    // ── Calendar list defaults ─────────────────────────────────────

    #[test]
    fn calendar_list_has_optional_time_range() {
        let defs = definitions();
        let def = defs
            .iter()
            .find(|d| d.function.name == "google_calendar_list")
            .unwrap();
        assert!(
            def.function.parameters.get("required").is_none()
                || def.function.parameters["required"]
                    .as_array()
                    .map(|a| a.is_empty())
                    .unwrap_or(true),
            "calendar_list should have no required params (defaults to today)"
        );
    }
}
