// Pawz Agent Engine — Skill Vault
// Secure credential storage and skill management for agent tools.
//
// Architecture:
// - Skills are instruction sets that teach the agent how to use specific CLIs/APIs.
// - "Vault" skills (email, slack, github, etc.) have dedicated tool functions + encrypted credentials.
// - "Instruction" skills just inject knowledge into the agent's system prompt.
// - The agent uses exec/fetch (tools it already has) to interact with most CLIs/APIs.
// - Credentials are stored encrypted in SQLite, with the encryption key in OS keychain.

use log::{info, warn};
use serde::{Deserialize, Serialize};

/// Skill categories for organization in the UI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SkillCategory {
    /// Core skills with dedicated tool functions and credential vault
    Vault,
    /// CLI tools the agent can use via exec
    Cli,
    /// API integrations the agent can use via fetch/exec
    Api,
    /// Productivity: notes, reminders, project management
    Productivity,
    /// Media: audio, video, images
    Media,
    /// Smart home and IoT
    SmartHome,
    /// Communication: messaging, calls
    Communication,
    /// Development: coding, CI/CD
    Development,
    /// System: security, monitoring
    System,
}

/// A skill definition — describes what the skill does and what it needs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub category: SkillCategory,
    /// Credentials this skill requires (name → description). Empty for instruction-only skills.
    pub required_credentials: Vec<CredentialField>,
    /// The dedicated tool names this skill provides (vault skills only).
    pub tool_names: Vec<String>,
    /// CLI binaries required for this skill (checked on PATH).
    pub required_binaries: Vec<String>,
    /// Environment variables required for this skill.
    pub required_env_vars: Vec<String>,
    /// How to install missing dependencies (shown to user).
    pub install_hint: String,
    /// Instructions injected into the agent's system prompt when enabled.
    /// This teaches the agent HOW to use the skill's CLI/API.
    pub agent_instructions: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialField {
    pub key: String,
    pub label: String,
    pub description: String,
    /// If true, this is a required field. If false, optional.
    pub required: bool,
    /// Hint text for the input field.
    pub placeholder: String,
}

/// A stored skill record (from DB) — tracks enabled state and credential status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRecord {
    pub skill_id: String,
    pub enabled: bool,
    /// Which credential keys have been set (not the values — just the key names).
    pub configured_keys: Vec<String>,
    pub updated_at: String,
}

/// Skill status for the frontend — combines definition + stored state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillStatus {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub category: SkillCategory,
    pub enabled: bool,
    pub required_credentials: Vec<CredentialField>,
    pub configured_credentials: Vec<String>,
    pub missing_credentials: Vec<String>,
    pub required_binaries: Vec<String>,
    pub missing_binaries: Vec<String>,
    pub required_env_vars: Vec<String>,
    pub missing_env_vars: Vec<String>,
    pub install_hint: String,
    pub is_ready: bool,
    pub tool_names: Vec<String>,
    pub has_instructions: bool,
    /// Default agent instructions (from builtin definition).
    pub default_instructions: String,
    /// Custom user-edited instructions (if any). Empty string = using defaults.
    pub custom_instructions: String,
}

// ── Built-in Skill Definitions ─────────────────────────────────────────

pub fn builtin_skills() -> Vec<SkillDefinition> {
    vec![
        // ───── VAULT SKILLS (dedicated tool functions + credentials) ─────

        SkillDefinition {
            id: "email".into(),
            name: "Email".into(),
            description: "Send and read emails via IMAP/SMTP".into(),
            icon: "📧".into(),
            category: SkillCategory::Vault,
            required_credentials: vec![
                CredentialField { key: "SMTP_HOST".into(), label: "SMTP Host".into(), description: "SMTP server hostname (e.g. smtp.gmail.com)".into(), required: true, placeholder: "smtp.gmail.com".into() },
                CredentialField { key: "SMTP_PORT".into(), label: "SMTP Port".into(), description: "SMTP port (587 for TLS, 465 for SSL)".into(), required: true, placeholder: "587".into() },
                CredentialField { key: "SMTP_USER".into(), label: "Email Address".into(), description: "Your email address for authentication".into(), required: true, placeholder: "you@gmail.com".into() },
                CredentialField { key: "SMTP_PASSWORD".into(), label: "App Password".into(), description: "App-specific password (not your main password)".into(), required: true, placeholder: "xxxx xxxx xxxx xxxx".into() },
                CredentialField { key: "IMAP_HOST".into(), label: "IMAP Host".into(), description: "IMAP server for reading mail (e.g. imap.gmail.com)".into(), required: false, placeholder: "imap.gmail.com".into() },
                CredentialField { key: "IMAP_PORT".into(), label: "IMAP Port".into(), description: "IMAP port (993 for SSL)".into(), required: false, placeholder: "993".into() },
            ],
            tool_names: vec!["email_send".into(), "email_read".into()],
            required_binaries: vec![], required_env_vars: vec![], install_hint: String::new(),
            agent_instructions: "You can send and read emails. Use email_send to compose and send messages. Use email_read to check inbox. Always confirm recipients before sending.".into(),
        },
        SkillDefinition {
            id: "slack".into(),
            name: "Slack".into(),
            description: "Send messages to Slack channels and DMs".into(),
            icon: "💬".into(),
            category: SkillCategory::Vault,
            required_credentials: vec![
                CredentialField { key: "SLACK_BOT_TOKEN".into(), label: "Bot Token".into(), description: "Slack Bot User OAuth Token (xoxb-...)".into(), required: true, placeholder: "xoxb-your-slack-bot-token".into() },
                CredentialField { key: "SLACK_DEFAULT_CHANNEL".into(), label: "Default Channel".into(), description: "Default channel ID to post to (optional)".into(), required: false, placeholder: "C0123456789".into() },
            ],
            tool_names: vec!["slack_send".into(), "slack_read".into()],
            required_binaries: vec![], required_env_vars: vec![], install_hint: String::new(),
            agent_instructions: "You can post to and read from Slack channels. Use slack_send to post messages. Use slack_read to fetch recent messages from a channel.".into(),
        },
        SkillDefinition {
            id: "telegram".into(),
            name: "Telegram".into(),
            description: "Send proactive messages to Telegram users via the bot bridge. No extra credentials needed — uses the Telegram bot token configured in the channel bridge.".into(),
            icon: "✈️".into(),
            category: SkillCategory::Vault,
            required_credentials: vec![],
            tool_names: vec!["telegram_send".into(), "telegram_read".into()],
            required_binaries: vec![], required_env_vars: vec![], install_hint: String::new(),
            agent_instructions: "You can send proactive messages to Telegram users. Use telegram_send to push messages — specify a username or it defaults to the owner. Use telegram_read to check bridge status and known users. The Telegram bot must be set up in channel settings first, and the user must have messaged the bot at least once.".into(),
        },
        SkillDefinition {
            id: "github".into(),
            name: "GitHub".into(),
            description: "Create issues, PRs, read repos, manage projects via gh CLI and GitHub API".into(),
            icon: "🐙".into(),
            category: SkillCategory::Vault,
            required_credentials: vec![
                CredentialField { key: "GITHUB_TOKEN".into(), label: "Personal Access Token".into(), description: "GitHub PAT with repo access (ghp_...)".into(), required: true, placeholder: "ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".into() },
            ],
            tool_names: vec!["github_api".into()],
            required_binaries: vec!["gh".into()],
            required_env_vars: vec![], install_hint: "brew install gh".into(),
            agent_instructions: r#"You have GitHub access via the github_api tool and the `gh` CLI.
For quick operations use the `gh` CLI via exec: `gh issue list`, `gh pr create`, `gh repo view`, `gh api ...`.
For complex API calls use github_api tool which sends authenticated requests to api.github.com.
Available gh commands: issue (list/create/view/close), pr (list/create/view/merge/checkout), run (list/view/watch), repo (view/clone/fork), api (raw REST calls)."#.into(),
        },
        SkillDefinition {
            id: "rest_api".into(),
            name: "REST API".into(),
            description: "Make authenticated API calls to any REST service".into(),
            icon: "🔌".into(),
            category: SkillCategory::Vault,
            required_credentials: vec![
                CredentialField { key: "API_BASE_URL".into(), label: "Base URL".into(), description: "The base URL for the API".into(), required: true, placeholder: "https://api.example.com/v1".into() },
                CredentialField { key: "API_KEY".into(), label: "API Key".into(), description: "Authentication key/token".into(), required: true, placeholder: "sk-...".into() },
                CredentialField { key: "API_AUTH_HEADER".into(), label: "Auth Header".into(), description: "Header name (default: Authorization)".into(), required: false, placeholder: "Authorization".into() },
                CredentialField { key: "API_AUTH_PREFIX".into(), label: "Auth Prefix".into(), description: "Key prefix (default: Bearer)".into(), required: false, placeholder: "Bearer".into() },
            ],
            tool_names: vec!["rest_api_call".into()],
            required_binaries: vec![], required_env_vars: vec![], install_hint: String::new(),
            agent_instructions: "You can make authenticated REST API calls to a pre-configured service. Use rest_api_call with method, path, and optional body/headers.".into(),
        },
        SkillDefinition {
            id: "webhook".into(),
            name: "Webhooks".into(),
            description: "Send data to webhook URLs (Zapier, IFTTT, n8n, custom)".into(),
            icon: "🪝".into(),
            category: SkillCategory::Vault,
            required_credentials: vec![
                CredentialField { key: "WEBHOOK_URL".into(), label: "Webhook URL".into(), description: "The webhook endpoint URL".into(), required: true, placeholder: "https://hooks.zapier.com/hooks/catch/...".into() },
                CredentialField { key: "WEBHOOK_SECRET".into(), label: "Secret (optional)".into(), description: "Shared secret for webhook signing".into(), required: false, placeholder: "whsec_...".into() },
            ],
            tool_names: vec!["webhook_send".into()],
            required_binaries: vec![], required_env_vars: vec![], install_hint: String::new(),
            agent_instructions: "You can send JSON payloads to configured webhooks. Use webhook_send with a JSON body. Great for triggering Zapier/IFTTT/n8n automations.".into(),
        },
        SkillDefinition {
            id: "discord".into(),
            name: "Discord".into(),
            description: "Send messages and manage Discord servers via bot token".into(),
            icon: "🎮".into(),
            category: SkillCategory::Vault,
            required_credentials: vec![
                CredentialField { key: "DISCORD_BOT_TOKEN".into(), label: "Bot Token".into(), description: "Discord bot token".into(), required: true, placeholder: "MTIz...".into() },
                CredentialField { key: "DISCORD_DEFAULT_CHANNEL".into(), label: "Default Channel ID".into(), description: "Default channel to post to".into(), required: false, placeholder: "1234567890".into() },
            ],
            tool_names: vec![],
            required_binaries: vec![], required_env_vars: vec![], install_hint: String::new(),
            agent_instructions: r#"You have Discord bot access. Use the fetch tool to interact with the Discord API:
- POST https://discord.com/api/v10/channels/{channel_id}/messages with {"content":"message"}
- GET https://discord.com/api/v10/channels/{channel_id}/messages?limit=10
- Headers: Authorization: Bot {token}, Content-Type: application/json
Use exec to read the DISCORD_BOT_TOKEN and DISCORD_DEFAULT_CHANNEL from environment if needed."#.into(),
        },
        SkillDefinition {
            id: "coinbase".into(),
            name: "Coinbase (CDP Agentic Wallet)".into(),
            description: "Trade crypto, manage wallets, and check prices via Coinbase Developer Platform".into(),
            icon: "🪙".into(),
            category: SkillCategory::Vault,
            required_credentials: vec![
                CredentialField { key: "CDP_API_KEY_NAME".into(), label: "API Key Name".into(), description: "The 'name' field from cdp_api_key.json (e.g. organizations/abc123/apiKeys/xyz789). For older keys, use the 'id' field.".into(), required: true, placeholder: "organizations/abc123-def/apiKeys/xyz789-...".into() },
                CredentialField { key: "CDP_API_KEY_SECRET".into(), label: "API Secret (Private Key)".into(), description: "The 'privateKey' field from cdp_api_key.json. Paste the raw base64 string or PEM block exactly as given.".into(), required: true, placeholder: "+jSZpC...base64...Wg==".into() },
            ],
            tool_names: vec!["coinbase_prices".into(), "coinbase_balance".into(), "coinbase_wallet_create".into(), "coinbase_trade".into(), "coinbase_transfer".into()],
            required_binaries: vec![], required_env_vars: vec![], install_hint: "Get API keys at portal.cdp.coinbase.com".into(),
            agent_instructions: r#"You have Coinbase CDP (Developer Platform) access for crypto trading and wallet management.

CRITICAL: Credentials are already configured and injected automatically by the engine. Authentication (Ed25519 JWT signing) is handled for you. Do NOT:
- Read source code files (.rs, .ts, etc.) to understand how tools work
- Read or inspect cdp_api_key.json or any credential/key files
- Tell the user their key format is wrong or suggest they need a different key type
- Guess at authentication issues — just call the tool and report the exact error

When the user asks to check balances, trade, or do anything with Coinbase: IMMEDIATELY call the appropriate tool below. Do not investigate first.

Available tools:
- **coinbase_prices**: Get current spot prices for crypto assets (e.g. BTC, ETH). Just call it.
- **coinbase_balance**: Check wallet balances. Just call it.
- **coinbase_wallet_create**: Create a new MPC wallet. Requires user approval.
- **coinbase_trade**: Execute a buy/sell order. ALWAYS requires user approval. Include clear reasoning.
- **coinbase_transfer**: Send crypto to an address. ALWAYS requires user approval. Double-check addresses.

Risk Management Rules:
- NEVER risk more than 2% of portfolio on a single trade
- Always state your reasoning before proposing a trade
- Always include a stop-loss level when proposing trades
- Prefer limit orders over market orders when possible
- Check balances before proposing any trade
- If the user hasn't set risk parameters, ask before trading"#.into(),
        },
        SkillDefinition {
            id: "notion".into(),
            name: "Notion".into(),
            description: "Create and manage Notion pages, databases, and blocks".into(),
            icon: "📝".into(),
            category: SkillCategory::Api,
            required_credentials: vec![
                CredentialField { key: "NOTION_API_KEY".into(), label: "Integration Token".into(), description: "Notion internal integration token (secret_...)".into(), required: true, placeholder: "secret_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".into() },
            ],
            tool_names: vec![],
            required_binaries: vec![], required_env_vars: vec![], install_hint: "Create an integration at notion.so/my-integrations".into(),
            agent_instructions: r#"You have Notion API access. Use the fetch tool to interact with the Notion API (https://api.notion.com/v1/).
Key endpoints:
- POST /pages — create a page
- PATCH /pages/{id} — update page properties
- POST /databases/{id}/query — query a database
- GET /blocks/{id}/children — get block children
- PATCH /blocks/{id} — update a block
- POST /search — search across all pages/databases
Headers: Authorization: Bearer {token}, Notion-Version: 2022-06-28, Content-Type: application/json
Notion uses rich text blocks. Page content is a list of block objects (paragraph, heading_1, to_do, etc.)."#.into(),
        },
        SkillDefinition {
            id: "trello".into(),
            name: "Trello".into(),
            description: "Manage Trello boards, lists, and cards".into(),
            icon: "📋".into(),
            category: SkillCategory::Api,
            required_credentials: vec![
                CredentialField { key: "TRELLO_API_KEY".into(), label: "API Key".into(), description: "Trello API key from trello.com/app-key".into(), required: true, placeholder: "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".into() },
                CredentialField { key: "TRELLO_TOKEN".into(), label: "Token".into(), description: "Trello authorization token".into(), required: true, placeholder: "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".into() },
            ],
            tool_names: vec![],
            required_binaries: vec![], required_env_vars: vec![], install_hint: "Get API key at trello.com/app-key, then authorize for a token".into(),
            agent_instructions: r#"You have Trello API access. Use the fetch tool with https://api.trello.com/1/ endpoints.
Authentication: append ?key={api_key}&token={token} to all URLs.
Key endpoints:
- GET /members/me/boards — list boards
- GET /boards/{id}/lists — get lists on a board
- GET /lists/{id}/cards — get cards in a list  
- POST /cards — create card (idList, name, desc, due, pos)
- PUT /cards/{id} — update card
- DELETE /cards/{id} — delete card
- POST /cards/{id}/actions/comments — add comment"#.into(),
        },

        // ───── PRODUCTIVITY SKILLS ─────

        SkillDefinition {
            id: "apple_notes".into(),
            name: "Apple Notes".into(),
            description: "Manage Apple Notes on macOS (create, view, edit, search, export)".into(),
            icon: "📝".into(),
            category: SkillCategory::Productivity,
            required_credentials: vec![],
            tool_names: vec![],
            required_binaries: vec!["memo".into()],
            required_env_vars: vec![], install_hint: "brew install memo".into(),
            agent_instructions: r#"You can manage Apple Notes via the `memo` CLI.
Commands: memo list, memo show <id>, memo create <title> --body <text>, memo edit <id> --body <text>,
memo delete <id>, memo search <query>, memo export <id> --format md|html|txt.
Notes are organized in folders. Use memo folders to list, memo create --folder <name>."#.into(),
        },
        SkillDefinition {
            id: "apple_reminders".into(),
            name: "Apple Reminders".into(),
            description: "Manage Apple Reminders on macOS (list, add, complete, delete)".into(),
            icon: "⏰".into(),
            category: SkillCategory::Productivity,
            required_credentials: vec![],
            tool_names: vec![],
            required_binaries: vec!["remindctl".into()],
            required_env_vars: vec![], install_hint: "brew install remindctl".into(),
            agent_instructions: r#"You can manage Apple Reminders via `remindctl`.
Commands: remindctl list [--list <name>], remindctl add <title> [--list <name>] [--due <date>] [--notes <text>],
remindctl complete <id>, remindctl delete <id>, remindctl lists (show all lists).
Date formats: YYYY-MM-DD, YYYY-MM-DD HH:MM, "tomorrow", "next monday"."#.into(),
        },
        SkillDefinition {
            id: "things".into(),
            name: "Things 3".into(),
            description: "Manage Things 3 tasks on macOS via CLI".into(),
            icon: "✅".into(),
            category: SkillCategory::Productivity,
            required_credentials: vec![],
            tool_names: vec![],
            required_binaries: vec!["things".into()],
            required_env_vars: vec![], install_hint: "go install github.com/thingsapi/things3-cli@latest".into(),
            agent_instructions: r#"You can manage Things 3 tasks via the `things` CLI.
Commands: things list [inbox|today|upcoming|anytime|someday|logbook], things add <title> [--notes <text>] [--when <date>] [--deadline <date>] [--project <name>] [--tags <tag1,tag2>],
things complete <id>, things search <query>, things projects, things tags."#.into(),
        },
        SkillDefinition {
            id: "obsidian".into(),
            name: "Obsidian".into(),
            description: "Work with Obsidian vaults (Markdown notes)".into(),
            icon: "💎".into(),
            category: SkillCategory::Productivity,
            required_credentials: vec![],
            tool_names: vec![],
            required_binaries: vec!["obsidian-cli".into()],
            required_env_vars: vec![], install_hint: "brew install obsidian-cli".into(),
            agent_instructions: r#"You can manage Obsidian vaults via `obsidian-cli` and direct file access.
For direct file ops, use read_file/write_file with Markdown in the vault directory.
CLI commands: obsidian-cli search <query> --vault <path>, obsidian-cli list --vault <path>,
obsidian-cli create <name> --vault <path> --content <md>, obsidian-cli open <note>.
Obsidian uses [[wikilinks]], #tags, and YAML frontmatter. Respect existing formatting."#.into(),
        },
        SkillDefinition {
            id: "bear_notes".into(),
            name: "Bear Notes".into(),
            description: "Create, search, and manage Bear notes via CLI".into(),
            icon: "🐻".into(),
            category: SkillCategory::Productivity,
            required_credentials: vec![],
            tool_names: vec![],
            required_binaries: vec!["grizzly".into()],
            required_env_vars: vec![], install_hint: "go install github.com/nicholasgasior/grizzly@latest".into(),
            agent_instructions: r#"You can manage Bear notes via the `grizzly` CLI.
Commands: grizzly list, grizzly search <query>, grizzly show <id>, grizzly create --title <title> --body <md>,
grizzly edit <id> --body <md>, grizzly trash <id>, grizzly tags. Bear uses #tags and Markdown."#.into(),
        },

        // ───── DEVELOPMENT SKILLS ─────

        SkillDefinition {
            id: "tmux".into(),
            name: "tmux".into(),
            description: "Remote-control tmux sessions for interactive CLIs".into(),
            icon: "🧵".into(),
            category: SkillCategory::Development,
            required_credentials: vec![],
            tool_names: vec![],
            required_binaries: vec!["tmux".into()],
            required_env_vars: vec![], install_hint: "brew install tmux".into(),
            agent_instructions: r#"You can control tmux sessions to run long-lived or interactive processes.
Key patterns:
- tmux new-session -d -s <name> '<command>' — start detached session
- tmux send-keys -t <name> '<keys>' Enter — type into session  
- tmux capture-pane -t <name> -p — read current screen output
- tmux kill-session -t <name> — stop session
- tmux list-sessions — see running sessions
Use this for interactive CLIs, REPLs, running servers, or anything that needs persistent state."#.into(),
        },
        SkillDefinition {
            id: "session_logs".into(),
            name: "Session Logs".into(),
            description: "Search and analyze past conversation session logs".into(),
            icon: "📜".into(),
            category: SkillCategory::Development,
            required_credentials: vec![],
            tool_names: vec![],
            required_binaries: vec!["rg".into()],
            required_env_vars: vec![], install_hint: "brew install ripgrep".into(),
            agent_instructions: r#"You can search through past session logs using `rg` (ripgrep) and `jq`.
Use rg to search conversation history files, and jq to parse JSON log entries.
Example: rg "search term" ~/.paw/ --type json | jq '.content'"#.into(),
        },

        // ───── MEDIA SKILLS ─────

        SkillDefinition {
            id: "whisper".into(),
            name: "Whisper (Local)".into(),
            description: "Local speech-to-text transcription (no API key needed)".into(),
            icon: "🎙️".into(),
            category: SkillCategory::Media,
            required_credentials: vec![],
            tool_names: vec![],
            required_binaries: vec!["whisper".into()],
            required_env_vars: vec![], install_hint: "brew install whisper".into(),
            agent_instructions: r#"You can transcribe audio files using OpenAI's Whisper locally (no API key needed).
Usage: whisper <audio_file> --model small --language en --output_format txt
Models: tiny, base, small, medium, large (larger = more accurate, slower).
Supports: mp3, wav, m4a, flac, ogg, opus. Output: txt, vtt, srt, json."#.into(),
        },
        SkillDefinition {
            id: "whisper_api".into(),
            name: "Whisper API".into(),
            description: "Transcribe audio via OpenAI Whisper API".into(),
            icon: "☁️".into(),
            category: SkillCategory::Media,
            required_credentials: vec![
                CredentialField { key: "OPENAI_API_KEY".into(), label: "OpenAI API Key".into(), description: "OpenAI API key for Whisper API".into(), required: true, placeholder: "sk-...".into() },
            ],
            tool_names: vec![],
            required_binaries: vec![], required_env_vars: vec![], install_hint: "Get API key from platform.openai.com".into(),
            agent_instructions: r#"You can transcribe audio using the OpenAI Whisper API.
Use fetch to POST to https://api.openai.com/v1/audio/transcriptions with multipart form data.
Include: file (audio binary), model: "whisper-1", optional: language, response_format (json|text|srt|vtt)."#.into(),
        },
        SkillDefinition {
            id: "image_gen".into(),
            name: "Image Generation".into(),
            description: "Generate images from text using Gemini (Google AI)".into(),
            icon: "🖼️".into(),
            category: SkillCategory::Media,
            required_credentials: vec![
                CredentialField { key: "GEMINI_API_KEY".into(), label: "Gemini API Key".into(), description: "Google AI API key for image generation".into(), required: true, placeholder: "AIza...".into() },
            ],
            tool_names: vec!["image_generate".into()],
            required_binaries: vec![], required_env_vars: vec![], install_hint: "Get API key from aistudio.google.com/apikey".into(),
            agent_instructions: r#"You have an image_generate tool that creates images from text descriptions using Gemini.
Call image_generate with a detailed prompt describing the image you want to create.
The tool returns the file path of the generated image.
Tip: Be descriptive — include style, lighting, composition, colors, and mood in your prompts for best results."#.into(),
        },
        SkillDefinition {
            id: "video_frames".into(),
            name: "Video Frames".into(),
            description: "Extract frames or clips from videos using ffmpeg".into(),
            icon: "🎞️".into(),
            category: SkillCategory::Media,
            required_credentials: vec![],
            tool_names: vec![],
            required_binaries: vec!["ffmpeg".into()],
            required_env_vars: vec![], install_hint: "brew install ffmpeg".into(),
            agent_instructions: r#"You can extract frames, clips, and metadata from video files using ffmpeg.
Key commands:
- ffmpeg -i input.mp4 -vf "select=eq(n\,0)" -vframes 1 frame.png — extract first frame
- ffmpeg -i input.mp4 -ss 00:01:00 -t 10 -c copy clip.mp4 — extract 10s clip at 1 min
- ffmpeg -i input.mp4 -vf fps=1 frames/%04d.png — extract 1 frame per second
- ffprobe -v quiet -print_format json -show_format -show_streams input.mp4 — get metadata
- ffmpeg -i input.mp4 -vf "thumbnail" -vframes 1 thumb.png — auto-select best thumbnail"#.into(),
        },
        SkillDefinition {
            id: "tts_sag".into(),
            name: "ElevenLabs TTS".into(),
            description: "Text-to-speech via ElevenLabs API".into(),
            icon: "🗣️".into(),
            category: SkillCategory::Media,
            required_credentials: vec![
                CredentialField { key: "ELEVENLABS_API_KEY".into(), label: "ElevenLabs API Key".into(), description: "API key from elevenlabs.io".into(), required: true, placeholder: "xi_...".into() },
            ],
            tool_names: vec![],
            required_binaries: vec!["sag".into()],
            required_env_vars: vec![], install_hint: "brew install sag".into(),
            agent_instructions: r#"You can speak text aloud using ElevenLabs TTS via the `sag` CLI.
Usage: sag "text to speak" [--voice <name>] [--model eleven_turbo_v2] [--output file.mp3]
Or use the ElevenLabs API directly: POST https://api.elevenlabs.io/v1/text-to-speech/{voice_id}
with {"text":"...", "model_id":"eleven_turbo_v2"}. Returns audio bytes."#.into(),
        },

        // ───── SMART HOME & IoT ─────

        SkillDefinition {
            id: "hue".into(),
            name: "Philips Hue".into(),
            description: "Control Philips Hue lights and scenes".into(),
            icon: "💡".into(),
            category: SkillCategory::SmartHome,
            required_credentials: vec![],
            tool_names: vec![],
            required_binaries: vec!["openhue".into()],
            required_env_vars: vec![], install_hint: "brew install openhue".into(),
            agent_instructions: r#"You can control Philips Hue lights via the `openhue` CLI.
Commands: openhue get lights, openhue set light <id> --on/--off --brightness <0-100> --color <hex>,
openhue get rooms, openhue get scenes, openhue set scene <id>.
First run: openhue setup (discovers bridge and creates API key)."#.into(),
        },
        SkillDefinition {
            id: "sonos".into(),
            name: "Sonos".into(),
            description: "Control Sonos speakers (play, volume, group)".into(),
            icon: "🔊".into(),
            category: SkillCategory::SmartHome,
            required_credentials: vec![],
            tool_names: vec![],
            required_binaries: vec!["sonos".into()],
            required_env_vars: vec![], install_hint: "go install github.com/sonos/sonoscli@latest".into(),
            agent_instructions: r#"You can control Sonos speakers via the `sonos` CLI.
Commands: sonos status, sonos play, sonos pause, sonos next, sonos prev,
sonos volume <0-100>, sonos group <room1> <room2>, sonos ungroup <room>,
sonos rooms, sonos queue, sonos favorites."#.into(),
        },
        SkillDefinition {
            id: "eight_sleep".into(),
            name: "Eight Sleep".into(),
            description: "Control Eight Sleep pod temperature and alarms".into(),
            icon: "🎛️".into(),
            category: SkillCategory::SmartHome,
            required_credentials: vec![],
            tool_names: vec![],
            required_binaries: vec!["eightctl".into()],
            required_env_vars: vec![], install_hint: "go install github.com/eightctl@latest".into(),
            agent_instructions: r#"You can control Eight Sleep pods via `eightctl`.
Commands: eightctl status, eightctl temp <-10 to 10>, eightctl alarm <HH:MM>,
eightctl schedule list, eightctl schedule set <time> <temp>."#.into(),
        },

        // ───── COMMUNICATION SKILLS ─────

        SkillDefinition {
            id: "whatsapp".into(),
            name: "WhatsApp".into(),
            description: "Send WhatsApp messages and search chat history".into(),
            icon: "📱".into(),
            category: SkillCategory::Communication,
            required_credentials: vec![],
            tool_names: vec![],
            required_binaries: vec!["wacli".into()],
            required_env_vars: vec![], install_hint: "brew install wacli".into(),
            agent_instructions: r#"You can interact with WhatsApp via the `wacli` CLI.
Commands: wacli send <phone> <message>, wacli chats, wacli history <phone> [--limit 20],
wacli search <query>, wacli sync.
Phone numbers should include country code (e.g., +1234567890)."#.into(),
        },
        SkillDefinition {
            id: "imessage".into(),
            name: "iMessage".into(),
            description: "Send iMessages and search chat history on macOS".into(),
            icon: "📨".into(),
            category: SkillCategory::Communication,
            required_credentials: vec![],
            tool_names: vec![],
            required_binaries: vec!["imsg".into()],
            required_env_vars: vec![], install_hint: "brew install imsg".into(),
            agent_instructions: r#"You can manage iMessage on macOS via the `imsg` CLI.
Commands: imsg chats, imsg history <contact> [--limit 20], imsg send <contact> <message>,
imsg search <query>, imsg watch (live stream new messages).
Contacts can be phone numbers or email addresses."#.into(),
        },

        // ───── CLI TOOLS ─────

        SkillDefinition {
            id: "weather".into(),
            name: "Weather".into(),
            description: "Get current weather and forecasts (no API key needed)".into(),
            icon: "🌤️".into(),
            category: SkillCategory::Cli,
            required_credentials: vec![],
            tool_names: vec![],
            required_binaries: vec![],
            required_env_vars: vec![], install_hint: String::new(),
            agent_instructions: r#"You can get weather data without any special tools.
Use web_search or fetch with: curl wttr.in/<city>?format=j1 (JSON) or curl wttr.in/<city> (text).
Or use web_read on weather websites. For JSON: curl 'wttr.in/London?format=j1' gives detailed forecasts."#.into(),
        },
        SkillDefinition {
            id: "blogwatcher".into(),
            name: "Blog Watcher".into(),
            description: "Monitor blogs and RSS/Atom feeds for updates".into(),
            icon: "📰".into(),
            category: SkillCategory::Cli,
            required_credentials: vec![],
            tool_names: vec![],
            required_binaries: vec![],
            required_env_vars: vec![], install_hint: String::new(),
            agent_instructions: r#"You can monitor RSS/Atom feeds for updates.
Use fetch to GET any RSS/Atom feed URL. Parse the XML to extract titles, links, dates, and summaries.
Common feed URLs end in /feed, /rss, /atom.xml. You can also use web_read to scrape blog homepages."#.into(),
        },
        SkillDefinition {
            id: "one_password".into(),
            name: "1Password".into(),
            description: "Access 1Password vaults via CLI".into(),
            icon: "🔐".into(),
            category: SkillCategory::System,
            required_credentials: vec![],
            tool_names: vec![],
            required_binaries: vec!["op".into()],
            required_env_vars: vec![], install_hint: "brew install 1password-cli".into(),
            agent_instructions: r#"You can access 1Password via the `op` CLI (must be signed in).
Commands: op item list, op item get <name_or_id> --fields label=password,
op item create --category login --title <name> --url <url>,
op vault list, op document get <name>.
IMPORTANT: Always use --fields to fetch specific fields, never dump full items.
Enable desktop app integration for biometric unlock: Settings > Developer > CLI."#.into(),
        },
        SkillDefinition {
            id: "spotify".into(),
            name: "Spotify".into(),
            description: "Control Spotify playback and search music".into(),
            icon: "🎵".into(),
            category: SkillCategory::Media,
            required_credentials: vec![],
            tool_names: vec![],
            required_binaries: vec!["spotify_player".into()],
            required_env_vars: vec![], install_hint: "brew install spotify_player".into(),
            agent_instructions: r#"You can control Spotify via `spotify_player` or `spogo` CLI.
Commands: spotify_player play <uri>, spotify_player pause, spotify_player next, spotify_player prev,
spotify_player search <query>, spotify_player devices, spotify_player volume <0-100>,
spotify_player queue <uri>, spotify_player status.
First run requires Spotify OAuth login."#.into(),
        },
        SkillDefinition {
            id: "google_workspace".into(),
            name: "Google Workspace".into(),
            description: "Gmail, Calendar, Drive, Contacts, Sheets, and Docs via CLI".into(),
            icon: "🎮".into(),
            category: SkillCategory::Api,
            required_credentials: vec![],
            tool_names: vec![],
            required_binaries: vec!["gog".into()],
            required_env_vars: vec![], install_hint: "brew install gog".into(),
            agent_instructions: r#"You can interact with Google Workspace via the `gog` CLI.
Services: gmail, calendar, drive, contacts, sheets, docs.
Commands: gog gmail list, gog gmail read <id>, gog gmail send <to> <subject> <body>,
gog calendar list, gog calendar add <title> <start> <end>,
gog drive list, gog drive upload <file>, gog drive download <id>.
First run requires Google OAuth consent."#.into(),
        },
        SkillDefinition {
            id: "google_places".into(),
            name: "Google Places".into(),
            description: "Search places, get details, reviews via Google Places API".into(),
            icon: "📍".into(),
            category: SkillCategory::Api,
            required_credentials: vec![
                CredentialField { key: "GOOGLE_PLACES_API_KEY".into(), label: "API Key".into(), description: "Google Places API (New) key".into(), required: true, placeholder: "AIza...".into() },
            ],
            tool_names: vec![],
            required_binaries: vec!["goplaces".into()],
            required_env_vars: vec![], install_hint: "brew install goplaces".into(),
            agent_instructions: r#"You can query Google Places using the `goplaces` CLI.
Commands: goplaces search <query> [--location <lat,lng>] [--radius <meters>],
goplaces details <place_id>, goplaces reviews <place_id>, goplaces resolve <name>.
Or use the Places API directly via fetch with your API key."#.into(),
        },
        SkillDefinition {
            id: "peekaboo".into(),
            name: "Peekaboo".into(),
            description: "Capture and automate macOS UI via accessibility".into(),
            icon: "👀".into(),
            category: SkillCategory::System,
            required_credentials: vec![],
            tool_names: vec![],
            required_binaries: vec!["peekaboo".into()],
            required_env_vars: vec![], install_hint: "brew install peekaboo".into(),
            agent_instructions: r#"You can capture and interact with the macOS UI via `peekaboo`.
Commands: peekaboo screenshot [--window <app>] [--screen], peekaboo list-windows,
peekaboo click <x> <y>, peekaboo type <text>, peekaboo read [--window <app>].
Requires Accessibility permission in System Preferences > Privacy."#.into(),
        },
        SkillDefinition {
            id: "healthcheck".into(),
            name: "Security Audit".into(),
            description: "Host security hardening and system health checks".into(),
            icon: "🛡️".into(),
            category: SkillCategory::System,
            required_credentials: vec![],
            tool_names: vec![],
            required_binaries: vec![],
            required_env_vars: vec![], install_hint: String::new(),
            agent_instructions: r#"You can perform security audits and health checks on the host system.
Use exec to run these checks:
- System info: uname -a, sw_vers (macOS), hostnamectl (Linux)
- Open ports: lsof -i -P -n | grep LISTEN, netstat -tlnp  
- Firewall: sudo pfctl -sr (macOS), sudo ufw status (Linux)
- SSH config: cat /etc/ssh/sshd_config, ssh-keygen -l -f ~/.ssh/authorized_keys
- Disk encryption: fdesetup status (macOS), blkid (Linux)
- Updates: softwareupdate -l (macOS), apt list --upgradable (Linux)
- Users: dscl . -list /Users (macOS), cat /etc/passwd (Linux)
Always ask before making changes. Report findings clearly."#.into(),
        },
        SkillDefinition {
            id: "summarize".into(),
            name: "Summarize".into(),
            description: "Summarize URLs, podcasts, and video transcripts".into(),
            icon: "🧾".into(),
            category: SkillCategory::Cli,
            required_credentials: vec![],
            tool_names: vec![],
            required_binaries: vec!["summarize".into()],
            required_env_vars: vec![], install_hint: "brew install summarize".into(),
            agent_instructions: r#"You can transcribe and summarize content using the `summarize` CLI.
Commands: summarize <url> — works with YouTube videos, podcasts, articles, PDFs.
summarize <file> — local audio/video files.
Options: --format text|json|markdown, --length short|medium|long.
Falls back to web_read for articles if summarize isn't available."#.into(),
        },
        SkillDefinition {
            id: "gifgrep".into(),
            name: "GIF Search".into(),
            description: "Search and download GIFs from multiple providers".into(),
            icon: "🧲".into(),
            category: SkillCategory::Media,
            required_credentials: vec![],
            tool_names: vec![],
            required_binaries: vec!["gifgrep".into()],
            required_env_vars: vec![], install_hint: "brew install gifgrep".into(),
            agent_instructions: r#"You can search for GIFs using `gifgrep`.
Commands: gifgrep <query> [--provider giphy|tenor] [--limit 5] [--download <dir>],
gifgrep --extract-stills <gif> — extract frames as PNGs."#.into(),
        },
        SkillDefinition {
            id: "camsnap".into(),
            name: "Camera Capture".into(),
            description: "Capture frames from RTSP/ONVIF cameras".into(),
            icon: "📸".into(),
            category: SkillCategory::SmartHome,
            required_credentials: vec![],
            tool_names: vec![],
            required_binaries: vec!["camsnap".into()],
            required_env_vars: vec![], install_hint: "brew install camsnap".into(),
            agent_instructions: r#"You can capture snapshots from IP cameras via `camsnap`.
Commands: camsnap snap <url> [--output frame.jpg], camsnap discover (find cameras on network),
camsnap stream <url> --frames 10 --interval 1s (capture multiple).
Supports RTSP, ONVIF, and HTTP MJPEG streams."#.into(),
        },
        // ── DEX / Uniswap Trading ──
        SkillDefinition {
            id: "dex".into(),
            name: "DEX Trading (Uniswap)".into(),
            description: "Self-custody Ethereum wallet with on-chain swaps via Uniswap V3. Private key never leaves the vault.".into(),
            icon: "🦄".into(),
            category: SkillCategory::Vault,
            required_credentials: vec![
                CredentialField { key: "DEX_RPC_URL".into(), label: "Ethereum RPC URL".into(), description: "JSON-RPC endpoint for Ethereum (from Infura, Alchemy, or your own node). Example: https://mainnet.infura.io/v3/YOUR_KEY".into(), required: false, placeholder: "https://mainnet.infura.io/v3/abc123...".into() },
                CredentialField { key: "DEX_PRIVATE_KEY".into(), label: "Wallet Private Key".into(), description: "Auto-generated when you use dex_wallet_create. Or paste your own 0x-prefixed hex key. Stored encrypted in OS keychain vault.".into(), required: false, placeholder: "Auto-generated — leave blank".into() },
                CredentialField { key: "DEX_WALLET_ADDRESS".into(), label: "Wallet Address".into(), description: "Auto-populated when wallet is created. Or paste your own Ethereum address if importing a key.".into(), required: false, placeholder: "Auto-generated — leave blank".into() },
            ],
            tool_names: vec!["dex_wallet_create".into(), "dex_balance".into(), "dex_quote".into(), "dex_swap".into(), "dex_transfer".into(), "dex_portfolio".into(), "dex_token_info".into(), "dex_check_token".into(), "dex_search_token".into(), "dex_watch_wallet".into(), "dex_whale_transfers".into(), "dex_top_traders".into(), "dex_trending".into()],
            required_binaries: vec![], required_env_vars: vec![], install_hint: "Get an RPC URL at infura.io or alchemy.com (free tier works)".into(),
            agent_instructions: r#"You have a self-custody Ethereum wallet for DEX trading via Uniswap V3.

CRITICAL: Your private key is stored encrypted in the OS keychain vault. You NEVER see it — the engine signs transactions internally. Do NOT:
- Try to read or access the private key
- Try to export or display wallet credentials
- Run shell commands to interact with the blockchain (use the tools below)

Available tools:
- **dex_wallet_create**: Generate a new Ethereum wallet. Private key is encrypted and stored automatically.
- **dex_balance**: Check ETH and ERC-20 token balances for a specific token.
- **dex_quote**: Get a swap quote from Uniswap V3 (read-only, no transaction).
- **dex_swap**: Execute a token swap on Uniswap V3. ALWAYS requires user approval.
- **dex_transfer**: Send ETH or ERC-20 tokens from your wallet to any external Ethereum address. ALWAYS requires user approval.
- **dex_portfolio**: Check all token balances at once.
- **dex_token_info**: Get comprehensive on-chain info about any ERC-20 token (name, symbol, supply, owner, swap viability). Queries the blockchain directly — no website needed.
- **dex_check_token**: Run automated safety checks (honeypot detection, tax analysis, ownership audit, risk scoring 0-30). ALWAYS run this before trading any new/unknown token.
- **dex_search_token**: Search for tokens by name or symbol to find contract addresses, prices, volume, and liquidity. Uses DexScreener API (not web scraping). Use this to discover contract addresses before running dex_check_token.
- **dex_watch_wallet**: Monitor any wallet address — shows ETH balance, token holdings, and recent ERC-20 transfers. Use this to track smart money wallets and alpha traders.
- **dex_whale_transfers**: Scan recent large transfers of a token. Identifies top accumulators (whales buying) and distributors (insiders selling). Essential for spotting whale moves early.
- **dex_top_traders**: Analyze a token's Transfer events to discover the most profitable wallets — smart traders, rotators, early movers. Classifies each wallet (Accumulator, Profit Taker, Rotator) with PnL and trade stats. Use this to find alpha wallets to monitor.
- **dex_trending**: Get trending and recently boosted tokens from DexScreener. Shows what's gaining attention right now. Filter by chain.

Supported tokens: ETH, WETH, USDC, USDT, DAI, WBTC, UNI, LINK, PEPE, SHIB, ARB, AAVE (or any ERC-20 by contract address).

Trading Rules:
- ALWAYS get a quote (dex_quote) before proposing a swap
- ALWAYS run dex_check_token on any new/unfamiliar token before trading to detect honeypots and scams
- ALWAYS state your reasoning and expected outcome before swapping
- Check balances before trading
- Default slippage is 0.5% — warn the user if you need higher
- For ETH→token swaps, the wallet must have enough ETH for gas + swap amount
- For token→token swaps, the wallet must have ETH for gas fees
- If the user hasn't set risk parameters, ask before executing large trades

Alpha Hunting Workflow:
1. Use dex_trending to find what's hot right now (boosted tokens, new listings)
2. Use dex_search_token to discover tokens by name/symbol and get price/volume/liquidity
3. Use dex_check_token to run safety checks (honeypot, tax, ownership) — MANDATORY before trading
4. Use dex_top_traders to find the most profitable wallets trading a token (smart money, rotators, early buyers)
5. Use dex_whale_transfers to see large transfers and accumulation patterns
6. Use dex_watch_wallet on high-conviction wallets to see their full portfolio and recent moves
7. Cross-reference: when multiple smart wallets accumulate the same token, that's a strong signal
8. Execute: dex_quote first, then dex_swap with user approval"#.into(),
        },
        // ── Solana DEX Trading (Jupiter + PumpPortal) ──
        SkillDefinition {
            id: "solana_dex".into(),
            name: "Solana DEX Trading (Jupiter + PumpPortal)".into(),
            description: "Self-custody Solana wallet with on-chain swaps via Jupiter aggregator + PumpPortal fallback for pump.fun tokens. Private key never leaves the vault.".into(),
            icon: "☀️".into(),
            category: SkillCategory::Vault,
            required_credentials: vec![
                CredentialField { key: "JUPITER_API_KEY".into(), label: "Jupiter API Key".into(), description: "For Jupiter swap quotes. Get a free key at https://dev.jup.ag — Dashboard → API Keys. Optional: PumpPortal works without it.".into(), required: false, placeholder: "your-jupiter-api-key".into() },
                CredentialField { key: "SOLANA_RPC_URL".into(), label: "Solana RPC URL".into(), description: "JSON-RPC endpoint for Solana mainnet (from Alchemy, Helius, QuickNode, or your own node). Example: https://solana-mainnet.g.alchemy.com/v2/YOUR_KEY".into(), required: false, placeholder: "https://solana-mainnet.g.alchemy.com/v2/abc123...".into() },
                CredentialField { key: "SOLANA_PRIVATE_KEY".into(), label: "Wallet Private Key".into(), description: "Auto-generated when you use sol_wallet_create. Or paste your own base58-encoded keypair. Stored encrypted in OS keychain vault.".into(), required: false, placeholder: "Auto-generated — leave blank".into() },
                CredentialField { key: "SOLANA_WALLET_ADDRESS".into(), label: "Wallet Address".into(), description: "Auto-populated when wallet is created. Or paste your own Solana address if importing a key.".into(), required: false, placeholder: "Auto-generated — leave blank".into() },
            ],
            tool_names: vec!["sol_wallet_create".into(), "sol_balance".into(), "sol_quote".into(), "sol_swap".into(), "sol_transfer".into(), "sol_portfolio".into(), "sol_token_info".into()],
            required_binaries: vec![], required_env_vars: vec![], install_hint: "Get a Solana RPC URL at alchemy.com, helius.dev, or quicknode.com (free tier works). Jupiter API key optional (free at dev.jup.ag). PumpPortal needs no API key.".into(),
            agent_instructions: r#"You have a self-custody Solana wallet for DEX trading.

ROUTING: Swaps try Jupiter aggregator first (best prices across all DEXes). If Jupiter has no route (common for pump.fun memecoins), it automatically falls back to PumpPortal which routes through pump.fun bonding curve, PumpSwap AMM, Raydium, and more. This means you CAN buy AND sell pump.fun tokens.

CRITICAL: Your private key is stored encrypted in the OS keychain vault. You NEVER see it — the engine signs transactions internally. Do NOT:
- Try to read or access the private key
- Try to export or display wallet credentials
- Run shell commands to interact with the blockchain (use the tools below)

Available tools:
- **sol_wallet_create**: Generate a new Solana wallet (ed25519). Private key is encrypted and stored automatically.
- **sol_balance**: Check SOL and SPL token balances.
- **sol_quote**: Get a swap quote (tries Jupiter, falls back to PumpPortal for pump.fun tokens).
- **sol_swap**: Execute a token swap. Tries Jupiter first, falls back to PumpPortal if no route. REQUIRES USER APPROVAL.
- **sol_transfer**: Send SOL or SPL tokens from your wallet to any external Solana address. Creates recipient token account if needed. REQUIRES USER APPROVAL.
- **sol_portfolio**: Check all token balances at once.
- **sol_token_info**: Get on-chain info about any SPL token (decimals, supply, authorities).

Supported tokens: SOL, USDC, USDT, BONK, JUP, RAY, PYTH, WIF, ORCA, mSOL, jitoSOL (or any SPL token by mint address).

Trading Rules:
- ALWAYS get a quote (sol_quote) before proposing a swap
- ALWAYS state your reasoning and expected outcome before swapping
- Check balances before trading (sol_balance)
- Default slippage is 0.5% — for pump.fun memecoins, suggest 5-15% slippage
- For SOL→token swaps, the wallet needs enough SOL for fees + swap amount
- For token→token swaps, the wallet needs SOL for transaction fees (~0.005 SOL)
- If the user hasn't set risk parameters, ask before executing large trades
- Use dex_search_token (from the Ethereum DEX skill) to discover new Solana tokens — DexScreener supports all chains
- Use dex_trending with chain='solana' to find trending Solana tokens
- If a sell fails on Jupiter, try again with higher slippage — PumpPortal fallback will kick in automatically"#.into(),
        },
    ]
}

// ── Credential Storage (in SessionStore) ───────────────────────────────

use crate::engine::sessions::SessionStore;

impl SessionStore {
    /// Initialize the skill vault tables (call from open()).
    pub fn init_skill_tables(&self) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS skill_credentials (
                skill_id TEXT NOT NULL,
                cred_key TEXT NOT NULL,
                cred_value TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (skill_id, cred_key)
            );

            CREATE TABLE IF NOT EXISTS skill_state (
                skill_id TEXT PRIMARY KEY,
                enabled INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS skill_custom_instructions (
                skill_id TEXT PRIMARY KEY,
                instructions TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
        ").map_err(|e| format!("Failed to create skill tables: {}", e))?;
        Ok(())
    }

    /// Store a credential for a skill.
    /// Value is stored encrypted (caller must encrypt before calling).
    pub fn set_skill_credential(&self, skill_id: &str, key: &str, encrypted_value: &str) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO skill_credentials (skill_id, cred_key, cred_value, updated_at)
             VALUES (?1, ?2, ?3, datetime('now'))
             ON CONFLICT(skill_id, cred_key) DO UPDATE SET cred_value = ?3, updated_at = datetime('now')",
            rusqlite::params![skill_id, key, encrypted_value],
        ).map_err(|e| format!("Set credential error: {}", e))?;
        Ok(())
    }

    /// Get a credential for a skill (returns encrypted value).
    pub fn get_skill_credential(&self, skill_id: &str, key: &str) -> Result<Option<String>, String> {
        let conn = self.conn.lock();
        let result = conn.query_row(
            "SELECT cred_value FROM skill_credentials WHERE skill_id = ?1 AND cred_key = ?2",
            rusqlite::params![skill_id, key],
            |row: &rusqlite::Row| row.get::<_, String>(0),
        );
        match result {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("Query error: {}", e)),
        }
    }

    /// Delete a credential for a skill.
    pub fn delete_skill_credential(&self, skill_id: &str, key: &str) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM skill_credentials WHERE skill_id = ?1 AND cred_key = ?2",
            rusqlite::params![skill_id, key],
        ).map_err(|e| format!("Delete credential error: {}", e))?;
        Ok(())
    }

    /// Delete ALL credentials for a skill.
    pub fn delete_all_skill_credentials(&self, skill_id: &str) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM skill_credentials WHERE skill_id = ?1",
            rusqlite::params![skill_id],
        ).map_err(|e| format!("Delete credentials error: {}", e))?;
        Ok(())
    }

    /// List which credential keys are set for a skill (not the values).
    pub fn list_skill_credential_keys(&self, skill_id: &str) -> Result<Vec<String>, String> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT cred_key FROM skill_credentials WHERE skill_id = ?1 ORDER BY cred_key"
        ).map_err(|e| format!("Prepare error: {}", e))?;
        let keys: Vec<String> = stmt.query_map(rusqlite::params![skill_id], |row: &rusqlite::Row| row.get::<_, String>(0))
            .map_err(|e| format!("Query error: {}", e))?
            .filter_map(|r: Result<String, rusqlite::Error>| r.ok())
            .collect();
        Ok(keys)
    }

    /// Get/set skill enabled state.
    pub fn set_skill_enabled(&self, skill_id: &str, enabled: bool) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO skill_state (skill_id, enabled, updated_at) VALUES (?1, ?2, datetime('now'))
             ON CONFLICT(skill_id) DO UPDATE SET enabled = ?2, updated_at = datetime('now')",
            rusqlite::params![skill_id, enabled as i32],
        ).map_err(|e| format!("Set skill state error: {}", e))?;
        Ok(())
    }

    pub fn is_skill_enabled(&self, skill_id: &str) -> Result<bool, String> {
        let conn = self.conn.lock();
        let result = conn.query_row(
            "SELECT enabled FROM skill_state WHERE skill_id = ?1",
            rusqlite::params![skill_id],
            |row: &rusqlite::Row| row.get::<_, i32>(0),
        );
        match result {
            Ok(v) => Ok(v != 0),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
            Err(e) => Err(format!("Query error: {}", e)),
        }
    }

    /// Get custom instructions for a skill (if any).
    pub fn get_skill_custom_instructions(&self, skill_id: &str) -> Result<Option<String>, String> {
        let conn = self.conn.lock();
        let result = conn.query_row(
            "SELECT instructions FROM skill_custom_instructions WHERE skill_id = ?1",
            rusqlite::params![skill_id],
            |row: &rusqlite::Row| row.get::<_, String>(0),
        );
        match result {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("Query error: {}", e)),
        }
    }

    /// Set custom instructions for a skill.
    /// Pass empty string to clear (falls back to defaults).
    pub fn set_skill_custom_instructions(&self, skill_id: &str, instructions: &str) -> Result<(), String> {
        let conn = self.conn.lock();
        if instructions.is_empty() {
            conn.execute(
                "DELETE FROM skill_custom_instructions WHERE skill_id = ?1",
                rusqlite::params![skill_id],
            ).map_err(|e| format!("Delete error: {}", e))?;
        } else {
            conn.execute(
                "INSERT INTO skill_custom_instructions (skill_id, instructions, updated_at)
                 VALUES (?1, ?2, datetime('now'))
                 ON CONFLICT(skill_id) DO UPDATE SET instructions = ?2, updated_at = datetime('now')",
                rusqlite::params![skill_id, instructions],
            ).map_err(|e| format!("Set instructions error: {}", e))?;
        }
        Ok(())
    }
}

// ── Encryption helpers ─────────────────────────────────────────────────
// We use a simple XOR-with-key approach using a random key stored in the OS keychain.
// This isn't military-grade crypto but it keeps credentials from being readable
// if someone directly opens the SQLite file. The real security is the OS keychain.

const VAULT_KEYRING_SERVICE: &str = "paw-skill-vault";
const VAULT_KEYRING_USER: &str = "encryption-key";

/// Get or create the vault encryption key from the OS keychain.
pub fn get_vault_key() -> Result<Vec<u8>, String> {
    let entry = keyring::Entry::new(VAULT_KEYRING_SERVICE, VAULT_KEYRING_USER)
        .map_err(|e| format!("Keyring init failed: {}", e))?;

    match entry.get_password() {
        Ok(key_b64) => {
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &key_b64)
                .map_err(|e| format!("Failed to decode vault key: {}", e))
        }
        Err(keyring::Error::NoEntry) => {
            // Generate a new random key
            use rand::Rng;
            let mut key = vec![0u8; 32];
            rand::thread_rng().fill(&mut key[..]);
            let key_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &key);
            entry.set_password(&key_b64)
                .map_err(|e| format!("Failed to store vault key in keychain: {}", e))?;
            info!("[vault] Created new vault encryption key in OS keychain");
            Ok(key)
        }
        Err(e) => Err(format!("Keyring error: {}", e)),
    }
}

/// Encrypt a plaintext credential value.
pub fn encrypt_credential(plaintext: &str, key: &[u8]) -> String {
    let bytes = plaintext.as_bytes();
    let encrypted: Vec<u8> = bytes.iter().enumerate().map(|(i, b)| b ^ key[i % key.len()]).collect();
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &encrypted)
}

/// Decrypt an encrypted credential value.
pub fn decrypt_credential(encrypted_b64: &str, key: &[u8]) -> Result<String, String> {
    let encrypted = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encrypted_b64)
        .map_err(|e| format!("Failed to decode: {}", e))?;
    let decrypted: Vec<u8> = encrypted.iter().enumerate().map(|(i, b)| b ^ key[i % key.len()]).collect();
    String::from_utf8(decrypted).map_err(|e| format!("Failed to decrypt: {}", e))
}

// ── Skill Status Helpers ───────────────────────────────────────────────

/// Get the combined status of all skills (definition + stored state).
pub fn get_all_skill_status(store: &SessionStore) -> Result<Vec<SkillStatus>, String> {
    let definitions = builtin_skills();
    let mut statuses = Vec::new();

    for def in &definitions {
        let enabled = store.is_skill_enabled(&def.id)?;
        let configured_keys = store.list_skill_credential_keys(&def.id)?;
        let missing_creds: Vec<String> = def.required_credentials.iter()
            .filter(|c| c.required && !configured_keys.contains(&c.key))
            .map(|c| c.key.clone())
            .collect();

        // Check which required binaries are missing from PATH
        let missing_bins: Vec<String> = def.required_binaries.iter()
            .filter(|bin| {
                std::process::Command::new("which")
                    .arg(bin)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .map(|s| !s.success())
                    .unwrap_or(true)
            })
            .cloned()
            .collect();

        // Check which required env vars are missing
        let missing_envs: Vec<String> = def.required_env_vars.iter()
            .filter(|v| std::env::var(v).is_err())
            .cloned()
            .collect();

        let is_ready = enabled && missing_creds.is_empty() && missing_bins.is_empty() && missing_envs.is_empty();

        let custom_instr = store.get_skill_custom_instructions(&def.id)?.unwrap_or_default();

        statuses.push(SkillStatus {
            id: def.id.clone(),
            name: def.name.clone(),
            description: def.description.clone(),
            icon: def.icon.clone(),
            category: def.category.clone(),
            enabled,
            required_credentials: def.required_credentials.clone(),
            configured_credentials: configured_keys,
            missing_credentials: missing_creds,
            required_binaries: def.required_binaries.clone(),
            missing_binaries: missing_bins,
            required_env_vars: def.required_env_vars.clone(),
            missing_env_vars: missing_envs,
            install_hint: def.install_hint.clone(),
            has_instructions: !def.agent_instructions.is_empty() || !custom_instr.is_empty(),
            is_ready,
            tool_names: def.tool_names.clone(),
            default_instructions: def.agent_instructions.clone(),
            custom_instructions: custom_instr,
        });
    }

    Ok(statuses)
}

/// Get credential values for a skill (decrypted). Used by tool executor at runtime.
pub fn get_skill_credentials(store: &SessionStore, skill_id: &str) -> Result<std::collections::HashMap<String, String>, String> {
    let vault_key = get_vault_key()?;
    let keys = store.list_skill_credential_keys(skill_id)?;
    let mut creds = std::collections::HashMap::new();

    for key in keys {
        if let Some(encrypted) = store.get_skill_credential(skill_id, &key)? {
            match decrypt_credential(&encrypted, &vault_key) {
                Ok(value) => { creds.insert(key, value); }
                Err(e) => {
                    warn!("[vault] Failed to decrypt {}:{}: {}", skill_id, key, e);
                }
            }
        }
    }

    Ok(creds)
}
/// Collect agent instructions from all enabled skills.
/// Returns a combined string to be injected into the system prompt.
/// - Prefers custom instructions over defaults (if user edited them).
/// - For skills with credentials, injects actual decrypted values into placeholders.
/// - `agent_id` filters community skills to only those assigned to this agent.
pub fn get_enabled_skill_instructions(store: &SessionStore, agent_id: &str) -> Result<String, String> {
    let definitions = builtin_skills();
    let mut sections: Vec<String> = Vec::new();

    for def in &definitions {
        if !store.is_skill_enabled(&def.id)? { continue; }

        // Use custom instructions if set, otherwise fall back to defaults
        let base_instructions = store.get_skill_custom_instructions(&def.id)?
            .unwrap_or_else(|| def.agent_instructions.clone());

        if base_instructions.is_empty() { continue; }

        // For skills with credentials, inject actual values into the instructions
        // UNLESS the skill has built-in tool_executor auth (credentials stay server-side)
        let hidden_credential_skills = ["coinbase", "dex"];
        let instructions = if !def.required_credentials.is_empty() && !hidden_credential_skills.contains(&def.id.as_str()) {
            inject_credentials_into_instructions(store, &def.id, &def.required_credentials, &base_instructions)
        } else {
            base_instructions
        };

        sections.push(format!(
            "## {} Skill ({})\n{}",
            def.name, def.id, instructions
        ));
    }

    // Also include enabled community skills scoped to this agent
    let community_instructions = get_community_skill_instructions(store, agent_id).unwrap_or_default();

    let mut result = String::new();

    if !sections.is_empty() {
        result.push_str(&format!(
            "\n\n# Enabled Skills\nYou have the following skills available. Use exec, fetch, read_file, write_file, and other built-in tools to leverage them.\n\n{}\n",
            sections.join("\n\n")
        ));
    }

    if !community_instructions.is_empty() {
        result.push_str(&community_instructions);
    }

    // Guard: cap total skill instructions to ~8000 tokens (~32K chars).
    // Beyond this, skills eat too much context window and degrade conversation quality.
    const MAX_SKILL_CHARS: usize = 32_000;
    if result.len() > MAX_SKILL_CHARS {
        log::warn!(
            "[skills] Skill instructions too large ({} chars, ~{} tokens). Truncating to {} chars. \
            Consider reducing the number of enabled skills for this agent.",
            result.len(), result.len() / 4, MAX_SKILL_CHARS
        );
        // Truncate at a line boundary to avoid breaking mid-instruction
        let truncated = &result[..MAX_SKILL_CHARS];
        let last_newline = truncated.rfind('\n').unwrap_or(MAX_SKILL_CHARS);
        result = result[..last_newline].to_string();
        result.push_str("\n\n⚠️ Some skill instructions were truncated because too many skills are enabled. Consider disabling unused skills.");
    }

    Ok(result)
}

/// Inject decrypted credential values into instruction text.
/// Adds a "Credentials available:" block at the end of the instructions
/// so the agent knows the actual API keys/tokens to use.
fn inject_credentials_into_instructions(
    store: &SessionStore,
    skill_id: &str,
    required_credentials: &[CredentialField],
    instructions: &str,
) -> String {
    match get_skill_credentials(store, skill_id) {
        Ok(creds) if !creds.is_empty() => {
            let cred_lines: Vec<String> = required_credentials.iter()
                .filter_map(|field| {
                    creds.get(&field.key).map(|val| {
                        format!("- {} = {}", field.key, val)
                    })
                })
                .collect();

            if cred_lines.is_empty() {
                return instructions.to_string();
            }

            format!(
                "{}\n\nCredentials (use these values directly — do NOT ask the user for them):\n{}",
                instructions,
                cred_lines.join("\n")
            )
        }
        _ => instructions.to_string(),
    }
}

// ════════════════════════════════════════════════════════════════════════
// ██  Community Skills (skills.sh / SKILL.md ecosystem)  ████████████████
// ════════════════════════════════════════════════════════════════════════
//
// skills.sh is an open agent skills ecosystem by Vercel. Skills are
// SKILL.md files with YAML frontmatter (name, description) and a
// markdown body containing instructions for the agent.
//
// Pawz integrates by:
//   1. Fetching SKILL.md files from GitHub repos
//   2. Parsing frontmatter + body as instructions
//   3. Storing them in SQLite alongside builtin skills
//   4. Injecting their instructions into the agent's system prompt
//
// Community skills are instruction-only — no tool functions or
// credentials. They teach the agent procedures and best practices.

/// A community skill installed from skills.sh / GitHub.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunitySkill {
    /// Unique ID: "owner/repo/skill-name"
    pub id: String,
    /// Human name from SKILL.md frontmatter
    pub name: String,
    /// Description from SKILL.md frontmatter
    pub description: String,
    /// Full markdown instructions (the SKILL.md body after frontmatter)
    pub instructions: String,
    /// Source: "owner/repo" or full GitHub URL
    pub source: String,
    /// Whether this skill is enabled (injected into agent prompts)
    pub enabled: bool,
    /// JSON array of agent IDs this skill applies to. Empty array [] = all agents.
    pub agent_ids: Vec<String>,
    /// When it was installed
    pub installed_at: String,
    /// When it was last updated
    pub updated_at: String,
}

/// A skill discovered from a GitHub repo (not yet installed).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredSkill {
    /// Derived ID: "owner/repo/skill-name"
    pub id: String,
    /// Human name from SKILL.md frontmatter
    pub name: String,
    /// Description from SKILL.md frontmatter
    pub description: String,
    /// Source repo: "owner/repo"
    pub source: String,
    /// Path within the repo (e.g. "skills/my-skill/SKILL.md")
    pub path: String,
    /// Whether this skill is already installed locally
    pub installed: bool,
    /// Install count from skills.sh (0 if unknown)
    pub installs: u64,
}

// ── SKILL.md Parser ────────────────────────────────────────────────────

/// Parse a SKILL.md file into (name, description, instructions).
/// Format: YAML frontmatter between --- delimiters, then markdown body.
pub fn parse_skill_md(content: &str) -> Option<(String, String, String)> {
    let trimmed = content.trim();
    if !trimmed.starts_with("---") {
        return None;
    }

    // Find the closing --- delimiter
    let after_first = &trimmed[3..];
    let end_idx = after_first.find("\n---")?;
    let frontmatter = &after_first[..end_idx];
    let body = after_first[end_idx + 4..].trim();

    // Parse YAML frontmatter (just name + description — no serde_yaml needed)
    let mut name = String::new();
    let mut description = String::new();

    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("name:") {
            name = val.trim().trim_matches('"').trim_matches('\'').to_string();
        } else if let Some(val) = line.strip_prefix("description:") {
            description = val.trim().trim_matches('"').trim_matches('\'').to_string();
        }
    }

    if name.is_empty() {
        return None;
    }

    Some((name, description, body.to_string()))
}

// ── GitHub Fetcher ─────────────────────────────────────────────────────

/// Fetch the list of skills available in a GitHub repo.
/// Uses the GitHub API to list files in skills/ directories.
pub async fn fetch_repo_skills(source: &str) -> Result<Vec<DiscoveredSkill>, String> {
    let (owner, repo) = parse_github_source(source)?;
    let client = reqwest::Client::new();

    // Try the GitHub API to get the repo tree
    let tree_url = format!(
        "https://api.github.com/repos/{}/{}/git/trees/main?recursive=1",
        owner, repo
    );

    let resp = client.get(&tree_url)
        .header("User-Agent", "Pawz/1.0")
        .header("Accept", "application/vnd.github.v3+json")
        .send().await
        .map_err(|e| format!("GitHub API error: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("GitHub API returned {}", resp.status()));
    }

    let tree: serde_json::Value = resp.json().await
        .map_err(|e| format!("Failed to parse GitHub response: {}", e))?;

    // Find all SKILL.md files
    let skill_paths: Vec<String> = tree["tree"].as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|entry| {
            let path = entry["path"].as_str()?;
            if path.ends_with("/SKILL.md") || path == "SKILL.md" {
                Some(path.to_string())
            } else {
                None
            }
        })
        .collect();

    if skill_paths.is_empty() {
        return Err(format!("No SKILL.md files found in {}/{}", owner, repo));
    }

    // Fetch each SKILL.md and parse it
    let mut skills = Vec::new();
    for path in &skill_paths {
        let raw_url = format!(
            "https://raw.githubusercontent.com/{}/{}/main/{}",
            owner, repo, path
        );

        let content = match client.get(&raw_url)
            .header("User-Agent", "Pawz/1.0")
            .send().await
        {
            Ok(r) if r.status().is_success() => {
                r.text().await.unwrap_or_default()
            }
            _ => continue,
        };

        if let Some((name, description, _instructions)) = parse_skill_md(&content) {
            let skill_name = name.to_lowercase().replace(' ', "-");
            let id = format!("{}/{}/{}", owner, repo, skill_name);
            skills.push(DiscoveredSkill {
                id,
                name,
                description,
                source: format!("{}/{}", owner, repo),
                path: path.clone(),
                installed: false,
                installs: 0,
            });
        }
    }

    Ok(skills)
}

/// Install a single community skill from a GitHub repo.
pub async fn install_community_skill(
    store: &SessionStore,
    source: &str,
    skill_path: &str,
    agent_id: Option<&str>,
) -> Result<CommunitySkill, String> {
    let (owner, repo) = parse_github_source(source)?;
    let client = reqwest::Client::new();

    let raw_url = format!(
        "https://raw.githubusercontent.com/{}/{}/main/{}",
        owner, repo, skill_path
    );

    let resp = client.get(&raw_url)
        .header("User-Agent", "Pawz/1.0")
        .send().await
        .map_err(|e| format!("Failed to fetch skill: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Failed to download SKILL.md: HTTP {}", resp.status()));
    }

    let content = resp.text().await
        .map_err(|e| format!("Failed to read SKILL.md: {}", e))?;

    let (name, description, instructions) = parse_skill_md(&content)
        .ok_or("Invalid SKILL.md format — missing name in frontmatter")?;

    let skill_name = name.to_lowercase().replace(' ', "-");
    let id = format!("{}/{}/{}", owner, repo, skill_name);
    let now = chrono::Utc::now().to_rfc3339();

    // If installed by a specific agent, scope the skill to that agent.
    // Empty vec = all agents (when installed from the UI with no agent context).
    let agent_ids = match agent_id {
        Some(aid) => vec![aid.to_string()],
        None => vec![],
    };

    let skill = CommunitySkill {
        id: id.clone(),
        name,
        description,
        instructions,
        source: format!("{}/{}", owner, repo),
        enabled: true,
        agent_ids,
        installed_at: now.clone(),
        updated_at: now,
    };

    store.save_community_skill(&skill)?;
    info!("[skills] Installed community skill: {} from {} (agents: {:?})", id, source, &skill.agent_ids);

    Ok(skill)
}

/// Search for skills via the skills.sh directory API.
/// Uses https://skills.sh/api/search?q={query} to find skills across the ecosystem.
pub async fn search_community_skills(query: &str) -> Result<Vec<DiscoveredSkill>, String> {
    let client = reqwest::Client::new();

    let encoded_query = query.replace(' ', "+");
    let search_url = format!(
        "https://skills.sh/api/search?q={}",
        encoded_query
    );

    let resp = client.get(&search_url)
        .header("User-Agent", "Pawz/1.0")
        .send().await
        .map_err(|e| format!("skills.sh search error: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("skills.sh returned HTTP {}: {}", status, body));
    }

    let data: serde_json::Value = resp.json().await
        .map_err(|e| format!("Failed to parse skills.sh response: {}", e))?;

    let empty_vec = vec![];
    let items = data["skills"].as_array()
        .unwrap_or(&empty_vec);

    let mut skills = Vec::new();

    for item in items {
        let skill_id = item["skillId"].as_str().unwrap_or_default();
        let name = item["name"].as_str().unwrap_or_default();
        let source = item["source"].as_str().unwrap_or_default();
        let full_id = item["id"].as_str().unwrap_or_default();
        let installs = item["installs"].as_u64().unwrap_or(0);

        if skill_id.is_empty() || source.is_empty() {
            continue;
        }

        // Construct the SKILL.md path: skills/{skillId}/SKILL.md
        let path = format!("skills/{}/SKILL.md", skill_id);

        skills.push(DiscoveredSkill {
            id: full_id.to_string(),
            name: name.to_string(),
            description: String::new(), // skills.sh doesn't return descriptions in search
            source: source.to_string(),
            path,
            installed: false,
            installs,
        });
    }

    Ok(skills)
}

/// Parse "owner/repo" from a GitHub source string.
fn parse_github_source(source: &str) -> Result<(String, String), String> {
    // Handle full URLs: https://github.com/owner/repo
    let cleaned = source
        .trim()
        .trim_end_matches('/')
        .replace("https://github.com/", "")
        .replace("http://github.com/", "");

    let parts: Vec<&str> = cleaned.split('/').collect();
    if parts.len() < 2 {
        return Err(format!("Invalid source '{}' — expected 'owner/repo' format", source));
    }

    Ok((parts[0].to_string(), parts[1].to_string()))
}

// ── DB Storage for Community Skills ────────────────────────────────────

impl SessionStore {
    /// Initialize the community skills table.
    pub fn init_community_skills_table(&self) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS community_skills (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                instructions TEXT NOT NULL,
                source TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                agent_ids TEXT NOT NULL DEFAULT '[]',
                installed_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
        ").map_err(|e| format!("Failed to create community_skills table: {}", e))?;

        // Migration: add agent_ids column if table existed without it
        let _ = conn.execute_batch(
            "ALTER TABLE community_skills ADD COLUMN agent_ids TEXT NOT NULL DEFAULT '[]'"
        );

        Ok(())
    }

    /// Save (upsert) a community skill.
    pub fn save_community_skill(&self, skill: &CommunitySkill) -> Result<(), String> {
        let conn = self.conn.lock();
        let agent_ids_json = serde_json::to_string(&skill.agent_ids).unwrap_or_else(|_| "[]".to_string());
        conn.execute(
            "INSERT INTO community_skills (id, name, description, instructions, source, enabled, agent_ids, installed_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET name=?2, description=?3, instructions=?4, source=?5, agent_ids=?7, updated_at=?9",
            rusqlite::params![
                skill.id, skill.name, skill.description, skill.instructions,
                skill.source, skill.enabled as i32, agent_ids_json, skill.installed_at, skill.updated_at
            ],
        ).map_err(|e| format!("Save community skill error: {}", e))?;
        Ok(())
    }

    /// List all installed community skills.
    pub fn list_community_skills(&self) -> Result<Vec<CommunitySkill>, String> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, description, instructions, source, enabled, agent_ids, installed_at, updated_at
             FROM community_skills ORDER BY name"
        ).map_err(|e| format!("Query error: {}", e))?;

        let skills = stmt.query_map([], |row| {
            let agent_ids_json: String = row.get::<_, String>(6).unwrap_or_else(|_| "[]".to_string());
            let agent_ids: Vec<String> = serde_json::from_str(&agent_ids_json).unwrap_or_default();
            Ok(CommunitySkill {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                instructions: row.get(3)?,
                source: row.get(4)?,
                enabled: row.get::<_, i32>(5)? != 0,
                agent_ids,
                installed_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        }).map_err(|e| format!("Query error: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

        Ok(skills)
    }

    /// Set enabled state for a community skill.
    pub fn set_community_skill_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE community_skills SET enabled = ?1, updated_at = datetime('now') WHERE id = ?2",
            rusqlite::params![enabled as i32, id],
        ).map_err(|e| format!("Update error: {}", e))?;
        Ok(())
    }

    /// Remove a community skill.
    pub fn remove_community_skill(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM community_skills WHERE id = ?1",
            rusqlite::params![id],
        ).map_err(|e| format!("Delete error: {}", e))?;
        Ok(())
    }

    /// Update the agent_ids for a community skill.
    /// Empty vec = all agents. Non-empty = only those specific agents.
    pub fn set_community_skill_agents(&self, id: &str, agent_ids: &[String]) -> Result<(), String> {
        let conn = self.conn.lock();
        let agent_ids_json = serde_json::to_string(agent_ids).unwrap_or_else(|_| "[]".to_string());
        conn.execute(
            "UPDATE community_skills SET agent_ids = ?1, updated_at = datetime('now') WHERE id = ?2",
            rusqlite::params![agent_ids_json, id],
        ).map_err(|e| format!("Update agent_ids error: {}", e))?;
        Ok(())
    }
}

/// Get instructions from enabled community skills for a specific agent.
/// Skills with empty agent_ids apply to ALL agents.
/// Skills with specific agent_ids only apply to those agents.
pub fn get_community_skill_instructions(store: &SessionStore, agent_id: &str) -> Result<String, String> {
    let skills = store.list_community_skills()?;
    let mut sections: Vec<String> = Vec::new();

    for skill in &skills {
        if !skill.enabled || skill.instructions.is_empty() {
            continue;
        }
        // Filter: empty agent_ids = all agents, otherwise must contain this agent
        if !skill.agent_ids.is_empty() && !skill.agent_ids.contains(&agent_id.to_string()) {
            continue;
        }
        sections.push(format!(
            "## {} (community)\n{}",
            skill.name, skill.instructions
        ));
    }

    if sections.is_empty() {
        return Ok(String::new());
    }

    Ok(format!(
        "\n\n# Community Skills\nYou have the following community skills installed. Follow their instructions when relevant.\n\n{}\n",
        sections.join("\n\n")
    ))
}