// Pawz Agent Engine — Built-in Skill Definitions
// 40+ built-in skill definitions.

use super::types::{CredentialField, SkillCategory, SkillDefinition, SkillTier};

pub fn builtin_skills() -> Vec<SkillDefinition> {
    vec![
        // ───── VAULT SKILLS (dedicated tool functions + credentials) ─────

        SkillDefinition {
            id: "email".into(),
            name: "Email".into(),
            description: "Send and read emails via IMAP/SMTP".into(),
            icon: "📧".into(),
            category: SkillCategory::Vault,
            tier: SkillTier::Integration,
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
            tier: SkillTier::Integration,
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
            tier: SkillTier::Integration,
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
            tier: SkillTier::Integration,
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
            tier: SkillTier::Integration,
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
            tier: SkillTier::Integration,
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
            tier: SkillTier::Integration,
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
            tier: SkillTier::Integration,
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
            tier: SkillTier::Integration,
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
            tier: SkillTier::Integration,
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
            tier: SkillTier::Skill,
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
            tier: SkillTier::Skill,
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
            tier: SkillTier::Skill,
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
            tier: SkillTier::Skill,
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
            tier: SkillTier::Skill,
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
            tier: SkillTier::Skill,
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
            tier: SkillTier::Skill,
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
            tier: SkillTier::Skill,
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
            tier: SkillTier::Integration,
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
            tier: SkillTier::Integration,
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
            tier: SkillTier::Skill,
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
            tier: SkillTier::Integration,
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
            tier: SkillTier::Skill,
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
            tier: SkillTier::Skill,
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
            tier: SkillTier::Skill,
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
            tier: SkillTier::Skill,
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
            tier: SkillTier::Skill,
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
            tier: SkillTier::Skill,
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
            tier: SkillTier::Skill,
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
            tier: SkillTier::Skill,
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
            tier: SkillTier::Skill,
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
            description: "Gmail, Calendar, Drive, Sheets, and Docs via Google service account".into(),
            icon: "📧".into(),
            category: SkillCategory::Api,
            tier: SkillTier::Skill,
            required_credentials: vec![
                CredentialField {
                    key: "SERVICE_ACCOUNT_JSON".into(),
                    label: "Service Account JSON".into(),
                    description: "Paste the entire contents of your Google service account JSON key file".into(),
                    required: true,
                    placeholder: r#"{"type":"service_account","project_id":"...","private_key":"...","client_email":"...@...iam.gserviceaccount.com",...}"#.into(),
                },
                CredentialField {
                    key: "DELEGATE_EMAIL".into(),
                    label: "Delegated User Email".into(),
                    description: "The Google Workspace user email to impersonate (e.g. pawz@yourdomain.com). Required for domain-wide delegation.".into(),
                    required: true,
                    placeholder: "pawz@yourdomain.com".into(),
                },
            ],
            tool_names: vec![
                "google_gmail_list".into(),
                "google_gmail_read".into(),
                "google_gmail_send".into(),
                "google_calendar_list".into(),
                "google_calendar_create".into(),
                "google_drive_list".into(),
                "google_drive_read".into(),
                "google_sheets_read".into(),
                "google_sheets_append".into(),
                "google_api".into(),
            ],
            required_binaries: vec![],
            required_env_vars: vec![], install_hint: "Go to Skills → Google Workspace → Configure. Paste your service account JSON key and delegated email.".into(),
            agent_instructions: r#"You have full Google Workspace access via service account. Available tools:
- google_gmail_list: List/search emails. Use Gmail search operators (from:, to:, subject:, is:unread, etc).
- google_gmail_read: Read full email by message ID (from list results).
- google_gmail_send: Send emails. Supports plain text and HTML. Can CC recipients.
- google_calendar_list: List upcoming events with time range filtering.
- google_calendar_create: Schedule events with attendees, location, description.
- google_drive_list: Browse and search Drive files.
- google_drive_read: Read document contents (exports Google Docs/Sheets as text).
- google_sheets_read: Read spreadsheet data as markdown tables.
- google_sheets_append: Add rows to a spreadsheet.
- google_api: Generic authenticated call to any Google REST API endpoint.

Tips:
- Gmail message IDs come from google_gmail_list results.
- Drive file IDs come from google_drive_list results.
- For Sheets, the spreadsheet_id is the long string in the Google Sheets URL.
- Calendar times must be RFC3339 format like 2026-02-24T10:00:00-05:00."#.into(),
        },
        SkillDefinition {
            id: "google_places".into(),
            name: "Google Places".into(),
            description: "Search places, get details, reviews via Google Places API".into(),
            icon: "📍".into(),
            category: SkillCategory::Api,
            tier: SkillTier::Integration,
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
            tier: SkillTier::Skill,
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
            tier: SkillTier::Skill,
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
            tier: SkillTier::Skill,
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
            tier: SkillTier::Skill,
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
            tier: SkillTier::Skill,
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
            tier: SkillTier::Integration,
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
            tier: SkillTier::Integration,
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
