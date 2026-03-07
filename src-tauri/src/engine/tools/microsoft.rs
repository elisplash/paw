// Paw Agent Engine — Microsoft 365 Tools
//
// Direct OAuth2 implementation: Outlook Mail, Calendar, OneDrive, Teams,
// Tasks (To Do), OneNote, Contacts, and a generic Graph API escape hatch.
// Uses the stored Microsoft OAuth token from the key vault.
// No n8n or external dependencies needed.
//
// Tools:
//   outlook_mail_list       — list/search inbox messages
//   outlook_mail_read       — read a specific email
//   outlook_mail_send       — send (or draft) an email
//   outlook_calendar_list   — list events in a date range
//   outlook_calendar_create — create a calendar event
//   onedrive_list           — list/search files
//   onedrive_read           — read file metadata or download content
//   onedrive_upload         — upload a file
//   teams_list_teams        — list joined teams and channels
//   teams_send_message      — send a message to a Teams channel or chat
//   ms_tasks_list           — list To Do tasks
//   ms_tasks_create         — create a To Do task
//   onenote_list            — list OneNote notebooks and sections
//   microsoft_api           — generic Microsoft Graph API call (escape hatch)

use crate::atoms::types::*;
use log::info;
use std::time::Duration;

// ── Token helper ───────────────────────────────────────────────────────

/// Load the Microsoft OAuth access token from the encrypted vault.
fn load_microsoft_token() -> Result<String, String> {
    use crate::engine::key_vault;
    use crate::engine::skills::crypto::{decrypt_credential, get_vault_key};

    let vault_key = get_vault_key().map_err(|e| format!("Vault key error: {e}"))?;
    let encrypted = key_vault::get("oauth:microsoft")
        .or_else(|| key_vault::get("oauth:microsoft-365"))
        .ok_or("Microsoft 365 is not connected. The user needs to connect Microsoft — go to Integrations → Microsoft 365 → Connect.")?;
    let json = match decrypt_credential(&encrypted, &vault_key) {
        Ok(j) => j,
        Err(_) => {
            key_vault::remove("oauth:microsoft");
            key_vault::remove("oauth:microsoft-365");
            return Err("Microsoft OAuth token is corrupted (likely after an app update). The user needs to reconnect Microsoft 365 — go to Integrations → Microsoft 365 → Connect.".to_string());
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

/// Shared HTTP client with sane timeout.
fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap_or_default()
}

/// Check an HTTP response; return body text if success, or a helpful error.
async fn check_response(resp: reqwest::Response, api_name: &str) -> Result<String, String> {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if status.is_success() {
        Ok(body)
    } else {
        let hint = match status.as_u16() {
            401 => " (Microsoft OAuth token expired — user needs to reconnect Microsoft 365)",
            403 => " (insufficient Microsoft permissions — user may need to reconnect with updated scopes)",
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
        // ── Outlook Mail ───────────────────────────────────────────
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "outlook_mail_list".into(),
                description: "List or search Outlook emails. Returns id, from, subject, preview, date, and read status. Use OData $filter or $search for queries.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query (e.g. 'from:boss@co.com', 'subject:invoice'). Uses Microsoft Graph $search." },
                        "filter": { "type": "string", "description": "OData filter (e.g. \"isRead eq false\", \"receivedDateTime ge 2025-03-01\"). Alternative to query." },
                        "max_results": { "type": "integer", "description": "Max messages to return (1-50, default 20)" },
                        "folder": { "type": "string", "description": "Mail folder (default: 'inbox'). Options: inbox, sentitems, drafts, deleteditems, archive" }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "outlook_mail_read".into(),
                description: "Read the full content of a specific Outlook email by ID. Returns headers and body. Get message IDs from outlook_mail_list first.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "message_id": { "type": "string", "description": "The Outlook message ID" }
                    },
                    "required": ["message_id"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "outlook_mail_send".into(),
                description: "Send an email via Outlook. Always confirm with the user before sending.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "to": { "type": "string", "description": "Recipient email address (comma-separated for multiple)" },
                        "subject": { "type": "string", "description": "Email subject line" },
                        "body": { "type": "string", "description": "Email body (plain text or HTML)" },
                        "content_type": { "type": "string", "enum": ["Text", "HTML"], "description": "Body content type (default: Text)" },
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
                name: "outlook_calendar_list".into(),
                description: "List Outlook Calendar events in a date range. Returns event title, start/end time, location, attendees. Defaults to today's events.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "start": { "type": "string", "description": "Start of range (ISO 8601, e.g. '2025-03-05T00:00:00'). Default: start of today." },
                        "end": { "type": "string", "description": "End of range (ISO 8601). Default: end of today." },
                        "max_results": { "type": "integer", "description": "Max events (1-100, default 25)" }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "outlook_calendar_create".into(),
                description: "Create an Outlook Calendar event. Confirm with the user before creating.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "subject": { "type": "string", "description": "Event title" },
                        "start": { "type": "string", "description": "Start time (ISO 8601, e.g. '2025-03-05T10:00:00')" },
                        "end": { "type": "string", "description": "End time (ISO 8601)" },
                        "body": { "type": "string", "description": "Event description (optional)" },
                        "location": { "type": "string", "description": "Event location (optional)" },
                        "attendees": { "type": "string", "description": "Comma-separated attendee email addresses (optional)" },
                        "timezone": { "type": "string", "description": "IANA timezone (e.g. 'America/New_York'). Default: UTC." },
                        "is_all_day": { "type": "boolean", "description": "Whether this is an all-day event (default: false)" }
                    },
                    "required": ["subject", "start", "end"]
                }),
            },
        },
        // ── OneDrive ───────────────────────────────────────────────
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "onedrive_list".into(),
                description: "List or search files in OneDrive. Returns file id, name, type, size, and last modified. Use query for search.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query (e.g. 'budget report'). Omit to list root files." },
                        "path": { "type": "string", "description": "Folder path (e.g. '/Documents/Projects'). Default: root." },
                        "max_results": { "type": "integer", "description": "Max files (1-100, default 25)" }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "onedrive_read".into(),
                description: "Read a OneDrive file's content or metadata. For text files, returns content. For binary files, returns metadata and download URL.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "item_id": { "type": "string", "description": "The file's item ID (from onedrive_list)" },
                        "path": { "type": "string", "description": "Alternative: file path (e.g. '/Documents/report.txt')" }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "onedrive_upload".into(),
                description: "Upload a text file to OneDrive. Returns the new file item info and web URL.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Destination path including filename (e.g. '/Documents/report.txt')" },
                        "content": { "type": "string", "description": "File content (plain text)" }
                    },
                    "required": ["path", "content"]
                }),
            },
        },
        // ── Teams ──────────────────────────────────────────────────
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "teams_list_teams".into(),
                description: "List Microsoft Teams the user has joined, including their channels.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "include_channels": { "type": "boolean", "description": "Also list channels for each team (default: true)" }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "teams_send_message".into(),
                description: "Send a message to a Microsoft Teams channel or chat. Confirm with the user before sending.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "team_id": { "type": "string", "description": "Team ID (for channel messages)" },
                        "channel_id": { "type": "string", "description": "Channel ID (for channel messages)" },
                        "chat_id": { "type": "string", "description": "Chat ID (for 1:1 or group chat messages — use instead of team_id/channel_id)" },
                        "body": { "type": "string", "description": "Message body (plain text or HTML)" },
                        "content_type": { "type": "string", "enum": ["text", "html"], "description": "Content type (default: text)" }
                    },
                    "required": ["body"]
                }),
            },
        },
        // ── Tasks (To Do) ──────────────────────────────────────────
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "ms_tasks_list".into(),
                description: "List Microsoft To Do tasks. Returns task lists and tasks within them.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "list_id": { "type": "string", "description": "Task list ID. Omit to list all task lists first." },
                        "filter": { "type": "string", "description": "OData filter (e.g. \"status ne 'completed'\")" },
                        "max_results": { "type": "integer", "description": "Max tasks (1-100, default 25)" }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "ms_tasks_create".into(),
                description: "Create a Microsoft To Do task. Confirm with the user before creating.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "list_id": { "type": "string", "description": "Task list ID to create in" },
                        "title": { "type": "string", "description": "Task title" },
                        "body": { "type": "string", "description": "Task body/notes (optional)" },
                        "due_date": { "type": "string", "description": "Due date (ISO 8601 date, e.g. '2025-03-10')" },
                        "importance": { "type": "string", "enum": ["low", "normal", "high"], "description": "Task importance (default: normal)" }
                    },
                    "required": ["list_id", "title"]
                }),
            },
        },
        // ── OneNote ────────────────────────────────────────────────
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "onenote_list".into(),
                description: "List OneNote notebooks and their sections. Returns notebook name, ID, sections.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "include_sections": { "type": "boolean", "description": "Include sections for each notebook (default: true)" }
                    }
                }),
            },
        },
        // ── Generic Graph API ──────────────────────────────────────
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "microsoft_api".into(),
                description: "Make a generic authenticated Microsoft Graph API call. Use this as an escape hatch for any Graph API not covered by dedicated tools. The OAuth token is added automatically.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "method": { "type": "string", "enum": ["GET", "POST", "PUT", "PATCH", "DELETE"], "description": "HTTP method" },
                        "url": { "type": "string", "description": "Full Graph API URL (e.g. 'https://graph.microsoft.com/v1.0/me')" },
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
        "outlook_mail_list" => Some(mail_list(args).await),
        "outlook_mail_read" => Some(mail_read(args).await),
        "outlook_mail_send" => Some(mail_send(args).await),
        "outlook_calendar_list" => Some(calendar_list(args).await),
        "outlook_calendar_create" => Some(calendar_create(args).await),
        "onedrive_list" => Some(drive_list(args).await),
        "onedrive_read" => Some(drive_read(args).await),
        "onedrive_upload" => Some(drive_upload(args).await),
        "teams_list_teams" => Some(teams_list(args).await),
        "teams_send_message" => Some(teams_send(args).await),
        "ms_tasks_list" => Some(tasks_list(args).await),
        "ms_tasks_create" => Some(tasks_create(args).await),
        "onenote_list" => Some(onenote_list(args).await),
        "microsoft_api" => Some(generic_api(args).await),
        _ => None,
    }
}

// ════════════════════════════════════════════════════════════════════════
// Outlook Mail
// ════════════════════════════════════════════════════════════════════════

async fn mail_list(args: &serde_json::Value) -> Result<String, String> {
    let token = load_microsoft_token()?;
    let query = args["query"].as_str().unwrap_or("");
    let filter = args["filter"].as_str().unwrap_or("");
    let max = args["max_results"].as_u64().unwrap_or(20).min(50);
    let folder = args["folder"].as_str().unwrap_or("inbox");

    let mut url = format!(
        "https://graph.microsoft.com/v1.0/me/mailFolders/{}/messages?\
         $top={}&$select=id,subject,from,receivedDateTime,bodyPreview,isRead\
         &$orderby=receivedDateTime desc",
        urlencoding::encode(folder),
        max
    );

    if !query.is_empty() {
        url.push_str(&format!("&$search=\"{}\"", urlencoding::encode(query)));
    } else if !filter.is_empty() {
        url.push_str(&format!("&$filter={}", urlencoding::encode(filter)));
    }

    let resp = http()
        .get(&url)
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| format!("Outlook request failed: {e}"))?;
    let body = check_response(resp, "Outlook mail list").await?;

    let data: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Parse error: {e}"))?;
    let messages: Vec<serde_json::Value> = data["value"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|m| {
                    serde_json::json!({
                        "id": m["id"].as_str().unwrap_or(""),
                        "from": m["from"]["emailAddress"]["address"].as_str().unwrap_or(""),
                        "from_name": m["from"]["emailAddress"]["name"].as_str().unwrap_or(""),
                        "subject": m["subject"].as_str().unwrap_or(""),
                        "date": m["receivedDateTime"].as_str().unwrap_or(""),
                        "preview": m["bodyPreview"].as_str().unwrap_or(""),
                        "unread": !m["isRead"].as_bool().unwrap_or(true),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    if messages.is_empty() {
        return Ok("No messages found.".into());
    }

    info!("[microsoft] outlook_mail_list returned {} messages", messages.len());
    serde_json::to_string_pretty(&messages).map_err(|e| format!("Serialize error: {e}"))
}

async fn mail_read(args: &serde_json::Value) -> Result<String, String> {
    let token = load_microsoft_token()?;
    let message_id = args["message_id"]
        .as_str()
        .ok_or("message_id is required")?;

    let url = format!(
        "https://graph.microsoft.com/v1.0/me/messages/{}?\
         $select=id,subject,from,toRecipients,ccRecipients,receivedDateTime,body,bodyPreview",
        urlencoding::encode(message_id)
    );

    let resp = http()
        .get(&url)
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| format!("Outlook read failed: {e}"))?;
    let body = check_response(resp, "Outlook mail read").await?;
    let msg: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Parse error: {e}"))?;

    let to_addrs: Vec<String> = msg["toRecipients"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|r| r["emailAddress"]["address"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let cc_addrs: Vec<String> = msg["ccRecipients"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|r| r["emailAddress"]["address"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let result = serde_json::json!({
        "id": message_id,
        "from": msg["from"]["emailAddress"]["address"].as_str().unwrap_or(""),
        "from_name": msg["from"]["emailAddress"]["name"].as_str().unwrap_or(""),
        "to": to_addrs,
        "cc": cc_addrs,
        "subject": msg["subject"].as_str().unwrap_or(""),
        "date": msg["receivedDateTime"].as_str().unwrap_or(""),
        "body": msg["body"]["content"].as_str().unwrap_or(""),
        "content_type": msg["body"]["contentType"].as_str().unwrap_or("text"),
    });

    info!("[microsoft] outlook_mail_read message_id={}", message_id);
    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialize error: {e}"))
}

async fn mail_send(args: &serde_json::Value) -> Result<String, String> {
    let token = load_microsoft_token()?;
    let to = args["to"].as_str().ok_or("'to' is required")?;
    let subject = args["subject"].as_str().ok_or("'subject' is required")?;
    let body_text = args["body"].as_str().ok_or("'body' is required")?;
    let content_type = args["content_type"].as_str().unwrap_or("Text");
    let cc = args["cc"].as_str().unwrap_or("");
    let bcc = args["bcc"].as_str().unwrap_or("");

    let to_recipients: Vec<serde_json::Value> = to
        .split(',')
        .map(|e| {
            serde_json::json!({
                "emailAddress": { "address": e.trim() }
            })
        })
        .collect();

    let mut message = serde_json::json!({
        "message": {
            "subject": subject,
            "body": {
                "contentType": content_type,
                "content": body_text
            },
            "toRecipients": to_recipients
        }
    });

    if !cc.is_empty() {
        let cc_recipients: Vec<serde_json::Value> = cc
            .split(',')
            .map(|e| serde_json::json!({"emailAddress": {"address": e.trim()}}))
            .collect();
        message["message"]["ccRecipients"] = serde_json::json!(cc_recipients);
    }
    if !bcc.is_empty() {
        let bcc_recipients: Vec<serde_json::Value> = bcc
            .split(',')
            .map(|e| serde_json::json!({"emailAddress": {"address": e.trim()}}))
            .collect();
        message["message"]["bccRecipients"] = serde_json::json!(bcc_recipients);
    }

    let resp = http()
        .post("https://graph.microsoft.com/v1.0/me/sendMail")
        .bearer_auth(&token)
        .header("Content-Type", "application/json")
        .json(&message)
        .send()
        .await
        .map_err(|e| format!("Outlook send failed: {e}"))?;

    // sendMail returns 202 Accepted with empty body on success
    if resp.status().is_success() {
        info!("[microsoft] outlook_mail_send to={}", to);
        Ok(format!("Email sent successfully to {}", to))
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        Err(format!(
            "Outlook sendMail failed (HTTP {}): {}",
            status.as_u16(),
            &body[..body.len().min(500)]
        ))
    }
}

// ════════════════════════════════════════════════════════════════════════
// Calendar
// ════════════════════════════════════════════════════════════════════════

async fn calendar_list(args: &serde_json::Value) -> Result<String, String> {
    let token = load_microsoft_token()?;

    // Default to today
    let now = chrono::Utc::now();
    let default_start = now.format("%Y-%m-%dT00:00:00Z").to_string();
    let default_end = now.format("%Y-%m-%dT23:59:59Z").to_string();
    let start = args["start"].as_str().unwrap_or(&default_start);
    let end = args["end"].as_str().unwrap_or(&default_end);
    let max = args["max_results"].as_u64().unwrap_or(25).min(100);

    let url = format!(
        "https://graph.microsoft.com/v1.0/me/calendarView?\
         startDateTime={}&endDateTime={}&$top={}\
         &$select=id,subject,start,end,location,attendees,bodyPreview,isAllDay\
         &$orderby=start/dateTime",
        urlencoding::encode(start),
        urlencoding::encode(end),
        max
    );

    let resp = http()
        .get(&url)
        .bearer_auth(&token)
        .header("Prefer", "outlook.timezone=\"UTC\"")
        .send()
        .await
        .map_err(|e| format!("Calendar request failed: {e}"))?;
    let body = check_response(resp, "Outlook calendar list").await?;

    let data: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Parse error: {e}"))?;
    let events: Vec<serde_json::Value> = data["value"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|e| {
                    let attendees: Vec<String> = e["attendees"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|att| {
                                    att["emailAddress"]["address"].as_str().map(String::from)
                                })
                                .collect()
                        })
                        .unwrap_or_default();

                    serde_json::json!({
                        "id": e["id"].as_str().unwrap_or(""),
                        "subject": e["subject"].as_str().unwrap_or(""),
                        "start": e["start"]["dateTime"].as_str().unwrap_or(""),
                        "end": e["end"]["dateTime"].as_str().unwrap_or(""),
                        "location": e["location"]["displayName"].as_str().unwrap_or(""),
                        "is_all_day": e["isAllDay"].as_bool().unwrap_or(false),
                        "attendees": attendees,
                        "preview": e["bodyPreview"].as_str().unwrap_or(""),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    if events.is_empty() {
        return Ok("No events found in the specified range.".into());
    }

    info!("[microsoft] outlook_calendar_list returned {} events", events.len());
    serde_json::to_string_pretty(&events).map_err(|e| format!("Serialize error: {e}"))
}

async fn calendar_create(args: &serde_json::Value) -> Result<String, String> {
    let token = load_microsoft_token()?;
    let subject = args["subject"].as_str().ok_or("'subject' is required")?;
    let start = args["start"].as_str().ok_or("'start' is required")?;
    let end = args["end"].as_str().ok_or("'end' is required")?;
    let tz = args["timezone"].as_str().unwrap_or("UTC");
    let is_all_day = args["is_all_day"].as_bool().unwrap_or(false);

    let mut event = serde_json::json!({
        "subject": subject,
        "start": { "dateTime": start, "timeZone": tz },
        "end": { "dateTime": end, "timeZone": tz },
        "isAllDay": is_all_day,
    });

    if let Some(body) = args["body"].as_str() {
        event["body"] = serde_json::json!({ "contentType": "Text", "content": body });
    }
    if let Some(location) = args["location"].as_str() {
        event["location"] = serde_json::json!({ "displayName": location });
    }
    if let Some(attendees_str) = args["attendees"].as_str() {
        let attendees: Vec<serde_json::Value> = attendees_str
            .split(',')
            .map(|e| {
                serde_json::json!({
                    "emailAddress": { "address": e.trim() },
                    "type": "required"
                })
            })
            .collect();
        event["attendees"] = serde_json::json!(attendees);
    }

    let resp = http()
        .post("https://graph.microsoft.com/v1.0/me/events")
        .bearer_auth(&token)
        .header("Content-Type", "application/json")
        .json(&event)
        .send()
        .await
        .map_err(|e| format!("Calendar create failed: {e}"))?;
    let body = check_response(resp, "Outlook calendar create").await?;
    let created: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Parse error: {e}"))?;

    let result = serde_json::json!({
        "id": created["id"].as_str().unwrap_or(""),
        "subject": created["subject"].as_str().unwrap_or(""),
        "start": created["start"]["dateTime"].as_str().unwrap_or(""),
        "end": created["end"]["dateTime"].as_str().unwrap_or(""),
        "web_link": created["webLink"].as_str().unwrap_or(""),
    });

    info!("[microsoft] outlook_calendar_create subject={}", subject);
    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialize error: {e}"))
}

// ════════════════════════════════════════════════════════════════════════
// OneDrive
// ════════════════════════════════════════════════════════════════════════

async fn drive_list(args: &serde_json::Value) -> Result<String, String> {
    let token = load_microsoft_token()?;
    let query = args["query"].as_str().unwrap_or("");
    let path = args["path"].as_str().unwrap_or("");
    let max = args["max_results"].as_u64().unwrap_or(25).min(100);

    let url = if !query.is_empty() {
        format!(
            "https://graph.microsoft.com/v1.0/me/drive/root/search(q='{}')?\
             $top={}&$select=id,name,size,lastModifiedDateTime,file,folder,webUrl",
            urlencoding::encode(query),
            max
        )
    } else if !path.is_empty() {
        format!(
            "https://graph.microsoft.com/v1.0/me/drive/root:{}:/children?\
             $top={}&$select=id,name,size,lastModifiedDateTime,file,folder,webUrl",
            urlencoding::encode(path),
            max
        )
    } else {
        format!(
            "https://graph.microsoft.com/v1.0/me/drive/root/children?\
             $top={}&$select=id,name,size,lastModifiedDateTime,file,folder,webUrl",
            max
        )
    };

    let resp = http()
        .get(&url)
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| format!("OneDrive request failed: {e}"))?;
    let body = check_response(resp, "OneDrive list").await?;

    let data: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Parse error: {e}"))?;
    let items: Vec<serde_json::Value> = data["value"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|item| {
                    let item_type = if item["folder"].is_object() {
                        "folder"
                    } else {
                        item["file"]["mimeType"].as_str().unwrap_or("file")
                    };
                    serde_json::json!({
                        "id": item["id"].as_str().unwrap_or(""),
                        "name": item["name"].as_str().unwrap_or(""),
                        "type": item_type,
                        "size": item["size"].as_u64().unwrap_or(0),
                        "modified": item["lastModifiedDateTime"].as_str().unwrap_or(""),
                        "web_url": item["webUrl"].as_str().unwrap_or(""),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    if items.is_empty() {
        return Ok("No files found.".into());
    }

    info!("[microsoft] onedrive_list returned {} items", items.len());
    serde_json::to_string_pretty(&items).map_err(|e| format!("Serialize error: {e}"))
}

async fn drive_read(args: &serde_json::Value) -> Result<String, String> {
    let token = load_microsoft_token()?;
    let item_id = args["item_id"].as_str().unwrap_or("");
    let path = args["path"].as_str().unwrap_or("");

    if item_id.is_empty() && path.is_empty() {
        return Err("Either 'item_id' or 'path' is required".into());
    }

    // Get item metadata
    let meta_url = if !item_id.is_empty() {
        format!(
            "https://graph.microsoft.com/v1.0/me/drive/items/{}",
            urlencoding::encode(item_id)
        )
    } else {
        format!(
            "https://graph.microsoft.com/v1.0/me/drive/root:{}",
            urlencoding::encode(path)
        )
    };

    let resp = http()
        .get(&meta_url)
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| format!("OneDrive read failed: {e}"))?;
    let body = check_response(resp, "OneDrive read").await?;
    let meta: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Parse error: {e}"))?;

    let size = meta["size"].as_u64().unwrap_or(0);
    let mime = meta["file"]["mimeType"].as_str().unwrap_or("");

    // For text-ish files under 1MB, download content
    let is_text = mime.starts_with("text/")
        || mime.contains("json")
        || mime.contains("xml")
        || mime.contains("csv")
        || mime.contains("markdown");

    if is_text && size < 1_000_000 {
        let download_url = meta["@microsoft.graph.downloadUrl"]
            .as_str()
            .unwrap_or("");
        if !download_url.is_empty() {
            let content_resp = http()
                .get(download_url)
                .send()
                .await
                .map_err(|e| format!("Download failed: {e}"))?;
            let content = content_resp.text().await.unwrap_or_default();

            let result = serde_json::json!({
                "id": meta["id"].as_str().unwrap_or(""),
                "name": meta["name"].as_str().unwrap_or(""),
                "size": size,
                "content": content,
            });
            info!("[microsoft] onedrive_read (text content)");
            return serde_json::to_string_pretty(&result)
                .map_err(|e| format!("Serialize error: {e}"));
        }
    }

    // For binary/large files, return metadata + download URL
    let result = serde_json::json!({
        "id": meta["id"].as_str().unwrap_or(""),
        "name": meta["name"].as_str().unwrap_or(""),
        "size": size,
        "mime_type": mime,
        "modified": meta["lastModifiedDateTime"].as_str().unwrap_or(""),
        "web_url": meta["webUrl"].as_str().unwrap_or(""),
        "download_url": meta["@microsoft.graph.downloadUrl"].as_str().unwrap_or(""),
    });
    info!("[microsoft] onedrive_read (metadata)");
    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialize error: {e}"))
}

async fn drive_upload(args: &serde_json::Value) -> Result<String, String> {
    let token = load_microsoft_token()?;
    let path = args["path"].as_str().ok_or("'path' is required")?;
    let content = args["content"].as_str().ok_or("'content' is required")?;

    let url = format!(
        "https://graph.microsoft.com/v1.0/me/drive/root:{}:/content",
        urlencoding::encode(path)
    );

    let resp = http()
        .put(&url)
        .bearer_auth(&token)
        .header("Content-Type", "text/plain")
        .body(content.to_string())
        .send()
        .await
        .map_err(|e| format!("OneDrive upload failed: {e}"))?;
    let body = check_response(resp, "OneDrive upload").await?;
    let item: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Parse error: {e}"))?;

    let result = serde_json::json!({
        "id": item["id"].as_str().unwrap_or(""),
        "name": item["name"].as_str().unwrap_or(""),
        "size": item["size"].as_u64().unwrap_or(0),
        "web_url": item["webUrl"].as_str().unwrap_or(""),
    });

    info!("[microsoft] onedrive_upload path={}", path);
    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialize error: {e}"))
}

// ════════════════════════════════════════════════════════════════════════
// Teams
// ════════════════════════════════════════════════════════════════════════

async fn teams_list(args: &serde_json::Value) -> Result<String, String> {
    let token = load_microsoft_token()?;
    let include_channels = args["include_channels"].as_bool().unwrap_or(true);

    let resp = http()
        .get("https://graph.microsoft.com/v1.0/me/joinedTeams?$select=id,displayName,description")
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| format!("Teams request failed: {e}"))?;
    let body = check_response(resp, "Teams list").await?;
    let data: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Parse error: {e}"))?;

    let teams = data["value"].as_array().cloned().unwrap_or_default();
    let mut results: Vec<serde_json::Value> = Vec::new();

    for team in &teams {
        let team_id = team["id"].as_str().unwrap_or("");
        let mut team_info = serde_json::json!({
            "id": team_id,
            "name": team["displayName"].as_str().unwrap_or(""),
            "description": team["description"].as_str().unwrap_or(""),
        });

        if include_channels && !team_id.is_empty() {
            let ch_url = format!(
                "https://graph.microsoft.com/v1.0/teams/{}/channels?\
                 $select=id,displayName,description",
                urlencoding::encode(team_id)
            );
            if let Ok(ch_resp) = http().get(&ch_url).bearer_auth(&token).send().await {
                if let Ok(ch_body) = ch_resp.text().await {
                    if let Ok(ch_data) = serde_json::from_str::<serde_json::Value>(&ch_body) {
                        let channels: Vec<serde_json::Value> = ch_data["value"]
                            .as_array()
                            .map(|arr| {
                                arr.iter()
                                    .map(|ch| {
                                        serde_json::json!({
                                            "id": ch["id"].as_str().unwrap_or(""),
                                            "name": ch["displayName"].as_str().unwrap_or(""),
                                        })
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        team_info["channels"] = serde_json::json!(channels);
                    }
                }
            }
        }
        results.push(team_info);
    }

    if results.is_empty() {
        return Ok("No teams found.".into());
    }

    info!("[microsoft] teams_list returned {} teams", results.len());
    serde_json::to_string_pretty(&results).map_err(|e| format!("Serialize error: {e}"))
}

async fn teams_send(args: &serde_json::Value) -> Result<String, String> {
    let token = load_microsoft_token()?;
    let body_text = args["body"].as_str().ok_or("'body' is required")?;
    let content_type = args["content_type"].as_str().unwrap_or("text");

    let team_id = args["team_id"].as_str().unwrap_or("");
    let channel_id = args["channel_id"].as_str().unwrap_or("");
    let chat_id = args["chat_id"].as_str().unwrap_or("");

    let url = if !chat_id.is_empty() {
        format!(
            "https://graph.microsoft.com/v1.0/chats/{}/messages",
            urlencoding::encode(chat_id)
        )
    } else if !team_id.is_empty() && !channel_id.is_empty() {
        format!(
            "https://graph.microsoft.com/v1.0/teams/{}/channels/{}/messages",
            urlencoding::encode(team_id),
            urlencoding::encode(channel_id)
        )
    } else {
        return Err("Either 'chat_id' or both 'team_id' and 'channel_id' are required".into());
    };

    let message = serde_json::json!({
        "body": {
            "contentType": content_type,
            "content": body_text
        }
    });

    let resp = http()
        .post(&url)
        .bearer_auth(&token)
        .header("Content-Type", "application/json")
        .json(&message)
        .send()
        .await
        .map_err(|e| format!("Teams send failed: {e}"))?;
    let body = check_response(resp, "Teams send message").await?;
    let sent: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Parse error: {e}"))?;

    info!("[microsoft] teams_send_message");
    Ok(format!(
        "Message sent successfully (id: {})",
        sent["id"].as_str().unwrap_or("unknown")
    ))
}

// ════════════════════════════════════════════════════════════════════════
// Tasks (To Do)
// ════════════════════════════════════════════════════════════════════════

async fn tasks_list(args: &serde_json::Value) -> Result<String, String> {
    let token = load_microsoft_token()?;
    let list_id = args["list_id"].as_str().unwrap_or("");

    if list_id.is_empty() {
        // List all task lists
        let resp = http()
            .get("https://graph.microsoft.com/v1.0/me/todo/lists?$select=id,displayName")
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| format!("Tasks request failed: {e}"))?;
        let body = check_response(resp, "Tasks list").await?;
        let data: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| format!("Parse error: {e}"))?;
        let lists: Vec<serde_json::Value> = data["value"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|l| {
                        serde_json::json!({
                            "id": l["id"].as_str().unwrap_or(""),
                            "name": l["displayName"].as_str().unwrap_or(""),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        info!("[microsoft] ms_tasks_list returned {} lists", lists.len());
        return serde_json::to_string_pretty(&lists)
            .map_err(|e| format!("Serialize error: {e}"));
    }

    // List tasks in a specific list
    let max = args["max_results"].as_u64().unwrap_or(25).min(100);
    let filter = args["filter"].as_str().unwrap_or("");

    let mut url = format!(
        "https://graph.microsoft.com/v1.0/me/todo/lists/{}/tasks?\
         $top={}&$select=id,title,status,importance,dueDateTime,body,createdDateTime",
        urlencoding::encode(list_id),
        max
    );
    if !filter.is_empty() {
        url.push_str(&format!("&$filter={}", urlencoding::encode(filter)));
    }

    let resp = http()
        .get(&url)
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| format!("Tasks request failed: {e}"))?;
    let body = check_response(resp, "Tasks list tasks").await?;
    let data: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Parse error: {e}"))?;
    let tasks: Vec<serde_json::Value> = data["value"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|t| {
                    serde_json::json!({
                        "id": t["id"].as_str().unwrap_or(""),
                        "title": t["title"].as_str().unwrap_or(""),
                        "status": t["status"].as_str().unwrap_or(""),
                        "importance": t["importance"].as_str().unwrap_or(""),
                        "due_date": t["dueDateTime"]["dateTime"].as_str().unwrap_or(""),
                        "created": t["createdDateTime"].as_str().unwrap_or(""),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    info!("[microsoft] ms_tasks_list returned {} tasks", tasks.len());
    serde_json::to_string_pretty(&tasks).map_err(|e| format!("Serialize error: {e}"))
}

async fn tasks_create(args: &serde_json::Value) -> Result<String, String> {
    let token = load_microsoft_token()?;
    let list_id = args["list_id"].as_str().ok_or("'list_id' is required")?;
    let title = args["title"].as_str().ok_or("'title' is required")?;
    let importance = args["importance"].as_str().unwrap_or("normal");

    let mut task = serde_json::json!({
        "title": title,
        "importance": importance,
    });

    if let Some(body) = args["body"].as_str() {
        task["body"] = serde_json::json!({ "contentType": "text", "content": body });
    }
    if let Some(due) = args["due_date"].as_str() {
        task["dueDateTime"] = serde_json::json!({
            "dateTime": format!("{}T00:00:00Z", due),
            "timeZone": "UTC"
        });
    }

    let url = format!(
        "https://graph.microsoft.com/v1.0/me/todo/lists/{}/tasks",
        urlencoding::encode(list_id)
    );

    let resp = http()
        .post(&url)
        .bearer_auth(&token)
        .header("Content-Type", "application/json")
        .json(&task)
        .send()
        .await
        .map_err(|e| format!("Tasks create failed: {e}"))?;
    let body = check_response(resp, "Tasks create").await?;
    let created: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Parse error: {e}"))?;

    let result = serde_json::json!({
        "id": created["id"].as_str().unwrap_or(""),
        "title": created["title"].as_str().unwrap_or(""),
        "status": created["status"].as_str().unwrap_or(""),
    });

    info!("[microsoft] ms_tasks_create title={}", title);
    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialize error: {e}"))
}

// ════════════════════════════════════════════════════════════════════════
// OneNote
// ════════════════════════════════════════════════════════════════════════

async fn onenote_list(args: &serde_json::Value) -> Result<String, String> {
    let token = load_microsoft_token()?;
    let include_sections = args["include_sections"].as_bool().unwrap_or(true);

    let expand = if include_sections {
        "&$expand=sections($select=id,displayName)"
    } else {
        ""
    };
    let url = format!(
        "https://graph.microsoft.com/v1.0/me/onenote/notebooks?\
         $select=id,displayName,createdDateTime{}",
        expand
    );

    let resp = http()
        .get(&url)
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| format!("OneNote request failed: {e}"))?;
    let body = check_response(resp, "OneNote list").await?;
    let data: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Parse error: {e}"))?;

    let notebooks: Vec<serde_json::Value> = data["value"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|nb| {
                    let mut info = serde_json::json!({
                        "id": nb["id"].as_str().unwrap_or(""),
                        "name": nb["displayName"].as_str().unwrap_or(""),
                        "created": nb["createdDateTime"].as_str().unwrap_or(""),
                    });
                    if include_sections {
                        let sections: Vec<serde_json::Value> = nb["sections"]
                            .as_array()
                            .map(|s| {
                                s.iter()
                                    .map(|sec| {
                                        serde_json::json!({
                                            "id": sec["id"].as_str().unwrap_or(""),
                                            "name": sec["displayName"].as_str().unwrap_or(""),
                                        })
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        info["sections"] = serde_json::json!(sections);
                    }
                    info
                })
                .collect()
        })
        .unwrap_or_default();

    if notebooks.is_empty() {
        return Ok("No OneNote notebooks found.".into());
    }

    info!("[microsoft] onenote_list returned {} notebooks", notebooks.len());
    serde_json::to_string_pretty(&notebooks).map_err(|e| format!("Serialize error: {e}"))
}

// ════════════════════════════════════════════════════════════════════════
// Generic Graph API
// ════════════════════════════════════════════════════════════════════════

async fn generic_api(args: &serde_json::Value) -> Result<String, String> {
    let token = load_microsoft_token()?;
    let method = args["method"].as_str().ok_or("'method' is required")?;
    let url = args["url"].as_str().ok_or("'url' is required")?;

    // Security: only allow Graph API URLs
    if !url.starts_with("https://graph.microsoft.com/") {
        return Err("microsoft_api only supports https://graph.microsoft.com/ URLs".into());
    }

    let client = http();
    let mut request = match method.to_uppercase().as_str() {
        "GET" => client.get(url),
        "POST" => client.post(url),
        "PUT" => client.put(url),
        "PATCH" => client.patch(url),
        "DELETE" => client.delete(url),
        _ => return Err(format!("Unsupported HTTP method: {}", method)),
    };

    request = request
        .bearer_auth(&token)
        .header("Content-Type", "application/json");

    if let Some(body) = args.get("body") {
        if !body.is_null() {
            request = request.json(body);
        }
    }

    let resp = request
        .send()
        .await
        .map_err(|e| format!("Graph API request failed: {e}"))?;

    // DELETE returns 204 No Content
    if resp.status() == 204 {
        return Ok("Success (204 No Content)".into());
    }

    check_response(resp, "Microsoft Graph API").await
}

// ════════════════════════════════════════════════════════════════════════
// Tests
// ════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definitions_count() {
        let defs = definitions();
        assert_eq!(defs.len(), 14, "Should have 14 Microsoft 365 tools");
    }

    #[test]
    fn all_names_unique() {
        let defs = definitions();
        let mut names: Vec<&str> = defs.iter().map(|d| d.function.name.as_str()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), defs.len(), "All tool names must be unique");
    }

    #[test]
    fn microsoft_api_restricts_methods() {
        let defs = definitions();
        let def = defs
            .iter()
            .find(|d| d.function.name == "microsoft_api")
            .unwrap();
        let methods = def.function.parameters["properties"]["method"]["enum"]
            .as_array()
            .expect("microsoft_api method should have enum restriction");
        assert_eq!(methods.len(), 5, "Only 5 HTTP methods allowed");
    }

    #[test]
    fn mail_send_requires_fields() {
        let defs = definitions();
        let def = defs
            .iter()
            .find(|d| d.function.name == "outlook_mail_send")
            .unwrap();
        let required = def.function.parameters["required"]
            .as_array()
            .expect("outlook_mail_send should have required fields");
        let req_strs: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();
        assert!(req_strs.contains(&"to"));
        assert!(req_strs.contains(&"subject"));
        assert!(req_strs.contains(&"body"));
    }
}
