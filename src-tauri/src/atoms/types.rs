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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    },
    /// A tool finished executing
    #[serde(rename = "tool_result")]
    ToolResultEvent {
        session_id: String,
        run_id: String,
        tool_call_id: String,
        output: String,
        success: bool,
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
    ("AGENTS.md",    "Instructions",  "Operating rules, priorities, memory usage guide"),
    ("SOUL.md",      "Persona",       "Personality, tone, communication style, boundaries"),
    ("USER.md",      "About User",    "Who the user is, how to address them, preferences"),
    ("IDENTITY.md",  "Identity",      "Agent name, emoji, vibe/creature, avatar"),
    ("TOOLS.md",     "Tool Notes",    "Notes about local tools and conventions"),
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
pub(crate) fn default_max_trade() -> f64 { 100.0 }
pub(crate) fn default_max_daily() -> f64 { 500.0 }

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Base URL for embedding API (Ollama: http://localhost:11434)
    pub embedding_base_url: String,
    /// Embedding model name (e.g., "nomic-embed-text", "all-minilm")
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

pub(crate) fn default_user_timezone() -> String { "America/Chicago".to_string() }
pub(crate) fn default_daily_budget_usd() -> f64 { 10.0 }
pub(crate) fn default_max_concurrent_runs() -> u32 { 4 }

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: String,         // inbox, assigned, in_progress, review, blocked, done
    pub priority: String,       // low, medium, high, urgent
    pub assigned_agent: Option<String>,   // legacy single agent (kept for simple cases)
    #[serde(default)]
    pub assigned_agents: Vec<TaskAgent>,  // multi-agent assignments
    pub session_id: Option<String>,
    /// Override model for this task (e.g. "gemini-2.0-flash"). If empty, uses agent routing / default.
    #[serde(default)]
    pub model: Option<String>,
    pub cron_schedule: Option<String>,  // e.g. "every 1h", "daily 09:00", cron expression
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
    pub role: String,           // lead, collaborator
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskActivity {
    pub id: String,
    pub task_id: String,
    pub kind: String,           // created, assigned, status_change, comment, agent_started, agent_completed, agent_error, cron_triggered
    pub agent: Option<String>,
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub title: String,
    pub goal: String,
    pub status: String,         // planning, running, paused, completed, failed
    pub boss_agent: String,     // agent_id of the orchestrator/boss agent
    #[serde(default)]
    pub agents: Vec<ProjectAgent>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectAgent {
    pub agent_id: String,
    pub role: String,           // boss, worker
    pub specialty: String,      // coder, researcher, designer, communicator, security, general
    pub status: String,         // idle, working, done, error
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
    pub to_agent: Option<String>,   // None = broadcast to project
    pub kind: String,               // delegation, progress, result, error, message
    pub content: String,
    pub metadata: Option<String>,   // JSON blob for structured data
    pub created_at: String,
}

// ── Inter-Agent Communication ──────────────────────────────────────────────

/// A direct message between agents, independent of any project context.
/// Stored in the `agent_messages` table and accessible via agent comm tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub id: String,
    pub from_agent: String,
    pub to_agent: String,           // target agent_id, or "broadcast" for all
    pub channel: String,            // topic/channel name for filtering (e.g. "general", "alerts")
    pub content: String,
    pub metadata: Option<String>,   // JSON blob for structured payloads
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
    pub status: String,             // active, paused, disbanded
    #[serde(default)]
    pub members: Vec<SquadMember>,
    pub created_at: String,
    pub updated_at: String,
}

/// A member of a squad with a defined role.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SquadMember {
    pub agent_id: String,
    pub role: String,               // coordinator, member
}
