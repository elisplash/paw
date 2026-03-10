// pawz-code — types.rs
// Shared types: EngineEvent (SSE wire format), internal Message representation,
// tool definitions, and chat request/response types.
//
// EngineEvent MUST match PawzEvent in vscode-extension/src/pawz-client.ts exactly
// so the existing VS Code extension works with zero changes.

use serde::{Deserialize, Serialize};

// ── SSE Wire Events (match PawzEvent in pawz-client.ts) ─────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum EngineEvent {
    #[serde(rename = "delta")]
    Delta {
        session_id: String,
        run_id: String,
        text: String,
    },
    #[serde(rename = "tool_request")]
    ToolRequest {
        session_id: String,
        run_id: String,
        tool_call: ToolCall,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_tier: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        round_number: Option<u32>,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        session_id: String,
        run_id: String,
        tool_call_id: String,
        tool_name: String,
        output: String,
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },
    #[serde(rename = "complete")]
    Complete {
        session_id: String,
        run_id: String,
        text: String,
        tool_calls_count: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<TokenUsage>,
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        total_rounds: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max_rounds: Option<u32>,
    },
    #[serde(rename = "thinking_delta")]
    ThinkingDelta {
        session_id: String,
        run_id: String,
        text: String,
    },
    #[serde(rename = "tool_auto_approved")]
    ToolAutoApproved {
        session_id: String,
        run_id: String,
        tool_name: String,
        tool_call_id: String,
    },
    #[serde(rename = "error")]
    Error {
        session_id: String,
        run_id: String,
        message: String,
    },
}

/// Serialise an EngineEvent to a JSON string for the SSE broadcast channel.
pub fn event_to_json(ev: &EngineEvent) -> String {
    serde_json::to_string(ev).unwrap_or_default()
}

// ── Tool Call (SSE + API) ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

// ── Token Usage ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

// ── Internal Message Representation (provider-agnostic) ─────────────────────
// Stored in SQLite as JSON and converted to provider-specific wire format.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String, // "user" | "assistant"
    pub blocks: Vec<ContentBlock>,
}

impl Message {
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            blocks: vec![ContentBlock::Text { text: text.into() }],
        }
    }
    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            blocks: vec![ContentBlock::Text { text: text.into() }],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        is_error: bool,
    },
}

// ── Chat Request (HTTP body for POST /chat/stream) ───────────────────────────

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub message: String,
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default = "default_user")]
    #[allow(dead_code)]
    pub user_id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub agent_id: Option<String>,
}

fn default_user() -> String {
    "user".into()
}

// ── Tool Definition (sent to LLM) ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: serde_json::Value,
}

// ── Result of a single LLM streaming call ────────────────────────────────────

#[derive(Debug, Default)]
pub struct LlmResult {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Option<TokenUsage>,
    pub model: Option<String>,
    #[allow(dead_code)]
    pub stop_reason: String,
}
