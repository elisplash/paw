// WhatsApp Bridge — Configuration
// WhatsAppConfig, CONFIG_KEY, load_config, save_config, approve/deny/remove_user

use crate::engine::channels::{self, PendingUser};
use serde::{Deserialize, Serialize};
use crate::atoms::error::EngineResult;

// ── Constants ──────────────────────────────────────────────────────────

pub(crate) const CONFIG_KEY: &str = "whatsapp_config";

// ── Config Struct ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WhatsAppConfig {
    pub enabled: bool,
    /// Instance name for Evolution API (default: "paw")
    pub instance_name: String,
    /// Evolution API base URL (auto-set when Docker container starts)
    pub api_url: String,
    /// Evolution API key (auto-generated on first run)
    pub api_key: String,
    /// Port for the Evolution API container (default: 8085)
    pub api_port: u16,
    /// Port for the local webhook listener (default: 8086)
    pub webhook_port: u16,
    /// "open" | "allowlist" | "pairing"
    pub dm_policy: String,
    /// Allowed phone numbers or WhatsApp JIDs (e.g. "1234567890" or "1234567890@s.whatsapp.net")
    pub allowed_users: Vec<String>,
    #[serde(default)]
    pub pending_users: Vec<PendingUser>,
    /// Which agent to route messages to
    pub agent_id: Option<String>,
    /// Whether to respond in group chats (when mentioned)
    #[serde(default)]
    pub respond_in_groups: bool,
    /// Docker container ID (managed internally)
    #[serde(default)]
    pub container_id: Option<String>,
    /// Whether the WhatsApp session is connected (QR scanned)
    #[serde(default)]
    pub session_connected: bool,
    /// QR code data (base64) for the frontend to display
    #[serde(default)]
    pub qr_code: Option<String>,
}

impl Default for WhatsAppConfig {
    fn default() -> Self {
        // Generate a random API key on first creation
        let api_key = format!("paw-wa-{}", &uuid::Uuid::new_v4().to_string().replace('-', "")[..16]);
        WhatsAppConfig {
            enabled: false,
            instance_name: "paw".into(),
            api_url: "http://127.0.0.1:8085".into(),
            api_key,
            api_port: 8085,
            webhook_port: 8086,
            dm_policy: "pairing".into(),
            allowed_users: vec![],
            pending_users: vec![],
            agent_id: None,
            respond_in_groups: false,
            container_id: None,
            session_connected: false,
            qr_code: None,
        }
    }
}

// ── Config Persistence ─────────────────────────────────────────────────

pub fn load_config(app_handle: &tauri::AppHandle) -> EngineResult<WhatsAppConfig> {
    channels::load_channel_config(app_handle, CONFIG_KEY)
}

pub fn save_config(app_handle: &tauri::AppHandle, config: &WhatsAppConfig) -> EngineResult<()> {
    channels::save_channel_config(app_handle, CONFIG_KEY, config)
}

pub fn approve_user(app_handle: &tauri::AppHandle, user_id: &str) -> EngineResult<()> {
    channels::approve_user_generic(app_handle, CONFIG_KEY, user_id)
}

pub fn deny_user(app_handle: &tauri::AppHandle, user_id: &str) -> EngineResult<()> {
    channels::deny_user_generic(app_handle, CONFIG_KEY, user_id)
}

pub fn remove_user(app_handle: &tauri::AppHandle, user_id: &str) -> EngineResult<()> {
    channels::remove_user_generic(app_handle, CONFIG_KEY, user_id)
}
