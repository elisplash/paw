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

// ── Tool Calling ───────────────────────────────────────────────────────

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

/// A Gemini "thought" part that must be echoed back with function calls
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

    /// Create a `list_directory` tool.
    pub fn list_directory() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "list_directory".into(),
                description: "List files and subdirectories in a directory. Returns names, sizes, and types. Optionally recurse into subdirectories.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Directory path to list (default: current directory)"
                        },
                        "recursive": {
                            "type": "boolean",
                            "description": "If true, list contents recursively (default: false)"
                        },
                        "max_depth": {
                            "type": "integer",
                            "description": "Maximum recursion depth (default: 3)"
                        }
                    },
                    "required": []
                }),
            },
        }
    }

    /// Create an `append_file` tool.
    pub fn append_file() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "append_file".into(),
                description: "Append content to the end of a file. Creates the file if it doesn't exist. Unlike write_file, this preserves existing content.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "File path to append to"
                        },
                        "content": {
                            "type": "string",
                            "description": "Content to append to the file"
                        }
                    },
                    "required": ["path", "content"]
                }),
            },
        }
    }

    /// Create a `delete_file` tool.
    pub fn delete_file() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "delete_file".into(),
                description: "Delete a file or directory from the filesystem. For directories, set recursive=true to delete non-empty directories.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file or directory to delete"
                        },
                        "recursive": {
                            "type": "boolean",
                            "description": "If true and path is a directory, delete it and all contents (default: false)"
                        }
                    },
                    "required": ["path"]
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

    // ── Skill-based tools ──────────────────────────────────────────────

    pub fn email_send() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "email_send".into(),
                description: "Send an email. Credentials are stored securely — you don't need to provide passwords or API keys.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "to": { "type": "string", "description": "Recipient email address" },
                        "subject": { "type": "string", "description": "Email subject line" },
                        "body": { "type": "string", "description": "Email body (plain text or HTML)" },
                        "html": { "type": "boolean", "description": "If true, body is HTML (default: false)" }
                    },
                    "required": ["to", "subject", "body"]
                }),
            },
        }
    }

    pub fn email_read() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "email_read".into(),
                description: "Read recent emails from the inbox. Returns subjects, senders, and previews.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "limit": { "type": "integer", "description": "Number of emails to fetch (default: 5)" },
                        "folder": { "type": "string", "description": "Mailbox folder (default: INBOX)" }
                    },
                    "required": []
                }),
            },
        }
    }

    pub fn telegram_send() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "telegram_send".into(),
                description: "Send a proactive message to a Telegram user. The user must have messaged the bot at least once so their chat_id is known. You can specify the user by their @username (without the @) or by numeric chat_id. If neither is specified, sends to the first known user (the owner). Bot token is loaded automatically from the Telegram bridge config.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "text": { "type": "string", "description": "The message text to send" },
                        "username": { "type": "string", "description": "Telegram username (without @) to send to. Optional — defaults to first known user." },
                        "chat_id": { "type": "integer", "description": "Numeric Telegram chat ID. Alternative to username." }
                    },
                    "required": ["text"]
                }),
            },
        }
    }

    pub fn telegram_read() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "telegram_read".into(),
                description: "Get information about the Telegram bridge status, known users, and configuration. Useful for checking if the Telegram bridge is running and who has messaged the bot.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "info": { "type": "string", "enum": ["status", "users"], "description": "What to retrieve: 'status' for bridge health, 'users' for list of known users (default: status)" }
                    }
                }),
            },
        }
    }

    pub fn slack_send() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "slack_send".into(),
                description: "Send a message to a Slack channel or DM. Credentials are stored securely.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "channel": { "type": "string", "description": "Channel ID (C...) or user ID (U...) to send to. Uses default channel if not specified." },
                        "text": { "type": "string", "description": "The message text to send" }
                    },
                    "required": ["text"]
                }),
            },
        }
    }

    pub fn slack_read() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "slack_read".into(),
                description: "Read recent messages from a Slack channel.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "channel": { "type": "string", "description": "Channel ID to read from" },
                        "limit": { "type": "integer", "description": "Number of messages (default: 10)" }
                    },
                    "required": ["channel"]
                }),
            },
        }
    }

    pub fn github_api() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "github_api".into(),
                description: "Make a GitHub API call. Credentials are stored securely. Common operations: list repos, create issues, list PRs, read files.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "endpoint": { "type": "string", "description": "GitHub API endpoint path (e.g. /repos/owner/repo/issues)" },
                        "method": { "type": "string", "enum": ["GET", "POST", "PUT", "PATCH", "DELETE"], "description": "HTTP method (default: GET)" },
                        "body": { "type": "object", "description": "JSON body for POST/PUT/PATCH requests" }
                    },
                    "required": ["endpoint"]
                }),
            },
        }
    }

    pub fn rest_api_call() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "rest_api_call".into(),
                description: "Make an authenticated API call using stored credentials. The API key is injected automatically — never include credentials in the request.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "API endpoint path (appended to the stored base URL)" },
                        "method": { "type": "string", "enum": ["GET", "POST", "PUT", "PATCH", "DELETE"], "description": "HTTP method (default: GET)" },
                        "headers": { "type": "object", "description": "Additional headers (auth is added automatically)" },
                        "body": { "type": "string", "description": "Request body (JSON string)" }
                    },
                    "required": ["path"]
                }),
            },
        }
    }

    pub fn webhook_send() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "webhook_send".into(),
                description: "Send a JSON payload to a stored webhook URL (Zapier, IFTTT, n8n, custom). The URL is stored securely.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "payload": { "type": "object", "description": "JSON payload to send to the webhook" }
                    },
                    "required": ["payload"]
                }),
            },
        }
    }

    pub fn image_generate() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "image_generate".into(),
                description: "Generate an image from a text description using AI. Returns the file path of the saved image. Use detailed, descriptive prompts for best results.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "prompt": {
                            "type": "string",
                            "description": "Detailed text description of the image to generate. Be specific about style, subject, lighting, colors, composition."
                        },
                        "filename": {
                            "type": "string",
                            "description": "Optional filename for the output image (without extension). Defaults to a generated name."
                        }
                    },
                    "required": ["prompt"]
                }),
            },
        }
    }

    // ── Web browsing tools ─────────────────────────────────────────────

    pub fn web_search() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "web_search".into(),
                description: "Search the internet using DuckDuckGo. Returns titles, URLs, and snippets. No API key needed.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query" },
                        "limit": { "type": "integer", "description": "Max results to return (default: 8)" }
                    },
                    "required": ["query"]
                }),
            },
        }
    }

    pub fn web_read() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "web_read".into(),
                description: "Fetch a web page and extract its readable text content (strips HTML). Much better than raw fetch for reading articles, docs, and web pages. Optionally pass a CSS selector to extract specific elements.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "url": { "type": "string", "description": "URL to read" },
                        "selector": { "type": "string", "description": "Optional CSS selector to extract specific content (e.g. '.article-body', '#main')" }
                    },
                    "required": ["url"]
                }),
            },
        }
    }

    pub fn web_screenshot() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "web_screenshot".into(),
                description: "Take a screenshot of any web page using a headless browser. Returns the file path and visible text. Works with JavaScript-heavy pages, SPAs, and dynamic content that web_read can't handle.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "url": { "type": "string", "description": "URL to screenshot" },
                        "full_page": { "type": "boolean", "description": "Capture the full scrollable page (default: false, viewport only)" },
                        "width": { "type": "integer", "description": "Viewport width in pixels (default: 1280)" },
                        "height": { "type": "integer", "description": "Viewport height in pixels (default: 800)" }
                    },
                    "required": ["url"]
                }),
            },
        }
    }

    pub fn web_browse() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "web_browse".into(),
                description: "Control a headless browser — navigate pages, click buttons, fill forms, run JavaScript, extract data. The browser session persists between calls so you can interact with websites step by step. Actions: navigate, click, type, press, extract, javascript, scroll, links, info.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["navigate", "click", "type", "press", "extract", "javascript", "scroll", "links", "info"],
                            "description": "What to do: navigate (go to URL), click (CSS selector), type (text into selector), press (key like Enter/Tab), extract (get text from selector), javascript (run JS), scroll (up/down/top/bottom), links (list all links), info (current page)"
                        },
                        "url": { "type": "string", "description": "URL for navigate action" },
                        "selector": { "type": "string", "description": "CSS selector for click/type/extract actions" },
                        "text": { "type": "string", "description": "Text to type, key to press, or scroll direction" },
                        "javascript": { "type": "string", "description": "JavaScript code for javascript action" }
                    },
                    "required": ["action"]
                }),
            },
        }
    }

    /// Tool to create a new agent persona from chat.
    pub fn create_agent() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "create_agent".into(),
                description: "Create a new agent persona in Pawz. The agent will appear in the Agents view and can be selected for conversations. Use this when the user asks you to create, build, or set up a new agent/persona.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "The agent's display name, e.g. 'Code Cat', 'Research Owl'"
                        },
                        "role": {
                            "type": "string",
                            "description": "A short role description, e.g. 'Senior Rust Developer', 'Research Assistant'"
                        },
                        "specialty": {
                            "type": "string",
                            "description": "The agent's specialty area: 'coder', 'researcher', 'writer', 'analyst', 'general'",
                            "enum": ["coder", "researcher", "writer", "analyst", "general"]
                        },
                        "system_prompt": {
                            "type": "string",
                            "description": "The system prompt that defines this agent's behavior and personality"
                        },
                        "model": {
                            "type": "string",
                            "description": "Optional: specific model to use for this agent (leave empty to use default)"
                        },
                        "capabilities": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "List of tools this agent can use, e.g. ['exec', 'read_file', 'write_file', 'web_search']. Leave empty for all tools."
                        }
                    },
                    "required": ["name", "role", "system_prompt"]
                }),
            },
        }
    }

    /// Tool to create a task (optionally a recurring cron job) on the task board.
    pub fn create_task() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "create_task".into(),
                description: "Create a new task on the task board. If a cron_schedule is provided, it becomes a recurring automation that the heartbeat executes automatically. Use this when the user asks you to set up a scheduled job, reminder, recurring task, or automation.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "Short title for the task, e.g. 'Check crypto prices', 'Daily standup summary'"
                        },
                        "description": {
                            "type": "string",
                            "description": "Detailed prompt/instructions for what the agent should do when this task runs"
                        },
                        "priority": {
                            "type": "string",
                            "description": "Task priority",
                            "enum": ["low", "medium", "high", "urgent"]
                        },
                        "agent_id": {
                            "type": "string",
                            "description": "Which agent should run this task. Use 'default' for the main agent, or a specific agent ID."
                        },
                        "cron_schedule": {
                            "type": "string",
                            "description": "Optional recurring schedule: 'every 5m', 'every 1h', 'every 6h', 'daily 09:00', 'daily 18:00'. If omitted, the task is one-shot."
                        }
                    },
                    "required": ["title", "description"]
                }),
            },
        }
    }

    /// Tool to list tasks from the task board.
    pub fn list_tasks() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "list_tasks".into(),
                description: "List all tasks on the task board, including their status, schedule, and assigned agents. Use this to check existing tasks before creating duplicates, or when the user asks about their tasks/automations.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "status_filter": {
                            "type": "string",
                            "description": "Optional: filter by status (inbox, assigned, in_progress, review, blocked, done). Omit to list all."
                        },
                        "cron_only": {
                            "type": "boolean",
                            "description": "If true, only return tasks that have a cron schedule (automations)."
                        }
                    },
                    "required": []
                }),
            },
        }
    }

    /// Tool to update or delete a task on the task board.
    pub fn manage_task() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "manage_task".into(),
                description: "Update or delete an existing task. Can change its title, description, schedule, status, priority, assigned agent, or enable/disable cron. Can also delete the task entirely.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "task_id": {
                            "type": "string",
                            "description": "The ID of the task to update or delete. Use list_tasks first to find the ID."
                        },
                        "action": {
                            "type": "string",
                            "description": "What to do with the task",
                            "enum": ["update", "delete", "run_now", "pause", "enable"]
                        },
                        "title": { "type": "string", "description": "New title (update only)" },
                        "description": { "type": "string", "description": "New description/prompt (update only)" },
                        "priority": { "type": "string", "description": "New priority (update only)", "enum": ["low", "medium", "high", "urgent"] },
                        "status": { "type": "string", "description": "New status (update only)", "enum": ["inbox", "assigned", "in_progress", "review", "blocked", "done"] },
                        "cron_schedule": { "type": "string", "description": "New schedule (update only)" },
                        "agent_id": { "type": "string", "description": "New agent assignment (update only)" }
                    },
                    "required": ["task_id", "action"]
                }),
            },
        }
    }

    /// Tool for the agent to update its own profile (name, avatar, bio, system prompt).
    pub fn update_profile() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "update_profile".into(),
                description: "Update your own agent profile — change your display name, avatar emoji, bio, or system prompt. Use this when the user asks you to change your name, identity, personality, or appearance. All fields are optional; only provided fields are updated.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "agent_id": {
                            "type": "string",
                            "description": "The agent ID to update. Use 'default' for the main/primary agent (yourself, unless you are a sub-agent)."
                        },
                        "name": {
                            "type": "string",
                            "description": "New display name, e.g. 'Pawz', 'Nova', 'Jarvis'"
                        },
                        "avatar": {
                            "type": "string",
                            "description": "New avatar emoji, e.g. '🐾', '🤖', '🧠'"
                        },
                        "bio": {
                            "type": "string",
                            "description": "New short bio/description"
                        },
                        "system_prompt": {
                            "type": "string",
                            "description": "New system prompt / personality instructions"
                        }
                    },
                    "required": ["agent_id"]
                }),
            },
        }
    }

    /// Tool for the agent to introspect its own configuration.
    pub fn self_info() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "self_info".into(),
                description: "Get information about your own runtime: current model, provider, session, engine settings, configured providers, memory status, and enabled skills. Use this when you need to check which model you're running, verify configuration, or answer questions about your own setup.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
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
            Self::list_directory(),
            Self::append_file(),
            Self::delete_file(),
            Self::web_search(),
            Self::web_read(),
            Self::web_screenshot(),
            Self::web_browse(),
            Self::soul_read(),
            Self::soul_write(),
            Self::soul_list(),
            Self::memory_store(),
            Self::memory_search(),
            Self::self_info(),
            Self::update_profile(),
            Self::create_agent(),
            Self::create_task(),
            Self::list_tasks(),
            Self::manage_task(),
        ]
    }

    /// Return tools for enabled skills.
    pub fn skill_tools(enabled_skill_ids: &[String]) -> Vec<Self> {
        let mut tools = Vec::new();
        for id in enabled_skill_ids {
            match id.as_str() {
                "email" => { tools.push(Self::email_send()); tools.push(Self::email_read()); }
                "slack" => { tools.push(Self::slack_send()); tools.push(Self::slack_read()); }
                "telegram" => { tools.push(Self::telegram_send()); tools.push(Self::telegram_read()); }
                "github" => { tools.push(Self::github_api()); }
                "rest_api" => { tools.push(Self::rest_api_call()); }
                "webhook" => { tools.push(Self::webhook_send()); }
                "image_gen" => { tools.push(Self::image_generate()); }
                "coinbase" => {
                    tools.push(Self::coinbase_prices());
                    tools.push(Self::coinbase_balance());
                    tools.push(Self::coinbase_wallet_create());
                    tools.push(Self::coinbase_trade());
                    tools.push(Self::coinbase_transfer());
                }
                "dex" => {
                    tools.push(Self::dex_wallet_create());
                    tools.push(Self::dex_balance());
                    tools.push(Self::dex_quote());
                    tools.push(Self::dex_swap());
                    tools.push(Self::dex_portfolio());
                    tools.push(Self::dex_token_info());
                    tools.push(Self::dex_check_token());
                    tools.push(Self::dex_search_token());
                    tools.push(Self::dex_watch_wallet());
                    tools.push(Self::dex_whale_transfers());
                    tools.push(Self::dex_top_traders());
                    tools.push(Self::dex_trending());
                }
                "solana_dex" => {
                    tools.push(Self::sol_wallet_create());
                    tools.push(Self::sol_balance());
                    tools.push(Self::sol_quote());
                    tools.push(Self::sol_swap());
                    tools.push(Self::sol_portfolio());
                    tools.push(Self::sol_token_info());
                }
                _ => {}
            }
        }
        tools
    }

    // ── Coinbase CDP tools ─────────────────────────────────────────────

    pub fn coinbase_prices() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "coinbase_prices".into(),
                description: "Get current spot prices for one or more crypto assets from Coinbase. Returns USD prices. Credentials are auto-injected — just call this tool directly.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "symbols": {
                            "type": "string",
                            "description": "Comma-separated crypto symbols (e.g. 'BTC,ETH,SOL'). Use standard ticker symbols."
                        }
                    },
                    "required": ["symbols"]
                }),
            },
        }
    }

    pub fn coinbase_balance() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "coinbase_balance".into(),
                description: "Check wallet/account balances on Coinbase. Returns all non-zero balances with USD values. Credentials are auto-injected — just call this tool directly.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "currency": {
                            "type": "string",
                            "description": "Optional: filter to a specific currency (e.g. 'BTC'). Omit to see all balances."
                        }
                    },
                    "required": []
                }),
            },
        }
    }

    pub fn coinbase_wallet_create() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "coinbase_wallet_create".into(),
                description: "Create a new Coinbase wallet. This creates an MPC-secured wallet on the Coinbase platform.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Human-readable name for the wallet (e.g. 'Trading Wallet', 'Savings')"
                        }
                    },
                    "required": ["name"]
                }),
            },
        }
    }

    pub fn coinbase_trade() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "coinbase_trade".into(),
                description: "Execute a crypto trade on Coinbase. REQUIRES USER APPROVAL. Always explain your reasoning and include risk parameters before calling this.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "side": {
                            "type": "string",
                            "enum": ["buy", "sell"],
                            "description": "Trade direction: 'buy' or 'sell'"
                        },
                        "product_id": {
                            "type": "string",
                            "description": "Trading pair (e.g. 'BTC-USD', 'ETH-USD', 'SOL-USD')"
                        },
                        "amount": {
                            "type": "string",
                            "description": "Amount in quote currency for buys (e.g. '100' for $100 of BTC) or base currency for sells (e.g. '0.5' for 0.5 BTC)"
                        },
                        "order_type": {
                            "type": "string",
                            "enum": ["market", "limit"],
                            "description": "Order type: 'market' (immediate) or 'limit' (at specific price). Default: market"
                        },
                        "limit_price": {
                            "type": "string",
                            "description": "Limit price (required if order_type is 'limit')"
                        },
                        "reason": {
                            "type": "string",
                            "description": "Your analysis and reasoning for this trade. This is shown to the user for approval."
                        }
                    },
                    "required": ["side", "product_id", "amount", "reason"]
                }),
            },
        }
    }

    pub fn coinbase_transfer() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "coinbase_transfer".into(),
                description: "Send crypto from your Coinbase account to an external address. REQUIRES USER APPROVAL. Double-check addresses — crypto transfers are irreversible.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "currency": {
                            "type": "string",
                            "description": "Currency to send (e.g. 'BTC', 'ETH', 'USDC')"
                        },
                        "amount": {
                            "type": "string",
                            "description": "Amount to send (e.g. '0.01')"
                        },
                        "to_address": {
                            "type": "string",
                            "description": "Destination wallet address"
                        },
                        "network": {
                            "type": "string",
                            "description": "Network to send on (e.g. 'base', 'ethereum', 'bitcoin'). Default: native network for the currency."
                        },
                        "reason": {
                            "type": "string",
                            "description": "Reason for this transfer"
                        }
                    },
                    "required": ["currency", "amount", "to_address", "reason"]
                }),
            },
        }
    }

    // ── DEX / Uniswap V3 tools ─────────────────────────────────────────

    pub fn dex_wallet_create() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "dex_wallet_create".into(),
                description: "Create a new self-custody Ethereum wallet. The private key is encrypted and stored in the OS keychain vault — you never see it. Returns the wallet address.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
        }
    }

    pub fn dex_balance() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "dex_balance".into(),
                description: "Check ETH and ERC-20 token balances for the DEX wallet. If no token specified, shows ETH and all tokens with non-zero balances.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "token": {
                            "type": "string",
                            "description": "Specific token to check (e.g. 'USDC', 'WBTC', or a contract address). Omit to check all known tokens."
                        }
                    },
                    "required": []
                }),
            },
        }
    }

    pub fn dex_quote() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "dex_quote".into(),
                description: "Get a swap quote from Uniswap V3 without executing. Shows expected output amount, exchange rate, and minimum output with slippage protection. ALWAYS use this before dex_swap.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "token_in": {
                            "type": "string",
                            "description": "Token to sell (e.g. 'ETH', 'USDC', 'WBTC', or contract address)"
                        },
                        "token_out": {
                            "type": "string",
                            "description": "Token to buy (e.g. 'USDC', 'ETH', 'UNI', or contract address)"
                        },
                        "amount": {
                            "type": "string",
                            "description": "Amount of token_in to swap (e.g. '0.5', '100')"
                        },
                        "fee_tier": {
                            "type": "integer",
                            "description": "Uniswap V3 fee tier in bps: 100 (0.01%), 500 (0.05%), 3000 (0.3%), 10000 (1%). Default: 3000"
                        },
                        "slippage_bps": {
                            "type": "integer",
                            "description": "Slippage tolerance in basis points. Default: 50 (0.5%). Max: 500 (5%)"
                        }
                    },
                    "required": ["token_in", "token_out", "amount"]
                }),
            },
        }
    }

    pub fn dex_swap() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "dex_swap".into(),
                description: "Execute a token swap on Uniswap V3. REQUIRES USER APPROVAL. Gets a quote, handles token approval if needed, builds and signs the transaction, then broadcasts it. The private key never leaves the vault.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "token_in": {
                            "type": "string",
                            "description": "Token to sell (e.g. 'ETH', 'USDC', 'WBTC')"
                        },
                        "token_out": {
                            "type": "string",
                            "description": "Token to buy (e.g. 'USDC', 'ETH', 'UNI')"
                        },
                        "amount": {
                            "type": "string",
                            "description": "Amount of token_in to swap (e.g. '0.1', '50')"
                        },
                        "reason": {
                            "type": "string",
                            "description": "Reason for this swap (shown in approval modal and trade history)"
                        },
                        "fee_tier": {
                            "type": "integer",
                            "description": "Uniswap V3 fee tier: 100, 500, 3000 (default), or 10000"
                        },
                        "slippage_bps": {
                            "type": "integer",
                            "description": "Slippage tolerance in basis points. Default: 50 (0.5%). Max: 500 (5%)"
                        }
                    },
                    "required": ["token_in", "token_out", "amount", "reason"]
                }),
            },
        }
    }

    pub fn dex_portfolio() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "dex_portfolio".into(),
                description: "Get a complete portfolio view: ETH balance + all known ERC-20 token balances + network info.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "tokens": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Additional ERC-20 contract addresses to check beyond the built-in list"
                        }
                    },
                    "required": []
                }),
            },
        }
    }

    pub fn dex_token_info() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "dex_token_info".into(),
                description: "Get comprehensive on-chain info about any ERC-20 token by its contract address. Reads name, symbol, decimals, total supply, owner, contract code size, and tests swap viability on Uniswap V3. No website scraping — queries the blockchain directly via RPC.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "token_address": {
                            "type": "string",
                            "description": "The ERC-20 contract address to analyze (0x-prefixed, 42 chars)"
                        }
                    },
                    "required": ["token_address"]
                }),
            },
        }
    }

    pub fn dex_check_token() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "dex_check_token".into(),
                description: "Run automated safety checks on a token contract before trading. Tests: contract verification, ERC-20 compliance, ownership renouncement, HONEYPOT detection (simulates buy AND sell on Uniswap), round-trip tax analysis. Returns a risk score 0-30 and explicit honeypot verdict. Always run this before trading any new token.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "token_address": {
                            "type": "string",
                            "description": "The ERC-20 contract address to safety-check (0x-prefixed, 42 chars)"
                        }
                    },
                    "required": ["token_address"]
                }),
            },
        }
    }

    pub fn dex_search_token() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "dex_search_token".into(),
                description: "Search for tokens by name or symbol to find their contract addresses, prices, volume, and liquidity. Uses the DexScreener API (JSON, not web scraping). Returns contract addresses you can pass to dex_check_token and dex_token_info. Supports all chains (Ethereum, Base, Arbitrum, etc.).".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Token name or symbol to search for (e.g. 'KIMCHI', 'pepe', 'uniswap')"
                        },
                        "chain": {
                            "type": "string",
                            "description": "Optional: filter results to a specific chain (e.g. 'base', 'ethereum', 'arbitrum')"
                        },
                        "max_results": {
                            "type": "integer",
                            "description": "Maximum results to return (default 10, max 25)"
                        }
                    },
                    "required": ["query"]
                }),
            },
        }
    }

    pub fn dex_watch_wallet() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "dex_watch_wallet".into(),
                description: "Monitor any wallet address: shows ETH balance, known token holdings, and recent ERC-20 transfers (buys/sells). Use this to track smart money wallets, alpha traders, and whale activity. Scans Transfer event logs directly from the blockchain.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "wallet_address": {
                            "type": "string",
                            "description": "The wallet address to monitor (0x-prefixed)"
                        },
                        "blocks_back": {
                            "type": "integer",
                            "description": "How many blocks back to scan for transfers (default 1000, ~3 hours on mainnet). Use 5000+ for deeper history."
                        },
                        "tokens": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Additional token contract addresses to check holdings for"
                        }
                    },
                    "required": ["wallet_address"]
                }),
            },
        }
    }

    pub fn dex_whale_transfers() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "dex_whale_transfers".into(),
                description: "Scan recent large transfers of a specific token to detect whale accumulation or distribution. Shows the biggest transfers, identifies top accumulators (smart money buying) and top distributors (insiders selling). Essential for spotting whale moves before retail follows.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "token_address": {
                            "type": "string",
                            "description": "The ERC-20 token contract address to scan"
                        },
                        "blocks_back": {
                            "type": "integer",
                            "description": "How many blocks back to scan (default 2000). Use higher for more history."
                        },
                        "min_amount": {
                            "type": "string",
                            "description": "Minimum transfer amount to show (in token units, e.g. '1000000'). Filters out small transfers."
                        }
                    },
                    "required": ["token_address"]
                }),
            },
        }
    }

    pub fn dex_top_traders() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "dex_top_traders".into(),
                description: "Analyze on-chain Transfer events for a token to discover the most profitable wallets — smart DEX traders, rotators, and early movers. Profiles each wallet by: total bought, total sold, estimated PnL (tokens), trade count, sell/buy ratio, timing (early vs late entry), and trader classification (Accumulator, Profit Taker, Rotator, Active Trader, Early Buyer). Use this to find alpha wallets worth monitoring with dex_watch_wallet.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "token_address": {
                            "type": "string",
                            "description": "The ERC-20 token contract address to analyze traders for"
                        },
                        "blocks_back": {
                            "type": "integer",
                            "description": "How many blocks back to scan (default 5000). More blocks = deeper history but slower."
                        },
                        "min_trades": {
                            "type": "integer",
                            "description": "Minimum number of trades for a wallet to be included (default 2). Higher = filters out one-time buyers."
                        }
                    },
                    "required": ["token_address"]
                }),
            },
        }
    }

    pub fn dex_trending() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "dex_trending".into(),
                description: "Get trending and recently boosted tokens from DexScreener. Shows tokens that are gaining attention — boosted listings and new token profiles. No API key needed. Use chain filter to focus on specific networks (ethereum, base, solana, etc.). Follow up with dex_check_token for safety and dex_top_traders to find who's trading profitably.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "chain": {
                            "type": "string",
                            "description": "Optional: filter to a specific chain (e.g. 'ethereum', 'base', 'solana', 'arbitrum')"
                        },
                        "max_results": {
                            "type": "integer",
                            "description": "Maximum results per category (default 20, max 50)"
                        }
                    },
                    "required": []
                }),
            },
        }
    }

    // ── Solana DEX / Jupiter tools ─────────────────────────────────────

    pub fn sol_wallet_create() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "sol_wallet_create".into(),
                description: "Create a new self-custody Solana wallet (ed25519). The private key is encrypted and stored in the OS keychain vault — you never see it. Returns the wallet address.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
        }
    }

    pub fn sol_balance() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "sol_balance".into(),
                description: "Check SOL and SPL token balances for the Solana wallet. If no token specified, shows SOL and all tokens with non-zero balances.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "token": {
                            "type": "string",
                            "description": "Specific token to check (e.g. 'USDC', 'BONK', 'JUP', or a mint address). Omit to check all tokens."
                        }
                    },
                    "required": []
                }),
            },
        }
    }

    pub fn sol_quote() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "sol_quote".into(),
                description: "Get a swap quote from Jupiter aggregator on Solana without executing. Shows expected output amount, exchange rate, price impact, and route. ALWAYS use this before sol_swap.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "token_in": {
                            "type": "string",
                            "description": "Token to sell (e.g. 'SOL', 'USDC', 'BONK', or mint address)"
                        },
                        "token_out": {
                            "type": "string",
                            "description": "Token to buy (e.g. 'USDC', 'SOL', 'JUP', or mint address)"
                        },
                        "amount": {
                            "type": "string",
                            "description": "Amount of token_in to swap (e.g. '1.5', '100')"
                        },
                        "slippage_bps": {
                            "type": "integer",
                            "description": "Slippage tolerance in basis points. Default: 50 (0.5%). Max: 500 (5%)"
                        }
                    },
                    "required": ["token_in", "token_out", "amount"]
                }),
            },
        }
    }

    pub fn sol_swap() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "sol_swap".into(),
                description: "Execute a token swap on Solana via Jupiter aggregator. REQUIRES USER APPROVAL. Gets a quote from Jupiter, builds and signs the transaction, then broadcasts it. The private key never leaves the vault. Supports all Solana tokens with Jupiter liquidity.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "token_in": {
                            "type": "string",
                            "description": "Token to sell (e.g. 'SOL', 'USDC', 'BONK')"
                        },
                        "token_out": {
                            "type": "string",
                            "description": "Token to buy (e.g. 'USDC', 'SOL', 'JUP')"
                        },
                        "amount": {
                            "type": "string",
                            "description": "Amount of token_in to swap (e.g. '0.5', '100')"
                        },
                        "reason": {
                            "type": "string",
                            "description": "Reason for this swap (shown in approval modal and trade history)"
                        },
                        "slippage_bps": {
                            "type": "integer",
                            "description": "Slippage tolerance in basis points. Default: 50 (0.5%). Max: 500 (5%)"
                        }
                    },
                    "required": ["token_in", "token_out", "amount", "reason"]
                }),
            },
        }
    }

    pub fn sol_portfolio() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "sol_portfolio".into(),
                description: "Get a complete Solana portfolio view: SOL balance + all SPL token balances + network info.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "tokens": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Additional SPL token mint addresses to check beyond auto-detected holdings"
                        }
                    },
                    "required": []
                }),
            },
        }
    }

    pub fn sol_token_info() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "sol_token_info".into(),
                description: "Get on-chain info about any SPL token: decimals, total supply, mint authority, freeze authority, and token program. Queries the blockchain directly. Use a mint address or known symbol (SOL, USDC, USDT, BONK, JUP, etc.).".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "mint_address": {
                            "type": "string",
                            "description": "The SPL token mint address or known symbol (e.g. 'USDC', 'BONK', or a base58 mint address)"
                        }
                    },
                    "required": ["mint_address"]
                }),
            },
        }
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
        /// The actual model that responded (from the API, not config)
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
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
    pub agent_id: Option<String>,
    /// Optional list of allowed tool names. If provided, only these tools
    /// will be offered to the AI model. Enforced by per-agent tool policies.
    #[serde(default)]
    pub tool_filter: Option<Vec<String>>,
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
    /// Original filename (optional)
    #[serde(default)]
    pub name: Option<String>,
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
    /// The actual model name returned by the API (proof of which model responded)
    pub model: Option<String>,
    /// Gemini thought parts that arrived alongside function calls (must be echoed back)
    pub thought_parts: Vec<ThoughtPart>,
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
    /// Agent that created this memory (None = shared/global).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
}

/// Trading policy for auto-approve guidelines.
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

fn default_max_trade() -> f64 { 100.0 }
fn default_max_daily() -> f64 { 500.0 }

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

// ── Model Routing (Multi-Model Agent System) ──────────────────────────

/// Defines which models to use for different agent roles.
/// With a single API key (e.g. Gemini), you can route the boss agent
/// to a powerful model and sub-agents to cheaper/faster models.
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
}

impl Default for ModelRouting {
    fn default() -> Self {
        ModelRouting {
            boss_model: None,
            worker_model: None,
            specialty_models: std::collections::HashMap::new(),
            agent_models: std::collections::HashMap::new(),
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
}

// ── Engine State ───────────────────────────────────────────────────────

fn default_user_timezone() -> String {
    "America/Chicago".to_string()
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
}

fn default_max_concurrent_runs() -> u32 { 4 }

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
            max_tool_rounds: 50,
            tool_timeout_secs: 300,
            user_timezone: default_user_timezone(),
            model_routing: ModelRouting::default(),
            max_concurrent_runs: default_max_concurrent_runs(),
        }
    }
}

// ── Tasks ──────────────────────────────────────────────────────────────

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

// ── Orchestrator: Projects ────────────────────────────────────────────

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
