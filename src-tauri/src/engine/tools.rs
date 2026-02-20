// Paw Agent Engine — ToolDefinition constructors & builtins
// Extracted from engine/types.rs — impl blocks for ToolDefinition.
// The struct itself lives in crate::atoms::types.

#[allow(clippy::too_many_lines)]

use crate::atoms::types::*;

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
            Self::agent_list(),
            Self::agent_skills(),
            Self::agent_skill_assign(),
            Self::skill_search(),
            Self::skill_install(),
            Self::skill_list(),
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

    // ── Agent Management tools (boss agent) ────────────────────────────

    pub fn agent_list() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "agent_list".into(),
                description: "List all agents with their roles, models, and skill counts.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
        }
    }

    pub fn agent_skills() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "agent_skills".into(),
                description: "View the community skills assigned to a specific agent. Shows each skill's name, description, enabled status, and source.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "agent_id": {
                            "type": "string",
                            "description": "The agent ID to inspect (e.g. 'crypto-cat', 'default')"
                        }
                    },
                    "required": ["agent_id"]
                }),
            },
        }
    }

    pub fn agent_skill_assign() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "agent_skill_assign".into(),
                description: "Add or remove a community skill from a specific agent. Use 'add' to assign or 'remove' to unassign. The skill must already be installed.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "skill_id": {
                            "type": "string",
                            "description": "The community skill ID (e.g. 'anthropics/skills/marketing-strategy')"
                        },
                        "agent_id": {
                            "type": "string",
                            "description": "The agent ID to assign/unassign the skill to/from"
                        },
                        "action": {
                            "type": "string",
                            "enum": ["add", "remove"],
                            "description": "'add' to give the agent this skill, 'remove' to take it away"
                        }
                    },
                    "required": ["skill_id", "agent_id", "action"]
                }),
            },
        }
    }

    // ── Community Skills tools ──────────────────────────────────────────

    pub fn skill_search() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "skill_search".into(),
                description: "Search for community agent skills by keyword. Finds open-source SKILL.md files that teach agents new capabilities. Returns a list of available skills with name, source repo, and install path. You can chain this with skill_install to install results.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search keywords (e.g. 'marketing', 'trading', 'supabase', 'nextjs')"
                        }
                    },
                    "required": ["query"]
                }),
            },
        }
    }

    pub fn skill_install() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "skill_install".into(),
                description: "Install a community skill from a GitHub repository. The skill will be scoped to YOUR agent only and enabled immediately. Use the source and path values from skill_search results. You can call this multiple times to install several skills in one turn.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "source": {
                            "type": "string",
                            "description": "GitHub repo in owner/repo format (e.g. 'vercel-labs/agent-skills')"
                        },
                        "path": {
                            "type": "string",
                            "description": "Path to the SKILL.md file within the repo (from skill_search results)"
                        }
                    },
                    "required": ["source", "path"]
                }),
            },
        }
    }

    pub fn skill_list() -> Self {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "skill_list".into(),
                description: "List community skills installed for YOUR agent. Shows name, description, enabled status, source, and scope.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
        }
    }
}
