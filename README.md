<div align="center">

# OpenPawz

**Your AI, your rules.**

A native desktop AI platform that runs fully offline, connects to any provider, and puts you in control.

[![CI](https://github.com/OpenPawz/openpawz/actions/workflows/ci.yml/badge.svg)](https://github.com/OpenPawz/openpawz/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

*Private by default. Powerful by design. Extensible by nature.*

</div>

---

## Why OpenPawz?

OpenPawz is a native Tauri v2 application with a pure Rust backend engine. It runs fully offline with Ollama, connects to any OpenAI-compatible provider, and gives you complete control over your AI agents, data, and tools.

- **Private** — No cloud, no telemetry, no open ports. Credentials encrypted with AES-256-GCM in your OS keychain.
- **Powerful** — Multi-agent orchestration, 11 channel bridges, hybrid memory, DeFi trading, browser automation, research workflows.
- **Extensible** — Unlimited providers, community skills via PawzHub, MCP server support, 75+ built-in tools, modular architecture.
- **Tiny** — ~5 MB native binary. Not a 200 MB Electron wrapper.

---

## Quality

Every commit is validated by a 3-job CI pipeline: Rust (check + test + clippy), TypeScript (tsc + eslint + vitest + prettier), and Security (cargo audit + npm audit). See [ENTERPRISE_PLAN.md](ENTERPRISE_PLAN.md) for the full hardening audit.

---

## Security

OpenPawz takes a defense-in-depth approach with 7 security layers. The agent never touches the OS directly — every tool call flows through the Rust engine where it can be intercepted, classified, and blocked.

1. **Prompt injection scanner** — Dual TypeScript + Rust detection, 30+ patterns
2. **Command risk classifier** — 30+ danger patterns across 5 risk levels
3. **Human-in-the-Loop approval** — Side-effect tools require explicit user approval
4. **Per-agent tool policies** — Allowlist, denylist, or unrestricted mode per agent
5. **Container sandboxing** — Docker isolation with `CAP_DROP ALL`, memory/CPU limits, network disabled
6. **Browser network policy** — Domain allowlist/blocklist prevents data exfiltration
7. **Credential vault** — OS keychain + AES-256-GCM encrypted SQLite; keys never appear in prompts

See [SECURITY.md](SECURITY.md) for the complete security architecture.

---

## Features

### Multi-Agent System
- Unlimited agents with custom personalities, models, and tool policies
- Boss/worker orchestration — agents delegate tasks and spawn sub-agents at runtime
- Inter-agent communication — direct messages, broadcast channels, and agent squads
- Agent squads — team formation with coordinator roles for collaborative tasks
- Per-agent chat sessions with persistent history and mini-chat popups
- Agent dock with avatars (50 custom Pawz Boi sprites)

### 10 AI Providers
| Provider | Models |
|----------|--------|
| Ollama | Any local model (auto-detected, fully offline) |
| OpenAI | GPT-4o, o1, o3-mini |
| Anthropic | Claude Sonnet 4, Opus 4, Haiku |
| Google Gemini | Gemini 2.5 Pro/Flash |
| OpenRouter | Meta-provider routing |
| DeepSeek | deepseek-chat, deepseek-reasoner |
| xAI (Grok) | grok-3, grok-3-mini |
| Mistral | mistral-large, codestral, pixtral |
| Moonshot/Kimi | moonshot-v1 models |
| Custom | Any OpenAI-compatible endpoint |

### 11 Channel Bridges
Telegram · Discord · IRC · Slack · Matrix · Mattermost · Nextcloud Talk · Nostr · Twitch · WebChat · WhatsApp

Each bridge includes user approval flows, per-agent routing, and uniform start/stop/config commands. The same agent brain, memory, and tools work across every platform.

### Memory System
- Hybrid BM25 + vector similarity search with Ollama embeddings
- MMR re-ranking for diversity (lambda=0.7)
- Temporal decay with 30-day half-life
- Auto-recall and auto-capture per agent
- Memory Palace visualization UI

### Built-in Tools & Skills
- 75+ built-in tools across 21 modules with encrypted credential injection
- Community skills from the [skills.sh](https://skills.sh) ecosystem and PawzHub marketplace
- Three-tier extensibility: Skills (SKILL.md) → Integrations (pawz-skill.toml) → Extensions (custom views + storage)
- Kanban task board with agent assignment, cron scheduling, and event-driven triggers
- Inter-agent communication — direct messaging and broadcast channels
- Agent squads — team formation with coordinator roles and squad broadcasts
- Persistent background tasks with automatic re-queuing
- Research workflow with findings and synthesis
- Full email client (IMAP/SMTP via Himalaya)
- Browser automation with managed profiles
- DeFi trading on ETH (7 EVM chains) + Solana (Jupiter, PumpPortal)
- Dashboard widgets with skill output persistence
- 15 slash commands with autocomplete

### Webhooks & MCP
- Generic webhook server — receive external events and route to agents
- MCP (Model Context Protocol) client — connect to any MCP server for additional tools
- Per-agent MCP server assignment
- Event-driven task triggers — tasks fire on webhooks or inter-agent messages
- Auto-approve mode for fully autonomous agent operation

### Voice
- Google TTS (Chirp 3 HD, Neural2, Journey)
- OpenAI TTS (9 voices)
- ElevenLabs TTS (16 premium voices)
- Talk Mode — continuous voice loop (mic → STT → agent → TTS → speaker)

---

## Architecture

```
Frontend (TypeScript)                  Rust Engine
┌──────────────────────┐              ┌────────────────────────────────┐
│ Vanilla DOM · 20+ views │◄── IPC ──► │ Tauri commands                  │
│ Feature modules         │   (typed)  │ Channel bridges                 │
│ Material Icons          │            │ AI providers                    │
│                         │            │ Tool executor + HIL approval    │
└──────────────────────┘              │ AES-256-GCM encrypted SQLite    │
                                       │ OS keychain · Docker sandbox    │
                                       └────────────────────────────────┘
```

No Node.js backend. No gateway process. No open ports. Everything flows through Tauri IPC.

See [ARCHITECTURE.md](ARCHITECTURE.md) for the full technical breakdown.

---

## Getting Started

### Prerequisites
- [Node.js](https://nodejs.org/) 18+
- [Rust](https://rustup.rs/) (latest stable)
- Platform dependencies for Tauri — see [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

### Development

```bash
git clone https://github.com/OpenPawz/openpawz.git
cd paw
npm install
npm run tauri dev
```

### Run Tests

```bash
# TypeScript tests
npx vitest run

# Rust tests
cd src-tauri && cargo test

# Lint
npx tsc --noEmit
cd src-tauri && cargo clippy -- -D warnings
```

### Production Build

```bash
npm run tauri build
```

---

## Documentation

| Document | Description |
|----------|-------------|
| [ARCHITECTURE.md](ARCHITECTURE.md) | Full technical breakdown — directory structure, module design, data flow |
| [SECURITY.md](SECURITY.md) | Complete security architecture — 7 layers, threat model, credential handling |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Development setup, code style, testing, PR guidelines |
| [ENTERPRISE_PLAN.md](ENTERPRISE_PLAN.md) | Enterprise hardening audit — all phases with test counts |
| [AUTONOMY_ROADMAP.md](AUTONOMY_ROADMAP.md) | Agent autonomy roadmap — auto-approve, webhooks, MCP, PawzHub |
| [Docs Site](https://openpawz.mintlify.dev) | Full documentation with guides, channel setup, and API reference |

---

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Framework | [Tauri v2](https://v2.tauri.app/) |
| Backend | Rust (async, Tokio) |
| Frontend | TypeScript (vanilla DOM) |
| Database | SQLite (21 tables, AES-256-GCM encrypted fields) |
| Bundler | Vite |
| Testing | vitest (TS) + cargo test (Rust) |
| CI | GitHub Actions (3 parallel jobs) |

---

## License

MIT — See [LICENSE](LICENSE)
