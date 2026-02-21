// Paw Agent Engine — Web Chat Session Management
//
// Cookie-based session handling for authenticated webchat users.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

// ── Session ────────────────────────────────────────────────────────────

pub(crate) struct Session {
    pub username: String,
    pub created_at: u64,
}

static SESSIONS: std::sync::OnceLock<parking_lot::Mutex<HashMap<String, Session>>> =
    std::sync::OnceLock::new();

fn get_sessions() -> &'static parking_lot::Mutex<HashMap<String, Session>> {
    SESSIONS.get_or_init(|| parking_lot::Mutex::new(HashMap::new()))
}

/// Create a new session for `username`, returning the session ID cookie value.
/// Also prunes expired sessions (> 24 h).
pub(crate) fn create_session(username: String) -> String {
    let session_id = uuid::Uuid::new_v4().to_string();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    get_sessions().lock().insert(session_id.clone(), Session { username, created_at: now });
    // Prune expired sessions (> 24 h)
    get_sessions().lock().retain(|_, s| now.saturating_sub(s.created_at) < 86_400);
    session_id
}

/// Validate a session ID and return the associated username, or `None` if expired/missing.
pub(crate) fn validate_session(session_id: &str) -> Option<String> {
    let sessions = get_sessions().lock();
    let s = sessions.get(session_id)?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    if now.saturating_sub(s.created_at) > 86_400 { return None; }
    Some(s.username.clone())
}

/// Extract a cookie value by name from raw HTTP headers.
pub(crate) fn extract_cookie<'a>(headers: &'a str, name: &str) -> Option<&'a str> {
    for line in headers.lines() {
        if line.to_lowercase().starts_with("cookie:") {
            let value = &line["cookie:".len()..];
            for cookie in value.split(';') {
                let cookie = cookie.trim();
                if let Some(rest) = cookie.strip_prefix(name) {
                    if let Some(val) = rest.strip_prefix('=') {
                        return Some(val.trim());
                    }
                }
            }
        }
    }
    None
}
