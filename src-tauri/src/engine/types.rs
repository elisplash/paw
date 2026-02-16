// Paw Agent Engine — Core types
// These are the data structures that flow through the entire engine.
// They are independent of any specific AI provider.

use serde::{Deserialize, Serialize};

// ── Model / Provider Config ────────────────────────────────────────────

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
}

impl ProviderKind {
    pub fn default_base_url(&self) -> &str {
        match self {
            ProviderKind::OpenAI => "https://api.openai.com/v1",
            ProviderKind::Anthropic => "https://api.anthropic.com",
            ProviderKind::Google => "https://generativelanguage.googleapis.com/v1beta",
            ProviderKind::Ollama => "http://localhost:11434",
            ProviderKind::OpenRouter => "https://openrouter.ai/api/v1",
            ProviderKind::Custom => "",
        }
    }
}

// ── Messages ───────────────────────────────────────────────────────────

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: ImageUrlData },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrlData {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

// ── Tool Calling ───────────────────────────────────────────────────────

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

impl ToolDefinition {
    /// Create the built-in `exec` tool definition that lets agents run shell commands.
    pub fn exec() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "exec".into(),
                description: "Execute a shell command on the user's machine. Returns stdout and stderr. Use this for file operations, git, package managers, CLI tools, etc.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The shell command to execute"
                        }
                    },
                    "required": ["command"]
                }),
            },
        }
    }

    /// Create the built-in `fetch` tool for HTTP requests.
    pub fn fetch() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "fetch".into(),
                description: "Make an HTTP request to any URL. Returns the response body. Use for API calls, web scraping, downloading content.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "The URL to fetch"
                        },
                        "method": {
                            "type": "string",
                            "enum": ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"],
                            "description": "HTTP method (default: GET)"
                        },
                        "headers": {
                            "type": "object",
                            "description": "HTTP headers as key-value pairs"
                        },
                        "body": {
                            "type": "string",
                            "description": "Request body (for POST/PUT/PATCH)"
                        }
                    },
                    "required": ["url"]
                }),
            },
        }
    }

    /// Create a `read_file` tool.
    pub fn read_file() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "read_file".into(),
                description: "Read the contents of a file on the user's machine.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Absolute or relative file path to read"
                        }
                    },
                    "required": ["path"]
                }),
            },
        }
    }

    /// Create a `write_file` tool.
    pub fn write_file() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "write_file".into(),
                description: "Write content to a file on the user's machine. Creates the file if it doesn't exist, overwrites if it does.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Absolute or relative file path to write"
                        },
                        "content": {
                            "type": "string",
                            "description": "The content to write to the file"
                        }
                    },
                    "required": ["path", "content"]
                }),
            },
        }
    }

    /// Tool to read a soul/persona file (SOUL.md, IDENTITY.md, etc.)
    pub fn soul_read() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "soul_read".into(),
                description: "Read one of your own soul/persona files. These files define who you are. Available files: IDENTITY.md (name, role, purpose), SOUL.md (personality, values, voice), USER.md (facts about the user), AGENTS.md (other agents you know about), TOOLS.md (your tool preferences and notes).".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_name": {
                            "type": "string",
                            "description": "The soul file to read, e.g. 'SOUL.md', 'IDENTITY.md', 'USER.md', 'AGENTS.md', 'TOOLS.md'"
                        }
                    },
                    "required": ["file_name"]
                }),
            },
        }
    }

    /// Tool to write/update a soul/persona file.
    pub fn soul_write() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "soul_write".into(),
                description: "Update one of your own soul/persona files. Use this to evolve your personality, record things about the user, or refine your identity. Be thoughtful — these files shape who you are across all conversations.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_name": {
                            "type": "string",
                            "description": "The soul file to write, e.g. 'SOUL.md', 'IDENTITY.md', 'USER.md', 'AGENTS.md', 'TOOLS.md'"
                        },
                        "content": {
                            "type": "string",
                            "description": "The full new content for the file (Markdown format)"
                        }
                    },
                    "required": ["file_name", "content"]
                }),
            },
        }
    }

    /// Tool to list all soul/persona files.
    pub fn soul_list() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "soul_list".into(),
                description: "List all your soul/persona files and their sizes. Use this to see what files exist before reading or writing them.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
        }
    }

    /// Tool to store a memory for long-term recall.
    pub fn memory_store() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "memory_store".into(),
                description: "Store a fact or piece of information in your long-term memory. These memories persist across conversations and are automatically recalled when relevant. Use this to remember user preferences, important facts, project details, etc.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "content": {
                            "type": "string",
                            "description": "The fact or information to remember"
                        },
                        "category": {
                            "type": "string",
                            "description": "Category for organization: 'user_preference', 'project', 'fact', 'instruction', 'general'",
                            "enum": ["user_preference", "project", "fact", "instruction", "general"]
                        }
                    },
                    "required": ["content"]
                }),
            },
        }
    }

    /// Tool to search memories.
    pub fn memory_search() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "memory_search".into(),
                description: "Search your long-term memories for information relevant to a query. Returns the most relevant stored facts.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query to find relevant memories"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of memories to return (default: 5)"
                        }
                    },
                    "required": ["query"]
                }),
            },
        }
    }

    /// Return the default set of built-in tools.
    pub fn builtins() -> Vec<Self> {
        vec![
            Self::exec(),
            Self::fetch(),
            Self::read_file(),
            Self::write_file(),
            Self::soul_read(),
            Self::soul_write(),
            Self::soul_list(),
            Self::memory_store(),
            Self::memory_search(),
        ]
    }
}

// ── Tool Execution Result ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub output: String,
    pub success: bool,
}

// ── Streaming Events (Tauri → Frontend) ────────────────────────────────

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
    },
    /// An error occurred during the run
    #[serde(rename = "error")]
    Error {
        session_id: String,
        run_id: String,
        message: String,
    },
}

// ── Session ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub label: Option<String>,
    pub model: String,
    pub system_prompt: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: i64,
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

// ── Chat Send Request (from frontend) ──────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub session_id: Option<String>,
    pub message: String,
    pub model: Option<String>,
    pub system_prompt: Option<String>,
    pub temperature: Option<f64>,
    pub provider_id: Option<String>,
    pub tools_enabled: Option<bool>,
    #[serde(default)]
    pub attachments: Vec<ChatAttachment>,
}

/// Attachment sent with a chat message (images, files).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatAttachment {
    /// MIME type: "image/png", "image/jpeg", "application/pdf", etc.
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    /// Base64-encoded file content (without data: prefix)
    pub content: String,
}

// ── Chat Send Response (to frontend) ───────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub run_id: String,
    pub session_id: String,
}

// ── Provider API response shapes ───────────────────────────────────────

/// Unified streaming chunk from any provider
#[derive(Debug, Clone)]
pub struct StreamChunk {
    pub delta_text: Option<String>,
    pub tool_calls: Vec<ToolCallDelta>,
    pub finish_reason: Option<String>,
    pub usage: Option<TokenUsage>,
}

#[derive(Debug, Clone)]
pub struct ToolCallDelta {
    pub index: usize,
    pub id: Option<String>,
    pub function_name: Option<String>,
    pub arguments_delta: Option<String>,
}

/// Token usage reported by the API (for metering).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

// ── Agent Files (Soul / Persona) ───────────────────────────────────────

/// An agent personality file (SOUL.md, AGENTS.md, USER.md, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFile {
    pub agent_id: String,
    pub file_name: String,
    pub content: String,
    pub updated_at: String,
}

/// Standard agent files that define soul / persona.
pub const AGENT_STANDARD_FILES: &[(&str, &str, &str)] = &[
    ("AGENTS.md",    "Instructions",  "Operating rules, priorities, memory usage guide"),
    ("SOUL.md",      "Persona",       "Personality, tone, communication style, boundaries"),
    ("USER.md",      "About User",    "Who the user is, how to address them, preferences"),
    ("IDENTITY.md",  "Identity",      "Agent name, emoji, vibe/creature, avatar"),
    ("TOOLS.md",     "Tool Notes",    "Notes about local tools and conventions"),
];

// ── Memory (Long-term Semantic) ────────────────────────────────────────

/// A single memory entry stored with its embedding vector.
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
}

/// Memory configuration (embedding provider settings).
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

/// Statistics about the memory store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub total_memories: i64,
    pub categories: Vec<(String, i64)>,
    pub has_embeddings: bool,
}

// ── Engine State ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    pub providers: Vec<ProviderConfig>,
    pub default_provider: Option<String>,
    pub default_model: Option<String>,
    pub default_system_prompt: Option<String>,
    pub max_tool_rounds: u32,
    pub tool_timeout_secs: u64,
}

impl Default for EngineConfig {
    fn default() -> Self {
        EngineConfig {
            providers: vec![],
            default_provider: None,
            default_model: None,
            default_system_prompt: Some("You are a helpful AI assistant. You have access to tools that let you execute commands, read and write files, and make HTTP requests on the user's machine. Always ask for confirmation before doing anything destructive.".into()),
            max_tool_rounds: 20,
            tool_timeout_secs: 120,
        }
    }
}
