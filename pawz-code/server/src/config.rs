// pawz-code — config.rs
// Loads/saves ~/.pawz-code/config.toml. Creates a default on first run.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// HTTP port for the SSE server
    #[serde(default = "default_port")]
    pub port: u16,
    /// Bind address — "127.0.0.1" (safe default) or "0.0.0.0"
    #[serde(default = "default_bind")]
    pub bind: String,
    /// Bearer token required on every /chat/stream request
    pub auth_token: String,
    /// LLM provider: "anthropic" | "openai" | "openai-compatible" | "claude_code"
    ///
    /// Set to "claude_code" to route through the Claude Code CLI subprocess
    /// (`claude` binary) instead of calling an API directly. Requires Claude
    /// Code to be installed and authenticated with a Max subscription.
    /// No `api_key` is needed when using this provider.
    #[serde(default = "default_provider")]
    pub provider: String,
    /// API key for the provider (not required when provider = "claude_code")
    #[serde(default)]
    pub api_key: String,
    /// Path to the `claude` binary when provider = "claude_code".
    /// Defaults to "claude" (resolved via $PATH). Override if claude is
    /// installed in a non-standard location.
    #[serde(default)]
    pub claude_binary_path: Option<String>,
    /// Model name (e.g. "claude-opus-4-5", "gpt-4o", "llama3")
    #[serde(default = "default_model")]
    pub model: String,
    /// Base URL override — required for OpenAI-compatible providers (Ollama, OpenRouter, etc.)
    #[serde(default)]
    pub base_url: Option<String>,
    /// Max agent loop rounds before forcing a final answer
    #[serde(default = "default_max_rounds")]
    pub max_rounds: u32,
    /// Optional workspace root injected into every system prompt
    #[serde(default)]
    pub workspace_root: Option<String>,
    /// Model role routing — override which model handles each task role.
    /// If not set, all roles use the default `model`.
    #[serde(default)]
    pub model_roles: ModelRoles,
}

/// Role-based model routing. Use cheaper/faster models for subtasks.
/// Leave any field as None to fall back to the default `model`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelRoles {
    /// Fast model for quick questions and classifications
    #[serde(default)]
    pub fast: Option<String>,
    /// Cheap model for compression, summarisation, and token reduction
    #[serde(default)]
    pub cheap: Option<String>,
    /// Planner model for decomposing complex tasks
    #[serde(default)]
    pub planner: Option<String>,
    /// Coder model for patch generation and edit tasks
    #[serde(default)]
    pub coder: Option<String>,
    /// Review model for final review and verification
    #[serde(default)]
    pub review: Option<String>,
    /// Long-context model for architecture reasoning over large codebases
    #[serde(default)]
    pub long_context: Option<String>,
}

fn default_port() -> u16 {
    3941
}
fn default_bind() -> String {
    "127.0.0.1".into()
}
fn default_provider() -> String {
    "anthropic".into()
}
fn default_model() -> String {
    "claude-opus-4-5".into()
}
fn default_max_rounds() -> u32 {
    12
}

impl Config {
    /// Resolve the model for a given role, falling back to the default model.
    pub fn model_for_role(&self, role: &str) -> &str {
        let roles = &self.model_roles;
        let routed = match role {
            "fast" => roles.fast.as_deref(),
            "cheap" => roles.cheap.as_deref(),
            "planner" => roles.planner.as_deref(),
            "coder" => roles.coder.as_deref(),
            "review" => roles.review.as_deref(),
            "long_context" => roles.long_context.as_deref(),
            _ => None,
        };
        routed.unwrap_or(&self.model)
    }

    pub fn config_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".pawz-code")
            .join("config.toml")
    }

    pub fn db_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".pawz-code")
            .join("memory.db")
    }

    pub fn load_or_create() -> Result<Self> {
        let path = Self::config_path();
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            let config: Config = toml::from_str(&content)?;
            return Ok(config);
        }

        // First run: generate default config with a random auth token
        let config = Config {
            port: default_port(),
            bind: default_bind(),
            auth_token: uuid::Uuid::new_v4().to_string().replace('-', ""),
            provider: default_provider(),
            api_key: String::new(),
            model: default_model(),
            base_url: None,
            max_rounds: default_max_rounds(),
            workspace_root: None,
            model_roles: ModelRoles::default(),
            claude_binary_path: None,
        };

        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, toml::to_string_pretty(&config)?)?;

        eprintln!(
            "\n[pawz-code] Created config: {}\n\
             [pawz-code] Set 'api_key' and (optionally) 'workspace_root' then restart.\n\
             [pawz-code] Auth token for VS Code: {}\n",
            path.display(),
            config.auth_token
        );

        Ok(config)
    }
}
