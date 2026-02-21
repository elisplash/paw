// ── Paw Atoms: Error Types ─────────────────────────────────────────────────
// Single canonical error enum for the engine, built with `thiserror`.
//
// Design rules:
//   • Variants are coarse-grained by domain (I/O, DB, Provider, Config…).
//   • The `#[from]` attribute wires std/external error conversions automatically.
//   • `EngineError` → `String` conversion is provided via `Display` so that
//     Tauri command boundaries (`Result<T, String>`) can call `.map_err(|e|
//     e.to_string())` without boilerplate.
//   • No variant carries secret material (API keys, passwords) in its message.

use thiserror::Error;

// ── Primary error enum ─────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum EngineError {
    /// Filesystem or OS-level I/O failure.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization / deserialization failure.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// HTTP / network failure (reqwest layer).
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    /// SQLite / rusqlite database failure.
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    /// AI provider HTTP or API-level failure (non-secret detail only).
    #[error("Provider error: {provider}: {message}")]
    Provider { provider: String, message: String },

    /// Tool execution failure.
    #[error("Tool error: {tool}: {message}")]
    Tool { tool: String, message: String },

    /// Channel / bridge failure.
    #[error("Channel error: {channel}: {message}")]
    Channel { channel: String, message: String },

    /// Engine or agent configuration is invalid or missing.
    #[error("Configuration error: {0}")]
    Config(String),

    /// Authentication / authorization failure.
    #[error("Auth error: {0}")]
    Auth(String),

    /// Security policy violation (risk classification, approval denial, etc.).
    #[error("Security error: {0}")]
    Security(String),

    /// OS keychain / credential store failure.
    #[error("Keyring error: {0}")]
    Keyring(String),

    /// External process (CLI tool, sandbox, etc.) returned a non-zero exit.
    #[error("Process error: {0}")]
    Process(String),

    /// Catch-all for errors that do not yet have a dedicated variant.
    /// Prefer adding a specific variant over using this in new code.
    #[error("{0}")]
    Other(String),
}

// ── Convenience constructors ───────────────────────────────────────────────

impl EngineError {
    /// Create a provider error with name and message.
    pub fn provider(provider: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Provider { provider: provider.into(), message: message.into() }
    }

    /// Create a tool error with name and message.
    pub fn tool(tool: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Tool { tool: tool.into(), message: message.into() }
    }

    /// Create a channel error with name and message.
    pub fn channel(channel: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Channel { channel: channel.into(), message: message.into() }
    }
}

// ── Migration bridge: String → EngineError ─────────────────────────────────
// Allows `?` on functions still returning `Result<T, String>` inside functions
// that return `EngineResult<T>`. Remove once all modules are migrated.

impl From<String> for EngineError {
    fn from(s: String) -> Self {
        EngineError::Other(s)
    }
}

impl From<&str> for EngineError {
    fn from(s: &str) -> Self {
        EngineError::Other(s.to_string())
    }
}

// ── Convenience alias ──────────────────────────────────────────────────────

/// All engine operations should return this type.
/// At Tauri command boundaries, convert with `.map_err(|e| e.to_string())`.
pub type EngineResult<T> = Result<T, EngineError>;

// ── Conversion: EngineError → String ──────────────────────────────────────
// Lets Tauri command functions call `.map_err(EngineError::into)` directly.

impl From<EngineError> for String {
    fn from(e: EngineError) -> Self {
        e.to_string()
    }
}
