// n8n_engine/types.rs — Types, constants, and pure utility functions
//
// Atom-level: no side effects, no I/O, no IPC. Only data definitions
// and deterministic helpers.

use serde::{Deserialize, Serialize};

// ── Constants ──────────────────────────────────────────────────────────

pub const CONTAINER_NAME: &str = "paw-n8n";
pub const N8N_IMAGE: &str = "n8nio/n8n:latest";
pub const DEFAULT_PORT: u16 = 5678;
pub const HEALTH_ENDPOINT: &str = "/healthz";
pub const API_PROBE_ENDPOINT: &str = "/api/v1/workflows?limit=1";

/// Maximum time (seconds) to wait for n8n to become healthy after start.
/// First-time `npx n8n@latest` download can take 3-5 minutes on slower
/// connections, plus community package reinstall adds more time.
pub const STARTUP_TIMEOUT_SECS: u64 = 360;
/// Interval between readiness polls.
pub const POLL_INTERVAL_SECS: u64 = 2;

pub const CONFIG_KEY: &str = "n8n_engine_config";

// ── Types ──────────────────────────────────────────────────────────────

/// Describes the running n8n endpoint the rest of the app can use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct N8nEndpoint {
    pub url: String,
    pub api_key: String,
    pub mode: N8nMode,
}

/// How the n8n engine was provisioned.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum N8nMode {
    /// Auto-managed Docker container (preferred).
    #[default]
    Embedded,
    /// Managed child process via `npx n8n` (no-Docker fallback).
    Process,
    /// User's locally-running n8n (auto-detected on localhost:5678).
    Local,
    /// User-provided remote URL (teams, n8n Cloud).
    Remote,
}

/// Extended configuration that supersedes the Phase 1 N8nConfig.
/// Backward-compatible: fields added with `#[serde(default)]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct N8nEngineConfig {
    // ── Mode ───────────────────────────────────────────────────────
    #[serde(default)]
    pub mode: N8nMode,

    // ── Remote / local mode (user-supplied) ────────────────────────
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub api_key: String,

    // ── Embedded Docker mode (auto-managed) ────────────────────────
    #[serde(default)]
    pub container_id: Option<String>,
    #[serde(default)]
    pub container_port: Option<u16>,
    #[serde(default)]
    pub encryption_key: Option<String>,

    // ── Process mode (npx n8n) ─────────────────────────────────────
    #[serde(default)]
    pub process_pid: Option<u32>,
    #[serde(default)]
    pub process_port: Option<u16>,

    // ── MCP ────────────────────────────────────────────────────────
    /// Bearer token for n8n's MCP server endpoint.
    /// Retrieved automatically after owner setup.
    #[serde(default)]
    pub mcp_token: Option<String>,

    // ── Common ─────────────────────────────────────────────────────
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub auto_discover: bool,
    #[serde(default)]
    pub mcp_mode: bool,
}

impl Default for N8nEngineConfig {
    fn default() -> Self {
        Self {
            mode: N8nMode::Embedded,
            url: String::new(),
            api_key: String::new(),
            container_id: None,
            container_port: None,
            encryption_key: None,
            process_pid: None,
            process_port: None,
            mcp_token: None,
            enabled: false,
            auto_discover: true,
            mcp_mode: false,
        }
    }
}

/// Status information for the Settings UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct N8nEngineStatus {
    pub running: bool,
    pub mode: N8nMode,
    pub url: String,
    pub docker_available: bool,
    pub node_available: bool,
    pub container_id: Option<String>,
    pub process_pid: Option<u32>,
    pub version: String,
}

// ── Pure utility functions ─────────────────────────────────────────────

/// Generate a random 32-byte hex string for API keys / encryption keys.
pub fn generate_random_key() -> String {
    use std::fmt::Write;
    let mut key = String::with_capacity(64);
    for _ in 0..32 {
        let byte: u8 = rand::random();
        let _ = write!(key, "{:02x}", byte);
    }
    key
}

/// Find an available TCP port starting from `preferred`.
pub fn find_available_port(preferred: u16) -> u16 {
    for port in preferred..preferred + 100 {
        if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return port;
        }
    }
    // Fallback: let OS pick
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .unwrap_or(preferred)
}
