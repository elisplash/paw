---
sidebar_position: 4
title: Skills
---

# Skills

The Skill Vault is where you enable integrations, CLI tools, and API connections for your agents.

## How skills work

1. **Enable** a skill in Settings → Skills
2. **Provide credentials** (API keys, tokens) if required
3. **Install binaries** (CLI tools) if required
4. Enabled skills inject their tools and instructions into agent prompts automatically

## Skill architecture

Skills are Rust functions registered in `skills.rs` that the agent can invoke as tools. The architecture works as follows:

- **Registration**: Each skill is defined as a `SkillDefinition` struct in `skills.rs`, specifying its ID, name, category, required credentials, tool names, required binaries, and agent instructions.
- **Per-agent toggle**: Every skill is gated by an enable/disable toggle per-agent. When disabled, the skill's tools and instructions are not injected into the agent's prompt.
- **Categories**: Skills are organized into 9 categories for the UI. Each skill belongs to exactly one category and can be independently toggled on or off.
- **Vault skills** have dedicated tool functions (e.g. `email_send`, `coinbase_trade`) with encrypted credential storage. The engine executes these tools directly — credentials never appear in the agent prompt.
- **Instruction skills** inject CLI/API knowledge into the agent's system prompt. The agent then uses its built-in `exec` (shell) and `fetch` (HTTP) tools to interact with CLIs and APIs.
- **Credential encryption**: Credentials are stored encrypted in SQLite. The encryption key lives in the OS keychain, separate from the database.

## Skill categories

Paw ships with **40 built-in skills** across **9 categories**:

| # | Category | Description | Skill count |
|---|----------|-------------|-------------|
| 1 | **Vault** | Credential-based integrations with dedicated tool functions | 10 |
| 2 | **CLI** | Command-line tools the agent uses via `exec` | 3 |
| 3 | **API Integrations** | Third-party APIs the agent calls via `fetch`/`exec` | 4 |
| 4 | **Productivity** | Notes, reminders, and project management apps | 5 |
| 5 | **Media** | Audio, video, image generation, and music | 7 |
| 6 | **Smart Home** | IoT devices and home automation | 4 |
| 7 | **Communication** | Messaging platforms (WhatsApp, iMessage) | 2 |
| 8 | **Development** | Coding, terminals, and session logs | 2 |
| 9 | **System** | Security, UI automation, and password management | 3 |

### Vault (Credential-based)

| Skill | Credentials | Tools |
|-------|------------|-------|
| **Email** | SMTP_HOST, SMTP_PORT, SMTP_USER, SMTP_PASS, IMAP_HOST, IMAP_USER, IMAP_PASS | `email_send`, `email_read` |
| **Slack** | SLACK_BOT_TOKEN | `slack_send`, `slack_read` |
| **Telegram** | (uses channel bridge token) | `telegram_send`, `telegram_read` |
| **GitHub** | GITHUB_TOKEN (+ `gh` CLI) | `github_api` |
| **REST API** | REST_API_BASE_URL, REST_API_KEY | `rest_api_call` |
| **Webhooks** | WEBHOOK_URL, WEBHOOK_SECRET | `webhook_send` |
| **Discord** | DISCORD_BOT_TOKEN, DISCORD_DEFAULT_CHANNEL | `discord_send` (via Discord REST API) |
| **Coinbase** | COINBASE_API_KEY, COINBASE_API_SECRET | `coinbase_balance`, `coinbase_price`, `coinbase_buy`, `coinbase_sell`, `coinbase_send` |
| **DEX Trading** | DEX_RPC_URL, DEX_PRIVATE_KEY, DEX_WALLET_ADDRESS | 13 `dex_*` tools |
| **Solana DEX** | JUPITER_API_KEY, SOLANA_RPC_URL, SOLANA_PRIVATE_KEY, SOLANA_WALLET_ADDRESS | 7 `sol_*` tools |

### API Integrations

| Skill | Credentials | Required Binary |
|-------|------------|-----------------|
| **Notion** | NOTION_TOKEN | — |
| **Trello** | TRELLO_API_KEY, TRELLO_TOKEN | — |
| **Google Workspace** | — | `gog` |
| **Google Places** | GOOGLE_PLACES_API_KEY | `goplaces` |

### Productivity

| Skill | Required Binary |
|-------|-----------------|
| **Apple Notes** | `memo` |
| **Apple Reminders** | `remindctl` |
| **Things 3** | `things-cli` |
| **Obsidian** | `obsidian-cli` |
| **Bear Notes** | `grizzly` |

### Media

| Skill | Credentials | Required Binary |
|-------|------------|-----------------|
| **Whisper Local** | — | `whisper` |
| **Whisper API** | OPENAI_API_KEY | — |
| **Image Gen** | GEMINI_API_KEY | — |
| **Video Frames** | — | `ffmpeg` |
| **ElevenLabs TTS** | ELEVENLABS_API_KEY | `sag` |
| **Spotify** | — | `spotify_player` |
| **GIF Search** | — | `gifgrep` |

> **Image Gen** uses the `image_generate` tool to create images from text prompts via Gemini (Google AI). Provide a detailed description including style, lighting, composition, and mood for best results.

### Smart Home

| Skill | Required Binary | Description |
|-------|-----------------|-------------|
| **Philips Hue** | `openhue` | Control lights, rooms, and scenes |
| **Sonos** | `sonoscli` | Control speakers, playback, and volume |
| **Eight Sleep** | `eightctl` | Control pod temperature and alarms |
| **Camera Capture** | `camsnap` | Capture frames from RTSP/ONVIF/webcam cameras for vision analysis |

> **Camera Capture** (`camsnap`) can snap images from IP cameras, discover cameras on your network, and capture timed frame sequences. Use it for security monitoring, pet watching, or any vision-analysis workflow. Commands: `camsnap snap <url>`, `camsnap discover`, `camsnap stream <url> --frames 10 --interval 1s`.

### Communication

| Skill | Required Binary |
|-------|-----------------|
| **WhatsApp** | `wacli` |
| **iMessage** | `imsg` |

### CLI Tools

| Skill | Description |
|-------|-------------|
| **Weather** | Weather information |
| **Blog Watcher** | Monitor blogs for updates |
| **Summarize** | Text summarization (`summarize` binary) |

### Development

| Skill | Required Binary |
|-------|-----------------|
| **tmux** | `tmux` |
| **Session Logs** | `rg` (ripgrep) |

### System

| Skill | Required Binary |
|-------|-----------------|
| **1Password** | `op` |
| **Peekaboo** | `peekaboo` |
| **Security Audit** | — |

## Credential security

Credentials are stored in SQLite and encrypted with XOR using a 32-byte random key stored in your OS keychain (service: `paw-skill-vault`, key: `encryption-key`).

High-risk credentials (Coinbase, DEX) are **server-side only** — they are never injected into agent prompts. The engine uses them directly when executing trades.

## Custom instructions

Each skill can have custom instructions that override the defaults. These are injected into the agent's system prompt when the skill is enabled.

## Readiness check

The skill status indicator shows:
- **Ready** — all credentials and binaries present
- **Missing credentials** — API keys needed
- **Missing binary** — CLI tool not installed
- **Disabled** — skill is turned off
