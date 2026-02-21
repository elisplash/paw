// Paw Agent Engine — Core types
// Struct/enum definitions have moved to crate::atoms::types.
// All impl blocks, free functions, and re-exports remain here.
// Downstream code uses `use crate::engine::types::*` unchanged.

pub use crate::atoms::types::*;

// These are the data structures that flow through the entire engine.
// They are independent of any specific AI provider.


// ── Utility ────────────────────────────────────────────────────────────

/// UTF-8–safe string truncation.  Returns a `&str` of at most `max_bytes`
/// bytes, backing up to the previous char boundary if `max_bytes` falls
/// inside a multi-byte character.  Appends "…" when truncated.
///
/// Use this instead of `&s[..s.len().min(N)]` which panics on non-ASCII.
pub fn truncate_utf8(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    // Walk backwards from max_bytes to find a valid char boundary
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

// ── Model / Provider Config ────────────────────────────────────────────



impl ProviderKind {
    pub fn default_base_url(&self) -> &str {
        match self {
            ProviderKind::OpenAI      => "https://api.openai.com/v1",
            ProviderKind::Anthropic   => "https://api.anthropic.com",
            ProviderKind::Google      => "https://generativelanguage.googleapis.com/v1beta",
            ProviderKind::Ollama      => "http://localhost:11434",
            ProviderKind::OpenRouter  => "https://openrouter.ai/api/v1",
            ProviderKind::Custom      => "",
            ProviderKind::DeepSeek    => "https://api.deepseek.com/v1",
            ProviderKind::Grok        => "https://api.x.ai/v1",
            ProviderKind::Mistral     => "https://api.mistral.ai/v1",
            ProviderKind::Moonshot    => "https://api.moonshot.cn/v1",
        }
    }
}

// ── Messages ───────────────────────────────────────────────────────────




impl MessageContent {
    pub fn as_text(&self) -> String {
        match self {
            MessageContent::Text(s) => s.clone(),
            MessageContent::Blocks(blocks) => {
                blocks.iter().filter_map(|b| {
                    if let ContentBlock::Text { text } = b {
                        Some(text.as_str())
                    } else {
                        None
                    }
                }).collect::<Vec<_>>().join("")
            }
        }
    }

    /// Borrow the text content without cloning (returns "" for non-text blocks).
    pub fn as_text_ref(&self) -> &str {
        match self {
            MessageContent::Text(s) => s.as_str(),
            MessageContent::Blocks(_) => "",
        }
    }
}



// ── Tool Calling ───────────────────────────────────────────────────────


// A Gemini "thought" part that must be echoed back with function calls




// ── Tool Calling ──────────────────────────────────────────────────────
// impl ToolDefinition — moved to engine/tools.rs
// Use crate::engine::tools or crate::engine::types::* (re-exported below).

// ── Tool Execution Result ──────────────────────────────────────────────


// ── Streaming Events (Tauri → Frontend) ────────────────────────────────


// ── Session ────────────────────────────────────────────────────────────



// ── Chat Send Request (from frontend) ──────────────────────────────────


// Attachment sent with a chat message (images, files).

// ── Chat Send Response (to frontend) ───────────────────────────────────


// ── Provider API response shapes ───────────────────────────────────────

// Unified streaming chunk from any provider


// Token usage reported by the API (for metering).

// ── Model Pricing ──────────────────────────────────────────────────────

// Per-million-token pricing for known models.
// (input_per_mtok, output_per_mtok)

// Look up pricing for a model. Falls back to cheap defaults.

// ── Model Pricing ─────────────────────────────────────────────────────
// model_price(), estimate_cost_usd(), classify_task_complexity() — moved to engine/pricing.rs
// Re-exported via pub use below.
pub use crate::engine::pricing::{model_price, estimate_cost_usd, classify_task_complexity};

// ── Agent Files (Soul / Persona) ───────────────────────────────────────

// An agent personality file (SOUL.md, AGENTS.md, USER.md, etc.)

// Standard agent files that define soul / persona.

// ── Memory (Long-term Semantic) ────────────────────────────────────────

// A single memory entry stored with its embedding vector.

// An open trading position with stop-loss / take-profit targets.

// Trading policy for auto-approve guidelines.
// serde default helpers for TradingPolicy live in crate::atoms::types


impl Default for TradingPolicy {
    fn default() -> Self {
        Self {
            auto_approve: false,
            max_trade_usd: 100.0,
            max_daily_loss_usd: 500.0,
            allowed_pairs: vec![],
            allow_transfers: false,
            max_transfer_usd: 0.0,
        }
    }
}

/// Memory configuration (embedding provider settings).
impl Default for MemoryConfig {
    fn default() -> Self {
        MemoryConfig {
            embedding_base_url: "http://localhost:11434".into(),
            embedding_model: "nomic-embed-text".into(),
            embedding_dims: 768,
            auto_recall: true,
            auto_capture: true,
            recall_limit: 5,
            recall_threshold: 0.3,
        }
    }
}

// Statistics about the memory store.

// ── Model Routing (Multi-Model Agent System) ──────────────────────────

/// Defines which models to use for different agent roles.
/// With a single API key (e.g. Gemini), you can route the boss agent
/// to a powerful model and sub-agents to cheaper/faster models.
impl Default for ModelRouting {
    fn default() -> Self {
        ModelRouting {
            boss_model: None,
            worker_model: None,
            specialty_models: std::collections::HashMap::new(),
            agent_models: std::collections::HashMap::new(),
            cheap_model: None,
            auto_tier: false,
        }
    }
}

impl ModelRouting {
    /// Resolve the model for a given agent in a project context.
    /// Priority: agent_models > specialty_models > role-based (boss/worker) > fallback
    pub fn resolve(&self, agent_id: &str, role: &str, specialty: &str, fallback: &str) -> String {
        // 1. Per-agent override
        if let Some(m) = self.agent_models.get(agent_id) {
            if !m.is_empty() { return m.clone(); }
        }
        // 2. Per-specialty override
        if !specialty.is_empty() {
            if let Some(m) = self.specialty_models.get(specialty) {
                if !m.is_empty() { return m.clone(); }
            }
        }
        // 3. Role-based (boss vs worker)
        match role {
            "boss" => self.boss_model.as_deref().unwrap_or(fallback).to_string(),
            _ => self.worker_model.as_deref().unwrap_or(fallback).to_string(),
        }
    }

    /// Resolve model using auto-tier: cheap_model for simple tasks, fallback for complex.
    /// Returns (model_name, was_downgraded)
    pub fn resolve_auto_tier(&self, message: &str, fallback: &str) -> (String, bool) {
        if !self.auto_tier {
            return (fallback.to_string(), false);
        }
        match classify_task_complexity(message) {
            TaskComplexity::Simple => {
                if let Some(ref cheap) = self.cheap_model {
                    if !cheap.is_empty() && cheap != fallback {
                        return (cheap.clone(), true);
                    }
                }
                (fallback.to_string(), false)
            }
            TaskComplexity::Complex => (fallback.to_string(), false),
        }
    }
}

// ── Engine State ───────────────────────────────────────────────────────

// serde default helpers for EngineConfig live in crate::atoms::types
use crate::atoms::types::{default_user_timezone, default_daily_budget_usd, default_max_concurrent_runs};

impl Default for EngineConfig {
    fn default() -> Self {
        EngineConfig {
            providers: vec![],
            default_provider: None,
            default_model: None,
            default_system_prompt: Some(r#"You are a powerful AI agent running in Pawz — a desktop AI assistant with full access to the user's machine.

You have these capabilities:
- **exec**: Run any shell command (git, npm, python, system tools, etc.)
- **read_file / write_file**: Read and write any file on the system
- **fetch**: Make HTTP requests to any URL (APIs, webhooks, downloads)
- **web_search / web_read / web_browse / web_screenshot**: Search the internet, read web pages, control a headless browser
- **memory_store / memory_search**: Store and recall long-term memories across conversations
- **soul_read / soul_write / soul_list**: Read and update your own personality and knowledge files
- **self_info**: Check your own configuration — which model you're running, provider, settings, enabled skills, and memory status. Use this proactively when asked about your own setup.
- **update_profile**: Update your own display name, avatar emoji, bio, or system prompt. When the user asks you to change your name or identity, use this tool — it will update the UI in real-time. Use agent_id 'default' for the main agent (you).
- **create_agent**: Create new agent personas that appear in the Agents view. When the user asks you to create an agent, use this tool — don't just describe how to do it.
- **create_task / list_tasks / manage_task**: Create tasks and scheduled automations (cron jobs). You can set up recurring tasks with schedules like 'every 5m', 'every 1h', 'daily 09:00'. The heartbeat system auto-executes due cron tasks every 60 seconds. Use these when the user asks to set up reminders, recurring checks, automations, or scheduled workflows.
- **Skill tools**: Email, Slack, GitHub, REST APIs, webhooks, image generation (when configured)

You have FULL ACCESS — use your tools proactively to accomplish tasks. Don't just describe what you would do; actually do it. If a task requires multiple steps, chain your tool calls together. You can read files, execute code, install packages, create projects, search the web, and interact with external services.

**Self-awareness**: You know which model and provider you're running on (it's in your system context). If asked to verify or confirm anything about your own setup, use the `self_info` tool — never ask the user to look things up for you. You are fully capable of introspecting your own configuration.

Be thorough, resourceful, and action-oriented. When the user asks you to do something, do it completely. Never ask the user to provide file paths, config locations, or technical details you can discover yourself using your tools."#.into()),
            max_tool_rounds: 20,
            tool_timeout_secs: 300,
            user_timezone: default_user_timezone(),
            model_routing: ModelRouting::default(),
            max_concurrent_runs: default_max_concurrent_runs(),
            daily_budget_usd: default_daily_budget_usd(),
        }
    }
}

// ── Tasks ──────────────────────────────────────────────────────────────




// ── Orchestrator: Projects ────────────────────────────────────────────



