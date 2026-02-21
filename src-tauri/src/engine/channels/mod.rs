// Paw Agent Engine — Shared Channel Bridge Helpers
//
// Common infrastructure that ALL channel bridges (Telegram, Discord, IRC, Slack,
// Matrix, Nostr, Twitch, Mattermost, Nextcloud Talk) share:
//   - run_channel_agent()  — routes a message through the agent loop, returns text
//   - ChannelConfig trait  — common config shape for load/save/user management
//   - split_message()      — splits long responses for platform message limits
//   - Access control       — allowlist / pairing logic

mod access;
mod agent;

use crate::engine::state::EngineState;
use crate::atoms::error::{EngineResult, EngineError};
use serde::{Deserialize, Serialize};
use tauri::Manager;

// Re-export public API
pub use access::{check_access, approve_user_generic, deny_user_generic, remove_user_generic};
pub use agent::{run_channel_agent, run_routed_channel_agent};

// ── Common Channel Config ──────────────────────────────────────────────

/// Every channel bridge stores its config under a unique DB key (e.g. "discord_config").
/// This helper pair handles load/save for any Serialize+Deserialize config type.
pub fn load_channel_config<T: for<'de> Deserialize<'de> + Default>(
    app_handle: &tauri::AppHandle,
    config_key: &str,
) -> EngineResult<T> {
    let engine_state = app_handle.try_state::<EngineState>()
        .ok_or("Engine not initialized")?;

    match engine_state.store.get_config(config_key) {
        Ok(Some(json)) => {
            serde_json::from_str::<T>(&json)
                .map_err(|e| EngineError::Config(format!("Parse {} config: {}", config_key, e)))
        }
        _ => Ok(T::default()),
    }
}

pub fn save_channel_config<T: Serialize>(
    app_handle: &tauri::AppHandle,
    config_key: &str,
    config: &T,
) -> EngineResult<()> {
    let engine_state = app_handle.try_state::<EngineState>()
        .ok_or("Engine not initialized")?;

    let json = serde_json::to_string(config)
        .map_err(|e| format!("Serialize {} config: {}", config_key, e))?;

    engine_state.store.set_config(config_key, &json)?;
    Ok(())
}

// ── Shared Pairing Struct ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingUser {
    pub user_id: String,
    pub username: String,
    pub display_name: String,
    pub requested_at: String,
}

// ── Channel Status (generic) ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelStatus {
    pub running: bool,
    pub connected: bool,
    pub bot_name: Option<String>,
    pub bot_id: Option<String>,
    pub message_count: u64,
    pub allowed_users: Vec<String>,
    pub pending_users: Vec<PendingUser>,
    pub dm_policy: String,
}

// ── Utility ────────────────────────────────────────────────────────────

/// Split a long message into chunks at a given limit, preferring newline/space breaks.
pub fn split_message(text: &str, max_len: usize) -> Vec<String> {
    if text.len() <= max_len {
        return vec![text.to_string()];
    }
    let mut chunks = Vec::new();
    let mut remaining = text;
    while !remaining.is_empty() {
        if remaining.len() <= max_len {
            chunks.push(remaining.to_string());
            break;
        }
        let split_at = remaining[..max_len]
            .rfind('\n')
            .or_else(|| remaining[..max_len].rfind(' '))
            .unwrap_or(max_len);
        chunks.push(remaining[..split_at].to_string());
        remaining = remaining[split_at..].trim_start();
    }
    chunks
}

/// Detect billing, auth, quota, or rate-limit errors that warrant trying
/// a different provider instead of failing outright.
pub fn is_provider_billing_error(err: &str) -> bool {
    let lower = err.to_lowercase();
    lower.contains("credit balance")
        || lower.contains("insufficient_quota")
        || lower.contains("billing")
        || lower.contains("rate_limit")
        || lower.contains("quota exceeded")
        || lower.contains("payment required")
        || lower.contains("account")
        || (lower.contains("api error 4") && (
            lower.contains("401") || lower.contains("402")
            || lower.contains("403") || lower.contains("429")
        ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_message_short() {
        let chunks = split_message("hello", 100);
        assert_eq!(chunks, vec!["hello"]);
    }

    #[test]
    fn split_message_exact_boundary() {
        let msg = "a".repeat(100);
        let chunks = split_message(&msg, 100);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), 100);
    }

    #[test]
    fn split_message_over_boundary() {
        let msg = "word ".repeat(50); // 250 chars
        let chunks = split_message(msg.trim(), 100);
        assert!(chunks.len() >= 2);
        for chunk in &chunks {
            assert!(chunk.len() <= 100);
        }
    }

    #[test]
    fn split_message_prefers_newline_break() {
        let msg = format!("{}\n{}", "a".repeat(60), "b".repeat(60));
        let chunks = split_message(&msg, 80);
        assert_eq!(chunks[0], "a".repeat(60));
    }

    #[test]
    fn split_message_prefers_space_break() {
        let msg = format!("{} {}", "a".repeat(60), "b".repeat(60));
        let chunks = split_message(&msg, 80);
        assert_eq!(chunks[0], "a".repeat(60));
    }

    #[test]
    fn is_provider_billing_error_detects_credit() {
        assert!(is_provider_billing_error("Your credit balance is too low"));
    }

    #[test]
    fn is_provider_billing_error_detects_quota() {
        assert!(is_provider_billing_error("insufficient_quota"));
        assert!(is_provider_billing_error("Quota exceeded"));
    }

    #[test]
    fn is_provider_billing_error_normal_error() {
        assert!(!is_provider_billing_error("Connection refused"));
        assert!(!is_provider_billing_error("Internal server error"));
    }
}
