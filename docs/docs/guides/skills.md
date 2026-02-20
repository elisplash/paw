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

## Skill categories

### Vault (Credential-based)

| Skill | Credentials | Tools |
|-------|------------|-------|
| **Email** | SMTP_HOST, SMTP_PORT, SMTP_USER, SMTP_PASS, IMAP_HOST, IMAP_USER, IMAP_PASS | `email_send`, `email_read` |
| **Slack** | SLACK_BOT_TOKEN | `slack_send`, `slack_read` |
| **Telegram** | (uses channel bridge token) | — |
| **GitHub** | GITHUB_TOKEN (+ `gh` CLI) | `github_api` |
| **REST API** | REST_API_BASE_URL, REST_API_KEY | `rest_api_call` |
| **Webhooks** | WEBHOOK_URL, WEBHOOK_SECRET | `webhook_send` |
| **Discord** | DISCORD_BOT_TOKEN | — |
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
| **Image Gen** | — | — |
| **Video Frames** | — | `ffmpeg` |
| **ElevenLabs TTS** | ELEVENLABS_API_KEY | `sag` |
| **Spotify** | — | `spotify_player` |
| **GIF Search** | — | `gifgrep` |

### Smart Home

| Skill | Required Binary |
|-------|-----------------|
| **Philips Hue** | `openhue` |
| **Sonos** | `sonoscli` |
| **Eight Sleep** | `eightctl` |
| **Camera Capture** | `camsnap` |

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
