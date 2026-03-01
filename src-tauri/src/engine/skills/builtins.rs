// Pawz Agent Engine — Built-in Skill Definitions
// 400+ built-in skill definitions.

use super::types::{CredentialField, SkillCategory, SkillDefinition, SkillTier};

pub fn builtin_skills() -> Vec<SkillDefinition> {
    vec![
        // ───── VAULT SKILLS (dedicated tool functions + credentials) ─────

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
            default_enabled: false,
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
            default_enabled: false,
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
            default_enabled: false,
        },
        SkillDefinition {
            id: "discord".into(),
            name: "Discord".into(),
            description: "Full Discord server management — channels, roles, categories, messages, members, permissions, and more".into(),
            icon: "🎮".into(),
            category: SkillCategory::Vault,
            tier: SkillTier::Integration,
            required_credentials: vec![
                CredentialField { key: "DISCORD_BOT_TOKEN".into(), label: "Bot Token".into(), description: "Discord bot token".into(), required: true, placeholder: "MTIz...".into() },
                CredentialField { key: "DISCORD_DEFAULT_CHANNEL".into(), label: "Default Channel ID".into(), description: "Default channel to post to".into(), required: false, placeholder: "1234567890".into() },
                CredentialField { key: "DISCORD_SERVER_ID".into(), label: "Server (Guild) ID".into(), description: "Right-click server → Copy Server ID (enable Developer Mode in Discord settings)".into(), required: false, placeholder: "1234567890".into() },
            ],
            tool_names: vec![
                // channels
                "discord_list_channels".into(), "discord_setup_channels".into(),
                "discord_delete_channels".into(), "discord_edit_channel".into(),
                // messages
                "discord_send_message".into(), "discord_edit_message".into(),
                "discord_delete_messages".into(), "discord_get_messages".into(),
                "discord_pin_message".into(), "discord_unpin_message".into(), "discord_react".into(),
                // roles
                "discord_list_roles".into(), "discord_create_role".into(),
                "discord_delete_role".into(), "discord_assign_role".into(), "discord_remove_role".into(),
                // members
                "discord_list_members".into(), "discord_get_member".into(),
                "discord_kick".into(), "discord_ban".into(), "discord_unban".into(),
                // server
                "discord_server_info".into(), "discord_create_invite".into(),
            ],
            required_binaries: vec![], required_env_vars: vec![], install_hint: "Bot must be invited with Administrator permission (permission value 8) for full server management.".into(),
            agent_instructions: r#"You have full Discord bot access with 23 built-in tools:

**Channels**: discord_list_channels, discord_setup_channels, discord_delete_channels, discord_edit_channel
**Messages**: discord_send_message, discord_edit_message, discord_delete_messages, discord_get_messages, discord_pin_message, discord_unpin_message, discord_react
**Roles**: discord_list_roles, discord_create_role, discord_delete_role, discord_assign_role, discord_remove_role
**Members**: discord_list_members, discord_get_member, discord_kick, discord_ban, discord_unban
**Server**: discord_server_info, discord_create_invite

TOOL SELECTION RULES:
- CREATE channels/categories → discord_setup_channels (idempotent, skips existing)
- SEND/POST messages → discord_send_message
- DELETE messages → discord_delete_messages (single or bulk purge)
- VIEW channels → discord_list_channels
- VIEW messages → discord_get_messages
NEVER use discord_setup_channels to send messages.

Server ID and channel IDs resolve automatically from credentials when not provided.
Do NOT run exec/curl to call the Discord API — use your built-in tools."#.into(),
            default_enabled: false,
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
            default_enabled: false,
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
            default_enabled: false,
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
            default_enabled: false,
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
            default_enabled: false,
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
            default_enabled: false,
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
            default_enabled: false,
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
            default_enabled: false,
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
            default_enabled: false,
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
            default_enabled: false,
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
            default_enabled: false,
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
            default_enabled: false,
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
            default_enabled: false,
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
            default_enabled: false,
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
            default_enabled: false,
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
            default_enabled: false,
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
            default_enabled: false,
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
            default_enabled: false,
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
            default_enabled: false,
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
            default_enabled: false,
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
            default_enabled: true,
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
            default_enabled: true,
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
            default_enabled: false,
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
            default_enabled: false,
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
            default_enabled: false,
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
            default_enabled: false,
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
            default_enabled: false,
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
            default_enabled: false,
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
            default_enabled: false,
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
            default_enabled: false,
        },
        SkillDefinition {
            id: "dex".into(),
            name: "DEX Trading (EVM)".into(),
            description: "Self-custody Ethereum wallet with Uniswap V3 on-chain swaps, whale tracking, and trending tokens".into(),
            icon: "🦄".into(),
            category: SkillCategory::Vault,
            tier: SkillTier::Integration,
            required_credentials: vec![
                CredentialField { key: "ETHEREUM_PRIVATE_KEY".into(), label: "Ethereum Private Key".into(), description: "Your Ethereum wallet private key (hex, with or without 0x prefix). Used for signing transactions locally — never sent to any server.".into(), required: true, placeholder: "0xabcdef1234567890...".into() },
            ],
            tool_names: vec!["dex_wallet_create".into(), "dex_balance".into(), "dex_quote".into(), "dex_swap".into(), "dex_transfer".into(), "dex_portfolio".into(), "dex_token_info".into(), "dex_check_token".into(), "dex_search_token".into(), "dex_watch_wallet".into(), "dex_whale_transfers".into(), "dex_top_traders".into(), "dex_trending".into()],
            required_binaries: vec![], required_env_vars: vec![], install_hint: "Import or create an Ethereum wallet".into(),
            agent_instructions: r#"You have EVM DEX trading tools for self-custody Ethereum trading.
Credentials are injected automatically. Do NOT read source code or key files.

Available tools:
- **dex_wallet_create**: Create or import an Ethereum wallet. Requires approval.
- **dex_balance**: Check ETH and token balances.
- **dex_quote**: Get swap quotes from Uniswap V3 before executing.
- **dex_swap**: Execute on-chain token swaps. ALWAYS requires approval.
- **dex_transfer**: Send ETH or tokens. ALWAYS requires approval.
- **dex_portfolio**: View full portfolio with USD values.
- **dex_token_info**: Get token details (price, liquidity, contract info).
- **dex_check_token**: Audit a token contract for rug-pull risks.
- **dex_search_token**: Search tokens by name or symbol.
- **dex_watch_wallet**: Track another wallet's activity.
- **dex_whale_transfers**: Monitor large transfers on-chain.
- **dex_top_traders**: Find top traders for a specific token.
- **dex_trending**: Get trending tokens on DEXes.

Risk Management:
- NEVER risk more than 2% of portfolio on a single swap
- Always check token safety with dex_check_token before buying new tokens
- Always get a quote before executing swaps
- Warn about gas costs on Ethereum mainnet
- Check liquidity depth before large trades"#.into(),
            default_enabled: false,
        },
        SkillDefinition {
            id: "solana_dex".into(),
            name: "Trading: Solana DEX".into(),
            description: "Solana self-custody wallet with Jupiter aggregator swaps and PumpPortal integration for pump.fun tokens".into(),
            icon: "☀️".into(),
            category: SkillCategory::Vault,
            tier: SkillTier::Integration,
            required_credentials: vec![
                CredentialField { key: "SOLANA_PRIVATE_KEY".into(), label: "Solana Private Key".into(), description: "Your Solana wallet private key (base58 encoded). Used for signing transactions locally — never sent to any server.".into(), required: true, placeholder: "4wBqpZ...base58...".into() },
            ],
            tool_names: vec!["sol_wallet_create".into(), "sol_balance".into(), "sol_quote".into(), "sol_swap".into(), "sol_transfer".into(), "sol_portfolio".into(), "sol_token_info".into()],
            required_binaries: vec![], required_env_vars: vec![], install_hint: "Import or create a Solana wallet".into(),
            agent_instructions: r#"You have Solana DEX trading tools for self-custody trading on Solana.
Credentials are injected automatically. Do NOT read source code or key files.

Available tools:
- **sol_wallet_create**: Create or import a Solana wallet. Requires approval.
- **sol_balance**: Check SOL and SPL token balances.
- **sol_quote**: Get swap quotes from Jupiter aggregator.
- **sol_swap**: Execute on-chain swaps via Jupiter or PumpPortal (for pump.fun tokens). ALWAYS requires approval.
- **sol_transfer**: Send SOL or SPL tokens. ALWAYS requires approval.
- **sol_portfolio**: View full portfolio with USD values.
- **sol_token_info**: Get token details, price, and metadata.

PumpPortal Integration:
- For pump.fun tokens, swaps automatically route through PumpPortal
- These tokens may have extreme volatility — always warn the user
- Check token age and holder distribution before buying

Risk Management:
- NEVER risk more than 2% of portfolio on a single swap
- Always get a quote before executing swaps
- Warn about pump.fun token risks (rug pulls, low liquidity)
- Check slippage tolerance — default 1% for established tokens, suggest higher for pump.fun
- Solana transactions are fast but check for congestion"#.into(),
            default_enabled: false,
        },
    ]
}
