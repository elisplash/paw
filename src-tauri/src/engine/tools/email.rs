// Paw Agent Engine — Email tools
// email_send, email_read

use crate::atoms::types::*;
use crate::atoms::error::EngineResult;
use log::info;
use std::process::Command as ProcessCommand;

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "email_send".into(),
                description: "Send an email. Credentials are stored securely — you don't need to provide passwords or API keys.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "to": { "type": "string", "description": "Recipient email address" },
                        "subject": { "type": "string", "description": "Email subject line" },
                        "body": { "type": "string", "description": "Email body (plain text or HTML)" },
                        "html": { "type": "boolean", "description": "If true, body is HTML (default: false)" }
                    },
                    "required": ["to", "subject", "body"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "email_read".into(),
                description: "Read recent emails from the inbox. Returns subjects, senders, and previews.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "limit": { "type": "integer", "description": "Number of emails to fetch (default: 5)" },
                        "folder": { "type": "string", "description": "Mailbox folder (default: INBOX)" }
                    }
                }),
            },
        },
    ]
}

pub async fn execute(
    name: &str,
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
) -> Option<Result<String, String>> {
    match name {
        "email_send" | "email_read" => {}
        _ => return None,
    }
    let creds = match super::get_skill_creds("email", app_handle) {
        Ok(c) => c,
        Err(e) => return Some(Err(e)),
    };
    Some(match name {
        "email_send" => execute_email_send(args, &creds).await,
        "email_read" => execute_email_read(args, &creds).await,
        _ => unreachable!(),
    })
}

async fn execute_email_send(
    args: &serde_json::Value,
    creds: &std::collections::HashMap<String, String>,
) -> EngineResult<String> {
    let to = args["to"].as_str().ok_or("email_send: missing 'to'")?;
    let subject = args["subject"].as_str().ok_or("email_send: missing 'subject'")?;
    let body = args["body"].as_str().ok_or("email_send: missing 'body'")?;
    let is_html = args["html"].as_bool().unwrap_or(false);

    let host = creds.get("SMTP_HOST").ok_or("Missing SMTP_HOST credential")?;
    let port: u16 = creds.get("SMTP_PORT").ok_or("Missing SMTP_PORT credential")?
        .parse().map_err(|_| "Invalid SMTP_PORT")?;
    let user = creds.get("SMTP_USER").ok_or("Missing SMTP_USER credential")?;
    let password = creds.get("SMTP_PASSWORD").ok_or("Missing SMTP_PASSWORD credential")?;

    info!("[skill:email] Sending to {} via {}:{}", to, host, port);

    let mail_body = if is_html {
        format!(
            "From: {from}\r\nTo: {to}\r\nSubject: {subject}\r\nContent-Type: text/html; charset=utf-8\r\nMIME-Version: 1.0\r\n\r\n{body}",
            from = user, to = to, subject = subject, body = body
        )
    } else {
        format!(
            "From: {from}\r\nTo: {to}\r\nSubject: {subject}\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{body}",
            from = user, to = to, subject = subject, body = body
        )
    };

    let url = if port == 465 {
        format!("smtps://{}:{}", host, port)
    } else {
        format!("smtp://{}:{}", host, port)
    };

    let output = ProcessCommand::new("curl")
        .args([
            "--ssl-reqd",
            "--url", &url,
            "--user", &format!("{}:{}", user, password),
            "--mail-from", user,
            "--mail-rcpt", to,
            "-T", "-",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                stdin.write_all(mail_body.as_bytes())?;
            }
            child.wait_with_output()
        })?;

    if output.status.success() {
        Ok(format!("Email sent successfully to {}", to))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("SMTP error: {}", stderr).into())
    }
}

async fn execute_email_read(
    args: &serde_json::Value,
    creds: &std::collections::HashMap<String, String>,
) -> EngineResult<String> {
    let limit = args["limit"].as_u64().unwrap_or(5);
    let folder = args["folder"].as_str().unwrap_or("INBOX");

    let host = creds.get("IMAP_HOST")
        .or_else(|| creds.get("SMTP_HOST"))
        .ok_or("Missing IMAP_HOST credential")?;
    let port = creds.get("IMAP_PORT").map(|p| p.as_str()).unwrap_or("993");
    let user = creds.get("SMTP_USER").ok_or("Missing SMTP_USER credential")?;
    let password = creds.get("SMTP_PASSWORD").ok_or("Missing SMTP_PASSWORD credential")?;

    info!("[skill:email] Reading {} from {}:{}/{}", limit, host, port, folder);

    let url = format!("imaps://{}:{}/{}", host, port, folder);
    let output = ProcessCommand::new("curl")
        .args([
            "--ssl-reqd",
            "--url", &format!("{};MAILINDEX=1:{}", url, limit),
            "--user", &format!("{}:{}", user, password),
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()?;

    if output.status.success() {
        let body = String::from_utf8_lossy(&output.stdout);
        let truncated = if body.len() > 20_000 {
            format!("{}...\n[truncated]", &body[..20_000])
        } else {
            body.to_string()
        };
        Ok(format!("Emails from {}/{}:\n\n{}", host, folder, truncated))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("IMAP error: {}", stderr).into())
    }
}
