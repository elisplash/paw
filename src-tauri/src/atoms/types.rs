// ── Paw Atoms: Pure Data Types ────────────────────────────────────────────────
// All plain struct/enum definitions with no logic.
// Atoms layer rule: no I/O, no side effects, no imports from engine/.
//
// These types are re-exported from engine/types.rs via
//   pub use crate::atoms::types::*;
// so all existing `use crate::engine::types::*` imports remain valid.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: String,
    pub kind: ProviderKind,
    pub api_key: String,
    pub base_url: Option<String>,
    pub default_model: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    OpenAI,
    Anthropic,
    Google,
    Ollama,
    OpenRouter,
    Custom,
    DeepSeek,
    Grok,
    Mistral,
    Moonshot,
    /// Azure AI Foundry — uses the Azure AI Inference API
    /// (OpenAI-compatible with `api-key` header + `api-version` query param).
    #[serde(alias = "azure_foundry")]
    AzureFoundry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: MessageContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: ImageUrlData },
    /// Binary document (PDF, etc.) — base64-encoded, sent natively to providers
    #[serde(rename = "document")]
    Document {
        mime_type: String,
        /// Raw base64 content (no data: prefix)
        data: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrlData {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCall,
    /// Google Gemini thought_signature — must be echoed back in functionCall parts
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought_signature: Option<String>,
    /// Gemini thought parts that preceded this function call (must be echoed back)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub thought_parts: Vec<ThoughtPart>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThoughtPart {
    pub text: String,
    pub thought_signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String, // JSON string
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub output: String,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum EngineEvent {
    /// A text delta from the model's response stream
    #[serde(rename = "delta")]
    Delta {
        session_id: String,
        run_id: String,
        text: String,
    },
    /// The model wants to call a tool — waiting for approval
    #[serde(rename = "tool_request")]
    ToolRequest {
        session_id: String,
        run_id: String,
        tool_call: ToolCall,
        /// Tool classification: "safe", "reversible", "external", "dangerous", "unknown"
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_tier: Option<String>,
        // ── Inspector metadata (Phase 4) ──
        /// Current round number in the agent loop
        #[serde(skip_serializing_if = "Option::is_none")]
        round_number: Option<u32>,
        /// Which tools are currently loaded for this agent turn
        #[serde(skip_serializing_if = "Option::is_none")]
        loaded_tools: Option<Vec<String>>,
        /// Estimated context token count at this point
        #[serde(skip_serializing_if = "Option::is_none")]
        context_tokens: Option<u32>,
    },
    /// A tool finished executing
    #[serde(rename = "tool_result")]
    ToolResultEvent {
        session_id: String,
        run_id: String,
        tool_call_id: String,
        output: String,
        success: bool,
        // ── Inspector metadata (Phase 4) ──
        /// Duration of the tool execution in milliseconds
        #[serde(skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },
    /// The full assistant turn is complete
    #[serde(rename = "complete")]
    Complete {
        session_id: String,
        run_id: String,
        text: String,
        tool_calls_count: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<TokenUsage>,
        /// The actual model that responded (from the API, not config)
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        // ── Inspector metadata (Phase 4) ──
        /// Total rounds executed in this agent turn
        #[serde(skip_serializing_if = "Option::is_none")]
        total_rounds: Option<u32>,
        /// Max rounds configured for this agent
        #[serde(skip_serializing_if = "Option::is_none")]
        max_rounds: Option<u32>,
    },
    /// A thinking/reasoning delta from extended-thinking models
    #[serde(rename = "thinking_delta")]
    ThinkingDelta {
        session_id: String,
        run_id: String,
        text: String,
    },
    /// A tool was auto-approved by agent policy (audit trail for auto-approve mode)
    #[serde(rename = "tool_auto_approved")]
    ToolAutoApproved {
        session_id: String,
        run_id: String,
        tool_name: String,
        tool_call_id: String,
    },
    /// An error occurred during the run
    #[serde(rename = "error")]
    Error {
        session_id: String,
        run_id: String,
        message: String,
    },
    /// Agent pushes a new component to the canvas.
    #[serde(rename = "canvas_push")]
    CanvasPush {
        session_id: String,
        run_id: String,
        agent_id: String,
        component_id: String,
        component: CanvasComponent,
    },
    /// Agent updates an existing canvas component in-place.
    #[serde(rename = "canvas_update")]
    CanvasUpdate {
        session_id: String,
        run_id: String,
        agent_id: String,
        component_id: String,
        patch: CanvasComponentPatch,
    },

    // ── Plan execution events (Phase 0: Action DAG) ──────────────────
    /// An execution plan is starting
    #[serde(rename = "plan_start")]
    PlanStart {
        session_id: String,
        run_id: String,
        description: String,
        node_count: usize,
    },
    /// A plan node is starting execution
    #[serde(rename = "plan_node_start")]
    PlanNodeStart {
        session_id: String,
        run_id: String,
        node_id: String,
        tool: String,
    },
    /// An execution plan completed (success or partial)
    #[serde(rename = "plan_complete")]
    PlanComplete {
        session_id: String,
        run_id: String,
        success_count: usize,
        total_count: usize,
        duration_ms: u64,
    },
}

// ── Canvas Components (Agent Canvas) ──────────────────────────────────

/// The type of a canvas component — determines rendering strategy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CanvasComponentType {
    Metric,
    Table,
    Chart,
    Log,
    Kv,
    Card,
    Status,
    Progress,
    Form,
    Markdown,
    Timeline,
    Checklist,
    Gauge,
    Countdown,
    Image,
    Embed,
}

/// A single canvas component created by an agent tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasComponent {
    pub component_type: CanvasComponentType,
    pub title: String,
    pub data: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<CanvasPosition>,
}

/// Grid placement hint for bento-style layout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasPosition {
    pub col: Option<u32>,
    pub row: Option<u32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// Partial update patch for an existing canvas component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasComponentPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<CanvasPosition>,
}

/// A persisted canvas component row, returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasComponentRow {
    pub id: String,
    pub session_id: Option<String>,
    pub dashboard_id: Option<String>,
    pub agent_id: String,
    pub component_type: String,
    pub title: String,
    pub data: String,
    pub position: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// A saved dashboard record, returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardRow {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub agent_id: String,
    pub source_session_id: Option<String>,
    pub template_id: Option<String>,
    pub pinned: bool,
    pub refresh_interval: Option<String>,
    pub refresh_prompt: Option<String>,
    pub last_refreshed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// A dashboard template record, returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardTemplateRow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub components: String,
    pub tags: String,
    pub setup_prompt: Option<String>,
    pub source: String,
    pub created_at: String,
}

/// A dashboard tab state row, returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardTabRow {
    pub id: String,
    pub dashboard_id: String,
    pub tab_order: i32,
    pub active: bool,
    pub window_id: String,
    pub created_at: String,
}

/// Persisted pop-out window geometry for a dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardWindowRow {
    pub dashboard_id: String,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub width: i32,
    pub height: i32,
    pub monitor: Option<i32>,
    pub popped_out: bool,
    pub updated_at: String,
}

/// A single telemetry metric row stored per agent turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryMetricRow {
    pub id: i64,
    pub date: String,
    pub session_id: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
    pub tool_calls: u32,
    pub tool_duration_ms: u64,
    pub llm_duration_ms: u64,
    pub total_duration_ms: u64,
    pub rounds: u32,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub label: Option<String>,
    pub model: String,
    pub system_prompt: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub tool_calls_json: Option<String>,
    pub tool_call_id: Option<String>,
    pub name: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub session_id: Option<String>,
    pub message: String,
    pub model: Option<String>,
    pub system_prompt: Option<String>,
    pub temperature: Option<f64>,
    pub provider_id: Option<String>,
    pub tools_enabled: Option<bool>,
    pub agent_id: Option<String>,
    /// Optional list of allowed tool names. If provided, only these tools
    /// will be offered to the AI model. Enforced by per-agent tool policies.
    #[serde(default)]
    pub tool_filter: Option<Vec<String>>,
    #[serde(default)]
    pub attachments: Vec<ChatAttachment>,
    /// Thinking/reasoning level: "none", "low", "medium", "high"
    #[serde(default)]
    pub thinking_level: Option<String>,
    /// Phase A: If true, all tool calls auto-approved (no HIL popups).
    /// Set by frontend based on agent mode's `auto_approve_all` setting.
    #[serde(default)]
    pub auto_approve_all: bool,
    /// Additional tool names the user has approved via the sidebar Approvals
    /// panel. These are merged with the hardcoded `auto_approved_tools` list.
    #[serde(default)]
    pub user_approved_tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatAttachment {
    /// MIME type: "image/png", "image/jpeg", "application/pdf", etc.
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    /// Base64-encoded file content (without data: prefix)
    pub content: String,
    /// Original filename (optional)
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub run_id: String,
    pub session_id: String,
}

#[derive(Debug, Clone)]
pub struct StreamChunk {
    pub delta_text: Option<String>,
    pub tool_calls: Vec<ToolCallDelta>,
    pub finish_reason: Option<String>,
    pub usage: Option<TokenUsage>,
    /// The actual model name returned by the API (proof of which model responded)
    pub model: Option<String>,
    /// Gemini thought parts that arrived alongside function calls (must be echoed back)
    pub thought_parts: Vec<ThoughtPart>,
    /// Thinking/reasoning text delta from extended thinking / reasoning models
    pub thinking_text: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ToolCallDelta {
    pub index: usize,
    pub id: Option<String>,
    pub function_name: Option<String>,
    pub arguments_delta: Option<String>,
    /// Google Gemini thought_signature — captured from streaming response
    pub thought_signature: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    /// Anthropic prompt-caching: tokens written to cache this request
    #[serde(default)]
    pub cache_creation_tokens: u64,
    /// Anthropic prompt-caching: tokens read from cache (90% cheaper)
    #[serde(default)]
    pub cache_read_tokens: u64,
}

pub struct ModelPrice {
    pub input: f64,
    pub output: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskComplexity {
    /// Simple Q&A, greetings, status checks, single-tool calls
    Simple,
    /// Multi-step reasoning, code generation, analysis, planning
    Complex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFile {
    pub agent_id: String,
    pub file_name: String,
    pub content: String,
    pub updated_at: String,
}

pub const AGENT_STANDARD_FILES: &[(&str, &str, &str)] = &[
    (
        "AGENTS.md",
        "Instructions",
        "Operating rules, priorities, memory usage guide",
    ),
    (
        "SOUL.md",
        "Persona",
        "Personality, tone, communication style, boundaries",
    ),
    (
        "USER.md",
        "About User",
        "Who the user is, how to address them, preferences",
    ),
    (
        "IDENTITY.md",
        "Identity",
        "Agent name, emoji, vibe/creature, avatar",
    ),
    (
        "TOOLS.md",
        "Tool Notes",
        "Notes about local tools and conventions",
    ),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    pub content: String,
    pub category: String,
    pub importance: u8,
    pub created_at: String,
    /// Cosine similarity score — only present in search results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    /// Agent that created this memory (None = shared/global).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub id: String,
    pub mint: String,
    pub symbol: String,
    pub entry_price_usd: f64,
    pub entry_sol: f64,
    pub amount: f64,
    /// Current amount (may decrease after partial take-profit sells)
    pub current_amount: f64,
    /// Stop-loss trigger as a fraction (e.g. 0.30 = sell if price drops 30%)
    pub stop_loss_pct: f64,
    /// Take-profit trigger as a fraction (e.g. 2.0 = sell half at 2x)
    pub take_profit_pct: f64,
    /// "open" | "closed_sl" | "closed_tp" | "closed_manual"
    pub status: String,
    /// Last known price from price check
    #[serde(default)]
    pub last_price_usd: f64,
    /// Timestamp of last price check
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_checked_at: Option<String>,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub close_tx: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
}

/// serde default helpers — must live in this module so #[serde(default = "fn")] resolves correctly
pub(crate) fn default_max_trade() -> f64 {
    100.0
}
pub(crate) fn default_max_daily() -> f64 {
    500.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingPolicy {
    /// Whether auto-approve is enabled for trading tools
    #[serde(default)]
    pub auto_approve: bool,
    /// Maximum allowed trade size in USD
    #[serde(default = "default_max_trade")]
    pub max_trade_usd: f64,
    /// Maximum daily spending (buys + transfers) before requiring manual approval
    #[serde(default = "default_max_daily")]
    pub max_daily_loss_usd: f64,
    /// Allowed trading pairs (empty = all pairs allowed)
    #[serde(default)]
    pub allowed_pairs: Vec<String>,
    /// Whether transfers (send crypto) are auto-approved
    #[serde(default)]
    pub allow_transfers: bool,
    /// Maximum transfer size in USD
    #[serde(default)]
    pub max_transfer_usd: f64,
}

/// Which embedding backend to use.
///
/// - `"auto"` — try Ollama first, then fall back to the user's chat provider
/// - `"ollama"` — Ollama only (local)
/// - `"openai"` — OpenAI embeddings API directly
/// - `"google"` — Google `text-embedding-004` via Gemini API
/// - `"provider"` — reuse whatever chat provider the user configured
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum EmbeddingProvider {
    #[default]
    Auto,
    Ollama,
    OpenAI,
    Google,
    /// Reuse the user's default chat provider for embeddings.
    Provider,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Which embedding backend to use (auto, ollama, openai, google, provider)
    #[serde(default)]
    pub embedding_provider: EmbeddingProvider,
    /// Base URL for embedding API (Ollama: http://localhost:11434)
    pub embedding_base_url: String,
    /// Embedding model name (e.g., "nomic-embed-text", "text-embedding-3-small")
    pub embedding_model: String,
    /// Embedding dimensions (e.g., 768 for nomic-embed-text, 384 for all-minilm)
    pub embedding_dims: usize,
    /// Whether to auto-recall relevant memories before each turn
    pub auto_recall: bool,
    /// Whether to auto-capture facts from conversations
    pub auto_capture: bool,
    /// Max memories to inject via auto-recall
    pub recall_limit: usize,
    /// Minimum similarity score for auto-recall (0.0–1.0)
    pub recall_threshold: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub total_memories: i64,
    pub categories: Vec<(String, i64)>,
    pub has_embeddings: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRouting {
    /// Model for the boss/orchestrator agent (expensive, powerful)
    pub boss_model: Option<String>,
    /// Default model for worker/sub-agents (cheap, fast)
    pub worker_model: Option<String>,
    /// Per-specialty model overrides: e.g. {"coder": "gemini-2.5-pro", "researcher": "gemini-2.0-flash"}
    #[serde(default)]
    pub specialty_models: std::collections::HashMap<String, String>,
    /// Per-agent overrides (highest priority): e.g. {"agent-123": "gemini-2.5-pro"}
    #[serde(default)]
    pub agent_models: std::collections::HashMap<String, String>,
    /// Cheapest model for simple tasks (auto-selected when smart routing is on).
    /// E.g. "claude-3-haiku-20240307", "gemini-2.0-flash", "gpt-4o-mini".
    #[serde(default)]
    pub cheap_model: Option<String>,
    /// Enable automatic model tier selection: simple tasks → cheap_model,
    /// complex tasks → default_model. Disabled by default.
    #[serde(default)]
    pub auto_tier: bool,
}

pub(crate) fn default_user_timezone() -> String {
    "America/Chicago".to_string()
}
pub(crate) fn default_daily_budget_usd() -> f64 {
    10.0
}
pub(crate) fn default_max_concurrent_runs() -> u32 {
    4
}
pub(crate) fn default_context_window_tokens() -> usize {
    32_000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    pub providers: Vec<ProviderConfig>,
    pub default_provider: Option<String>,
    pub default_model: Option<String>,
    pub default_system_prompt: Option<String>,
    pub max_tool_rounds: u32,
    pub tool_timeout_secs: u64,
    /// IANA timezone for local time display (e.g. "America/Chicago")
    #[serde(default = "default_user_timezone")]
    pub user_timezone: String,
    /// Model routing for multi-agent orchestration
    #[serde(default)]
    pub model_routing: ModelRouting,
    /// Maximum simultaneous agent runs (chat + cron + manual). Chat always gets priority.
    #[serde(default = "default_max_concurrent_runs")]
    pub max_concurrent_runs: u32,
    /// Daily budget in USD.  When estimated spend exceeds this, new API calls
    /// are blocked and an error is returned.  Set to 0 to disable.
    #[serde(default = "default_daily_budget_usd")]
    pub daily_budget_usd: f64,
    /// Context window size in tokens.  Controls how much conversation history
    /// the agent sees.  Higher = better topic tracking but more cost.
    /// Default 32K.  Models support 128K-1M, so this is conservative.
    #[serde(default = "default_context_window_tokens")]
    pub context_window_tokens: usize,
    /// Weather location for the Today dashboard (e.g. "New York", "London, UK").
    /// If empty, auto-detected via IP geolocation.
    #[serde(default)]
    pub weather_location: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: String,   // inbox, assigned, in_progress, review, blocked, done
    pub priority: String, // low, medium, high, urgent
    pub assigned_agent: Option<String>, // legacy single agent (kept for simple cases)
    #[serde(default)]
    pub assigned_agents: Vec<TaskAgent>, // multi-agent assignments
    pub session_id: Option<String>,
    /// Override model for this task (e.g. "gemini-2.0-flash"). If empty, uses agent routing / default.
    #[serde(default)]
    pub model: Option<String>,
    pub cron_schedule: Option<String>, // e.g. "every 1h", "daily 09:00", cron expression
    pub cron_enabled: bool,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// Event trigger condition (JSON). When set, the task fires on matching events
    /// instead of (or in addition to) a cron schedule.
    /// Example: `{"type":"webhook","path":"/deploy"}` or `{"type":"file_change","pattern":"*.md"}`
    #[serde(default)]
    pub event_trigger: Option<String>,
    /// If true, the task re-queues itself immediately after each run (always-on monitoring).
    #[serde(default)]
    pub persistent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAgent {
    pub agent_id: String,
    pub role: String, // lead, collaborator
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskActivity {
    pub id: String,
    pub task_id: String,
    pub kind: String, // created, assigned, status_change, comment, agent_started, agent_completed, agent_error, cron_triggered
    pub agent: Option<String>,
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub title: String,
    pub goal: String,
    pub status: String,     // planning, running, paused, completed, failed
    pub boss_agent: String, // agent_id of the orchestrator/boss agent
    #[serde(default)]
    pub agents: Vec<ProjectAgent>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectAgent {
    pub agent_id: String,
    pub role: String,      // boss, worker
    pub specialty: String, // coder, researcher, designer, communicator, security, general
    pub status: String,    // idle, working, done, error
    pub current_task: Option<String>,
    /// Optional per-agent model override (takes highest priority)
    #[serde(default)]
    pub model: Option<String>,
    /// Custom system prompt for this agent (set at creation time)
    #[serde(default)]
    pub system_prompt: Option<String>,
    /// Capabilities / tool names this agent is allowed to use
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMessage {
    pub id: String,
    pub project_id: String,
    pub from_agent: String,
    pub to_agent: Option<String>, // None = broadcast to project
    pub kind: String,             // delegation, progress, result, error, message
    pub content: String,
    pub metadata: Option<String>, // JSON blob for structured data
    pub created_at: String,
}

// ── Inter-Agent Communication ──────────────────────────────────────────────

/// A direct message between agents, independent of any project context.
/// Stored in the `agent_messages` table and accessible via agent comm tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub id: String,
    pub from_agent: String,
    pub to_agent: String, // target agent_id, or "broadcast" for all
    pub channel: String,  // topic/channel name for filtering (e.g. "general", "alerts")
    pub content: String,
    pub metadata: Option<String>, // JSON blob for structured payloads
    pub read: bool,
    pub created_at: String,
}

// ── Agent Squads ───────────────────────────────────────────────────────────

/// A named group of agents that can be assigned goals collectively.
/// Squads enable peer-to-peer collaboration without the boss/worker hierarchy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Squad {
    pub id: String,
    pub name: String,
    pub goal: String,
    pub status: String, // active, paused, disbanded
    #[serde(default)]
    pub members: Vec<SquadMember>,
    pub created_at: String,
    pub updated_at: String,
}

/// A member of a squad with a defined role.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SquadMember {
    pub agent_id: String,
    pub role: String, // coordinator, member
}

// ── Flows (Visual Pipeline) ───────────────────────────────────────────────

/// A persisted visual flow graph.
/// The graph payload is stored as a JSON blob — the Rust side doesn't
/// need to understand node/edge internals; it only indexes metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flow {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub folder: Option<String>,
    /// The full FlowGraph JSON (nodes, edges, etc.)
    pub graph_json: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A single execution run record for a flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowRun {
    pub id: String,
    pub flow_id: String,
    pub status: String, // running, success, error, cancelled
    pub duration_ms: Option<i64>,
    /// The FlowExecEvent[] JSON array
    #[serde(default)]
    pub events_json: Option<String>,
    /// Optional error message
    #[serde(default)]
    pub error: Option<String>,
    pub started_at: String,
    #[serde(default)]
    pub finished_at: Option<String>,
}
