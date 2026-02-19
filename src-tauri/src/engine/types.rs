// Paw Agent Engine — Core types
// Struct/enum definitions have moved to crate::atoms::types.
// All impl blocks, free functions, and re-exports remain here.
// Downstream code uses `use crate::engine::types::*` unchanged.

use serde::{Deserialize, Serialize};
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


/// A Gemini "thought" part that must be echoed back with function calls




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
                    tools.push(Self::dex_transfer());
                }
                "solana_dex" => {
                    tools.push(Self::sol_wallet_create());
                    tools.push(Self::sol_balance());
                    tools.push(Self::sol_quote());
                    tools.push(Self::sol_swap());
                    tools.push(Self::sol_portfolio());
                    tools.push(Self::sol_token_info());
                    tools.push(Self::sol_transfer());
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

    pub fn dex_transfer() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "dex_transfer".into(),
                description: "Transfer ETH or ERC-20 tokens from your DEX wallet to any external Ethereum address. For ETH: sends native ETH. For ERC-20 tokens: calls transfer() on the token contract. REQUIRES USER APPROVAL. Make sure the wallet has enough ETH for gas fees.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "currency": {
                            "type": "string",
                            "description": "Token to send: 'ETH' for native Ether, or a token symbol (USDC, USDT, DAI, WBTC, etc.) or ERC-20 contract address"
                        },
                        "amount": {
                            "type": "string",
                            "description": "Amount to send in human-readable units (e.g. '0.5' for 0.5 ETH, '100' for 100 USDC)"
                        },
                        "to_address": {
                            "type": "string",
                            "description": "Recipient Ethereum address (0x-prefixed, 42 characters)"
                        },
                        "reason": {
                            "type": "string",
                            "description": "Brief explanation of why this transfer is being made"
                        }
                    },
                    "required": ["currency", "amount", "to_address", "reason"]
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

    pub fn sol_transfer() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "sol_transfer".into(),
                description: "Transfer SOL or SPL tokens from your Solana wallet to any external Solana address. For SOL: sends native SOL via System Program. For SPL tokens: transfers via Token Program (creates recipient token account if needed). REQUIRES USER APPROVAL. Wallet needs SOL for transaction fees (~0.005 SOL).".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "currency": {
                            "type": "string",
                            "description": "Token to send: 'SOL' for native SOL, or a token symbol (USDC, USDT, BONK, JUP, etc.) or SPL mint address"
                        },
                        "amount": {
                            "type": "string",
                            "description": "Amount to send in human-readable units (e.g. '1.5' for 1.5 SOL, '100' for 100 USDC)"
                        },
                        "to_address": {
                            "type": "string",
                            "description": "Recipient Solana address (base58-encoded public key)"
                        },
                        "reason": {
                            "type": "string",
                            "description": "Brief explanation of why this transfer is being made"
                        }
                    },
                    "required": ["currency", "amount", "to_address", "reason"]
                }),
            },
        }
    }
}

// ── Tool Execution Result ──────────────────────────────────────────────


// ── Streaming Events (Tauri → Frontend) ────────────────────────────────


// ── Session ────────────────────────────────────────────────────────────



// ── Chat Send Request (from frontend) ──────────────────────────────────


/// Attachment sent with a chat message (images, files).

// ── Chat Send Response (to frontend) ───────────────────────────────────


// ── Provider API response shapes ───────────────────────────────────────

/// Unified streaming chunk from any provider


/// Token usage reported by the API (for metering).

// ── Model Pricing ──────────────────────────────────────────────────────

/// Per-million-token pricing for known models.
/// (input_per_mtok, output_per_mtok)

/// Look up pricing for a model. Falls back to cheap defaults.
pub fn model_price(model: &str) -> ModelPrice {
    // Normalize: strip provider prefixes like "anthropic/"
    let m = model.split('/').last().unwrap_or(model);
    match m {
        // Anthropic
        s if s.starts_with("claude-3-haiku") => ModelPrice { input: 0.25, output: 1.25 },
        s if s.starts_with("claude-haiku-4") => ModelPrice { input: 1.00, output: 5.00 },
        s if s.starts_with("claude-sonnet-4") || s.starts_with("claude-3-5-sonnet") || s.starts_with("claude-3-sonnet") =>
            ModelPrice { input: 3.00, output: 15.00 },
        s if s.starts_with("claude-opus-4") || s.starts_with("claude-3-opus") =>
            ModelPrice { input: 15.00, output: 75.00 },
        // Google
        s if s.starts_with("gemini-2.0-flash") || s.starts_with("gemini-2.5-flash") =>
            ModelPrice { input: 0.15, output: 0.60 },
        s if s.starts_with("gemini-2.5-pro") || s.starts_with("gemini-1.5-pro") || s.starts_with("gemini-pro") =>
            ModelPrice { input: 1.25, output: 10.00 },
        // OpenAI
        s if s.starts_with("gpt-4o-mini") || s.starts_with("gpt-4.1-mini") || s.starts_with("gpt-4.1-nano") =>
            ModelPrice { input: 0.15, output: 0.60 },
        s if s.starts_with("gpt-4o") || s.starts_with("gpt-4.1") =>
            ModelPrice { input: 2.50, output: 10.00 },
        s if s.starts_with("o4-mini") || s.starts_with("o3-mini") =>
            ModelPrice { input: 1.10, output: 4.40 },
        s if s.starts_with("o3") || s.starts_with("o1") =>
            ModelPrice { input: 10.00, output: 40.00 },
        // DeepSeek
        s if s.starts_with("deepseek-chat") || s.starts_with("deepseek-v3") =>
            ModelPrice { input: 0.27, output: 1.10 },
        s if s.starts_with("deepseek-reasoner") || s.starts_with("deepseek-r1") =>
            ModelPrice { input: 0.55, output: 2.19 },
        // Fallback: assume cheap model
        _ => ModelPrice { input: 0.50, output: 2.00 },
    }
}

/// Estimate USD cost given token counts and model name.
/// Accounts for Anthropic cache tokens: reads charged at 10%, creation at 25%.
pub fn estimate_cost_usd(model: &str, input: u64, output: u64, cache_read: u64, cache_create: u64) -> f64 {
    let p = model_price(model);
    // Regular input tokens (subtract cached from total input for accurate costing)
    let regular_input = input.saturating_sub(cache_read + cache_create);
    let input_cost = (regular_input as f64 * p.input / 1_000_000.0)
        + (cache_read as f64 * p.input * 0.10 / 1_000_000.0)   // 90% discount
        + (cache_create as f64 * p.input * 0.25 / 1_000_000.0); // 75% discount on write
    let output_cost = output as f64 * p.output / 1_000_000.0;
    input_cost + output_cost
}

// ── Task Complexity Classification ─────────────────────────────────────

/// How complex a user message is — determines model tier.

/// Classify a user message's complexity to choose the right model tier.
/// Looks for signals of multi-step reasoning, code, analysis, etc.
pub fn classify_task_complexity(message: &str) -> TaskComplexity {
    let msg = message.to_lowercase();
    let len = msg.len();

    // Long messages are usually complex
    if len > 1500 { return TaskComplexity::Complex; }

    // Code-related signals
    let code_signals = [
        "write code", "implement", "refactor", "debug", "fix the bug",
        "create a function", "write a script", "build a", "architect",
        "```", "code review", "unit test", "write test",
        "optimize", "performance", "algorithm",
    ];

    // Analysis / reasoning signals
    let reasoning_signals = [
        "analyze", "compare", "explain why", "reason", "think through",
        "pros and cons", "trade-off", "evaluate", "assess",
        "plan", "strategy", "design", "architecture",
        "step by step", "break down", "complex",
        "research", "investigate", "deep dive",
        "write a report", "summarize", "synthesis",
    ];

    // Multi-step signals
    let multi_step = [
        "and then", "after that", "first,", "second,", "third,",
        "steps:", "1.", "2.", "3.",
        "multiple", "several", "all of",
    ];

    for signal in code_signals.iter().chain(reasoning_signals.iter()).chain(multi_step.iter()) {
        if msg.contains(signal) {
            return TaskComplexity::Complex;
        }
    }

    TaskComplexity::Simple
}

// ── Agent Files (Soul / Persona) ───────────────────────────────────────

/// An agent personality file (SOUL.md, AGENTS.md, USER.md, etc.)

/// Standard agent files that define soul / persona.

// ── Memory (Long-term Semantic) ────────────────────────────────────────

/// A single memory entry stored with its embedding vector.

/// An open trading position with stop-loss / take-profit targets.

/// Trading policy for auto-approve guidelines.
// serde default helpers for TradingPolicy live in crate::atoms::types
use crate::atoms::types::{default_max_trade, default_max_daily};

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

/// Statistics about the memory store.

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



