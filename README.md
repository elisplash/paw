<div align="center">

<img src="images/pawz-logo-transparent.png" alt="OpenPawz logo" width="200">

<br>

**Your AI, your rules.**

A native desktop AI platform that runs fully offline, connects to any provider, and puts you in control.

[![CI](https://github.com/OpenPawz/openpawz/actions/workflows/ci.yml/badge.svg)](https://github.com/OpenPawz/openpawz/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Discord](https://img.shields.io/badge/Discord-Join-5865F2?logo=discord&logoColor=white)](https://discord.gg/wVvmgrMV)
[![X (Twitter)](https://img.shields.io/badge/Follow-%40openpawzai-000000?logo=x&logoColor=white)](https://x.com/openpawzai)
[![Instagram](https://img.shields.io/badge/Follow-%40openpawz-E4405F?logo=instagram&logoColor=white)](https://www.instagram.com/openpawz)

*Private by default. Powerful by design. Extensible by nature.*

</div>

---

## Screenshots

<div align="center">

| Integration Hub | Fleet Command | Chat |
|:---:|:---:|:---:|
| <img src="images/screenshots/Integrations.png" alt="Integration Hub" width="300"> | <img src="images/screenshots/Agents.png" alt="Fleet Command" width="300"> | <img src="images/screenshots/Chat.png" alt="Chat" width="300"> |
| 405+ services with category filters, connection health, and quick setup | Manage agents, deploy templates, monitor fleet activity | Session metrics, active jobs, quick actions, automations |

</div>

---

## Why OpenPawz?

OpenPawz is a native Tauri v2 application with a pure Rust backend engine. It runs fully offline with Ollama, connects to any OpenAI-compatible provider, and gives you complete control over your AI agents, data, and tools.

- **Private** — No cloud, no telemetry, no open ports. Credentials encrypted with AES-256-GCM in your OS keychain.
- **Powerful** — Multi-agent orchestration, 11 channel bridges, hybrid memory, DeFi trading, browser automation, research workflows.
- **Extensible** — 400+ integrations out of the box, unlimited providers, community skills via PawzHub, MCP server support, modular architecture.
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

### 400+ Integrations Out of the Box
Every integration ships ready to use — no plugins to install, no marketplace to browse. Configure credentials once and assign integrations per-agent.

| Category | Count | Examples |
|----------|-------|----------|
| Productivity | 40+ | Notion, Trello, Obsidian, Linear, Jira, Asana, Todoist, Google Workspace |
| Communication | 30+ | Slack, Discord, Telegram, WhatsApp, Teams, Email (IMAP/SMTP) |
| Development | 50+ | GitHub, GitLab, Bitbucket, Docker, Kubernetes, Vercel, Netlify, AWS |
| Data & Analytics | 35+ | PostgreSQL, MongoDB, Redis, Elasticsearch, BigQuery, Snowflake |
| Media & Content | 25+ | Spotify, YouTube, Whisper, ElevenLabs, Image Gen, DALL-E |
| Smart Home & IoT | 20+ | Philips Hue, Sonos, Home Assistant, MQTT, Zigbee |
| Finance & Trading | 30+ | Coinbase, Solana DEX, Ethereum DEX, Stripe, PayPal, QuickBooks |
| Cloud & Infrastructure | 40+ | AWS, GCP, Azure, Cloudflare, DigitalOcean, Terraform |
| Security & Monitoring | 25+ | 1Password, Vault, Datadog, PagerDuty, Sentry, Grafana |
| AI & ML | 20+ | Hugging Face, Replicate, Stability AI, Pinecone, Weaviate |
| CRM & Marketing | 30+ | Salesforce, HubSpot, Mailchimp, SendGrid, Intercom |
| Miscellaneous | 55+ | Weather, RSS, Web Scraping, PDF, OCR, QR codes, Maps |

### 10 AI Providers
| Provider | Models |
|----------|--------|
| Ollama | Any local model (auto-detected, fully offline) |
| OpenAI | GPT-4.1, GPT-4.1 mini, GPT-4.1 nano, o3, o4-mini |
| Anthropic | Claude Opus 4, Sonnet 4, Sonnet 4 Thinking, Haiku 3.5 |
| Google Gemini | Gemini 3.1 Pro, 3 Pro, 3 Flash (Preview), 2.5 Pro/Flash/Flash-Lite |
| OpenRouter | Meta-provider routing (100+ models) |
| DeepSeek | deepseek-chat, deepseek-reasoner |
| xAI (Grok) | grok-3, grok-3-mini |
| Mistral | mistral-large, codestral, pixtral-large |
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
- 400+ integrations across 12 categories with encrypted credential injection
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
│ Kinetic Intelligence    │   (typed)  │ 400+ integration engine         │
│ Material Icons          │            │ AI providers · Channel bridges  │
│                         │            │ Tool executor + HIL approval    │
└──────────────────────┘              │ AES-256-GCM encrypted SQLite    │
                                       │ OS keychain · Docker sandbox    │
                                       └────────────────────────────────┘
```

No Node.js backend. No gateway process. No open ports. Everything flows through Tauri IPC.

See [ARCHITECTURE.md](ARCHITECTURE.md) for the full technical breakdown.

---

## Installation

### Prerequisites

> **Note:** Node.js is only needed to build the frontend — the final app is a standalone ~5 MB native binary with no Node.js runtime.

| Requirement | Version | Why | Install |
|-------------|---------|-----|---------|
| **Node.js** | 18+ | Vite bundler + TypeScript compiler (build-time only) | [nodejs.org](https://nodejs.org/) |
| **Rust** | Latest stable | Compiles the native backend engine | [rustup.rs](https://rustup.rs/) |
| **Platform deps** | — | WebKit, SSL, system libraries (see below) | Per-platform |

#### Optional (runtime)

| Tool | Purpose | Install |
|------|---------|---------|
| **Ollama** | Fully local AI — no API keys needed | [ollama.com](https://ollama.com/) |
| **Docker** | Container sandboxing for agent commands | [docker.com](https://www.docker.com/) |
| **gnome-keyring** or **kwallet** | OS keychain for credential encryption (Linux) | System package manager |

### Platform-Specific Dependencies

<details>
<summary><strong>Linux (Debian / Ubuntu)</strong></summary>

```bash
# System libraries required by Tauri + WebKit
sudo apt update
sudo apt install -y \
  libwebkit2gtk-4.1-dev \
  build-essential \
  curl \
  wget \
  file \
  libxdo-dev \
  libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev

# Keychain (required for credential encryption)
# GNOME-based desktops usually have this already
sudo apt install -y gnome-keyring libsecret-1-dev

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# Install Node.js 18+ (via nvm)
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.1/install.sh | bash
nvm install 22
```

</details>

<details>
<summary><strong>Linux (Fedora)</strong></summary>

```bash
sudo dnf install -y \
  webkit2gtk4.1-devel \
  openssl-devel \
  curl \
  wget \
  file \
  libxdo-devel \
  libappindicator-gtk3-devel \
  librsvg2-devel \
  gnome-keyring \
  libsecret-devel

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

</details>

<details>
<summary><strong>Linux (Arch)</strong></summary>

```bash
sudo pacman -S --needed \
  webkit2gtk-4.1 \
  base-devel \
  curl \
  wget \
  file \
  openssl \
  libxdo \
  libappindicator-gtk3 \
  librsvg \
  gnome-keyring \
  libsecret

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

</details>

<details>
<summary><strong>macOS</strong></summary>

```bash
# Install Xcode command line tools (provides clang, make, etc.)
xcode-select --install

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# Install Node.js (via Homebrew)
brew install node
```

macOS Keychain is used automatically — no additional setup needed.

</details>

<details>
<summary><strong>Windows</strong></summary>

1. Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with:
   - "Desktop development with C++" workload
   - Windows 10/11 SDK
2. Install [Rust](https://rustup.rs/) — download and run `rustup-init.exe`
3. Install [Node.js 18+](https://nodejs.org/) — use the LTS installer

Windows Credential Manager is used automatically for the keychain.

</details>

<details>
<summary><strong>Containers / CI / Headless Linux</strong></summary>

If you're running in a Docker container, devcontainer, or headless server, there's no graphical keychain by default. You need to start one manually:

```bash
# Install gnome-keyring
sudo apt install -y gnome-keyring dbus-x11

# Start the keyring daemon
eval $(dbus-launch --sh-syntax)
eval $(gnome-keyring-daemon --start --components=secrets 2>/dev/null)
export GNOME_KEYRING_CONTROL
```

Without a running keychain, credential encryption will fail and integrations won't work. The app's **Settings → Security** panel shows keychain health status.

</details>

---

### Quick Start

```bash
# 1. Clone the repository
git clone https://github.com/OpenPawz/openpawz.git
cd paw

# 2. Install frontend dependencies
npm install

# 3. Run in development mode (hot-reload frontend + live Rust rebuilds)
npm run tauri dev
```

> **First build takes 3–5 minutes** while Rust compiles all dependencies. Subsequent builds are incremental (~5–15 seconds).

### Frontend Only (No Rust / Tauri Required)

If you just want to run the frontend UI without the Rust backend (useful for UI development or quick previews):

```bash
npm install
npm run dev
```

This starts the Vite dev server at `http://localhost:1420/` with hot-reload. The full Tauri backend (provider calls, credential vault, container sandbox, etc.) won't be available in this mode, but all views and UI components will render.

### Verify It's Working

After launching, OpenPawz opens to the Today dashboard. To verify everything is functional:

1. **Settings → Security** — check that keychain health shows "Healthy"
2. **Settings → Providers** — configure at least one AI provider (or install Ollama for local AI)
3. **Agents** — create an agent and start chatting

---

### Optional: Ollama (Fully Local AI)

For completely offline AI with no API keys or cloud dependency:

```bash
# Install Ollama
curl -fsSL https://ollama.com/install.sh | sh

# Pull a chat model
ollama pull llama3.1

# Pull the embedding model (used for memory search)
ollama pull nomic-embed-text
```

OpenPawz auto-detects Ollama on `localhost:11434` and lists available models automatically in **Settings → Providers**.

---

### Optional: Docker (Container Sandboxing)

To enable sandboxed command execution for agents:

```bash
# Install Docker (if not already installed)
curl -fsSL https://get.docker.com | sh
sudo usermod -aG docker $USER
# Log out and back in for group changes to take effect

# Verify Docker works
docker run --rm hello-world
```

Container sandboxing runs agent shell commands inside isolated Docker containers with `CAP_DROP ALL`, memory/CPU limits, and network disabled by default. Configure in **Settings → Security**.

---

### Configuring Integrations

OpenPawz stores all credentials in an AES-256-GCM encrypted vault backed by your OS keychain. There are two ways to add credentials:

**Option A: Settings → Skills** (recommended)
1. Open **Settings → Skills**
2. Find the integration (e.g. Slack, GitHub, n8n)
3. Enter your credentials and click **Save**
4. Toggle the skill to **Enabled**

**Option B: Integrations panel** (if using n8n)
1. Open the **Integrations** view
2. Click the service and follow the setup guide
3. Enter credentials, click **Test & Save**
4. The app tests the connection, then auto-provisions to the skill vault

> **Important:** Credentials must be saved through the app UI — setting environment variables (`.env` files, shell exports) does not work. The agent tools read exclusively from the encrypted skill vault in SQLite, not from environment variables.

---

### Run Tests

```bash
# TypeScript tests (360 tests)
npx vitest run

# Rust tests (242 tests)
cd src-tauri && cargo test

# TypeScript type-check
npx tsc --noEmit

# Rust lint (zero warnings enforced)
cd src-tauri && cargo clippy -- -D warnings

# Code formatting check
npx prettier --check "src/**/*.ts"
cd src-tauri && cargo fmt --check

# Run everything at once
npm run check
```

### Production Build

```bash
npm run tauri build
```

The built app will be in `src-tauri/target/release/bundle/` — platform-specific installer:

| Platform | Output |
|----------|--------|
| macOS | `.dmg` + `.app` |
| Linux | `.deb` + `.AppImage` |
| Windows | `.msi` + `.exe` |

---

### Troubleshooting

| Problem | Fix |
|---------|-----|
| **First build fails on Linux** | Make sure all system libraries are installed (see platform deps above) |
| **"Keyring init failed"** | No keychain daemon running — install `gnome-keyring` and start it (see headless section) |
| **"Missing required credentials" for a skill** | Credentials must be saved via the app UI (**Settings → Skills**), not via `.env` files |
| **Provision silently fails** | Check **Settings → Security** — if keychain is "unavailable", the vault can't encrypt credentials |
| **Ollama not detected** | Make sure Ollama is running (`ollama serve`) and accessible at `http://localhost:11434` |
| **n8n "no API access"** | Set `N8N_PUBLIC_API_ENABLED=true` in your n8n instance environment, restart n8n, and create an API key in n8n **Settings → API** |
| **Rust compilation OOM** | On low-memory machines (< 4 GB), close other apps or add swap: `sudo fallocate -l 4G /swapfile && sudo mkswap /swapfile && sudo swapon /swapfile` |
| **Docker sandbox won't start** | Ensure Docker is running and your user is in the `docker` group (`groups` to check) |

---

## Community

Join the conversation, share ideas, and follow development:

| Channel | Link |
|---------|------|
| Discord | [Join Server](https://discord.gg/wVvmgrMV) |
| X / Twitter | [@openpawzai](https://x.com/openpawzai) |
| Instagram | [@openpawz](https://www.instagram.com/openpawz) |
| Matrix | [#openpawz:matrix.org](https://matrix.to/#/#openpawz:matrix.org) |
| GitHub Discussions | [OpenPawz/openpawz Discussions](https://github.com/OpenPawz/openpawz/discussions) |
| Bluesky | [@openpawz.bsky.social](https://bsky.app/profile/openpawz.bsky.social) |
| Mastodon | [@openpawz@fosstodon.org](https://fosstodon.org/@openpawz) |

## Roadmap

Progress is tracked via [milestones](https://github.com/OpenPawz/openpawz/milestones) and [GitHub Projects](https://github.com/orgs/OpenPawz/projects):

- [**v0.2 — Packaging & Distribution**](https://github.com/OpenPawz/openpawz/milestone/1) — Stable binaries, Homebrew/AUR/Snap/Flatpak, Windows & macOS CI
- [**v0.3 — Plugin API & PawzHub**](https://github.com/OpenPawz/openpawz/milestone/2) — Community extension API, PawzHub marketplace, i18n
- [**v0.4 — Mobile & Sync**](https://github.com/OpenPawz/openpawz/milestone/3) — Mobile companion (iOS/Android), encrypted cloud sync
- [**v1.0 — Production Ready**](https://github.com/OpenPawz/openpawz/milestone/4) — Enterprise hardening, stable API, third-party security audit

See [ENTERPRISE_PLAN.md](ENTERPRISE_PLAN.md) for the hardening audit.

---

## Contributing

OpenPawz is built by one developer and needs your help. Every contribution matters — code, docs, tests, translations, packaging.

**Start here:**
- [`good first issue`](https://github.com/OpenPawz/openpawz/labels/good%20first%20issue) — scoped tasks for newcomers
- [`help wanted`](https://github.com/OpenPawz/openpawz/labels/help%20wanted) — bigger tasks we need help with
- [CONTRIBUTING.md](CONTRIBUTING.md) — full setup guide, code style, and "where to start" picker

**Claim an issue** by commenting "I'd like to work on this" — you'll be assigned within 24 hours. Questions? Ask in [Discord](https://discord.gg/wVvmgrMV) or [Discussions](https://github.com/OpenPawz/openpawz/discussions).

### Contributors

<a href="https://github.com/OpenPawz/openpawz/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=OpenPawz/openpawz" />
</a>

---

## Documentation

| Document | Description |
|----------|-------------|
| [ARCHITECTURE.md](ARCHITECTURE.md) | Full technical breakdown — directory structure, module design, data flow |
| [SECURITY.md](SECURITY.md) | Complete security architecture — 7 layers, threat model, credential handling |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Development setup, code style, testing, PR guidelines |
| [ENTERPRISE_PLAN.md](ENTERPRISE_PLAN.md) | Enterprise hardening audit — all phases with test counts |
| [CHANGELOG.md](CHANGELOG.md) | Version history and release notes |
| [Docs Site](https://www.openpawz.ai) | Full documentation with guides, channel setup, and API reference |

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
