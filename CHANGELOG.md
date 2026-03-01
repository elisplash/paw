# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Fixed

#### n8n Community Packages
- **Install in Process mode** — npm install now runs in a space-free data dir (`~/.openpawz/n8n-data`) to avoid node-gyp failures, restarts n8n after install, and refreshes MCP bridge so new tools are available immediately
- **Uninstall** — rewritten with REST API → npm uninstall fallback (Process) / docker exec (Docker), n8n restart after removal, and MCP bridge refresh so stale tools disappear
- **Installed tab** — list now falls back to reading `package.json` from the data dir when the REST API returns 404 (n8n 2.9.x removed the community-packages endpoint)
- **Uninstall UX** — delete button shows a spinning "Removing…" indicator during uninstall with double-click protection to prevent duplicate operations
- **`confirm()` → `confirmModal()`** — replaced all `window.confirm()` calls across the app with the async `confirmModal()` helper; native `confirm()` does not render in Tauri 2 WKWebView on macOS

#### MCP Bridge
- **Auto-setup on every reconnect** — owner account creation + MCP access enablement now runs on every engine reconnect (not just fresh provision), with retry logic (3 attempts, 2s delay)
- **POST → GET for MCP token** — `has_mcp_support()` and `retrieve_mcp_token()` now use GET on `/rest/mcp/api-key` matching n8n 2.9.x's `getOrCreateApiKey()` API
- **JWT validation** — cached MCP tokens are now validated (must contain `.` separators, must not contain `*`) before reuse; stale/redacted tokens trigger automatic re-fetch
- **Redacted key rotation** — if n8n returns a redacted MCP API key, we automatically rotate via `POST /rest/mcp/api-key/rotate` to get a fresh unredacted JWT
- **Token retry** — MCP token retrieval retries up to 2 times with 2s delay for fresh n8n starts where the MCP module initialises after the health endpoint

#### n8n Engine Lifecycle
- **Encryption key in OS keychain** — n8n encryption key now stored in macOS Keychain / Windows Credential Manager / Linux Secret Service (`paw-n8n-encryption`), with migration from n8n's plaintext config file and sync back to prevent mismatch errors
- **Space-free data dir** — n8n data moved to `~/.openpawz/n8n-data` (auto-migrated from old `~/Library/Application Support/` path) to fix node-gyp build failures on macOS
- **Process restart safety** — `kill_port()` now uses `-sTCP:LISTEN` to only kill the listening process, not client connections; prefers PID-based kill over port-based
- **App self-kill prevention** — restart logic no longer accidentally kills the Tauri app process when both share port 5678 connections

#### Security Documentation
- Added "Zero Attack Surface by Default" section to security.mdx covering network listeners, cryptographic key storage, soul file protection, and Content Security Policy

### Changed
- `dev:tauri:clean` script now kills stale n8n processes, cleans `~/.openpawz`, and removes all 5 keychain entries for a true clean slate

## [0.1.0] - 2026-02-24

First public pre-release of OpenPawz — a fully local, multi-agent AI desktop app
built with Tauri and Rust.

### Added

#### Core Engine
- Native Rust agent engine with multi-provider support (OpenAI, Anthropic, Google Gemini, Ollama, DeepSeek, xAI/Grok, Mistral, Moonshot, OpenRouter, any OpenAI-compatible endpoint)
- Multi-agent system with unlimited agents, custom personalities, models, and tool policies
- Boss/worker orchestration — agents delegate tasks and spawn sub-agents at runtime
- Inter-agent communication — direct messages, broadcast channels, and agent squads
- Agent squads — team formation with coordinator roles for collaborative tasks
- Per-agent chat sessions with persistent history and mini-chat popups
- Agent dock with 50 custom Pawz Boi avatar sprites
- Smart model routing — boss/worker split, auto-tier (cheap for simple, flagship for complex)
- Tool RAG — intent-based retrieval for on-demand tool discovery
- Streaming responses with real-time display
- Session compaction with configurable context windows (up to 2M tokens)
- Token metering with per-conversation cost tracking and daily budget caps
- Prompt caching support (Anthropic cache_control, Google context caching)
- Thinking/reasoning display for all providers (Claude, Gemini, DeepSeek)
- Configurable model pricing via DB

#### Memory System
- Hybrid BM25 + vector similarity search with Ollama embeddings
- MMR re-ranking for diversity (lambda=0.7)
- Temporal decay with 30-day half-life
- Auto-recall and auto-capture per agent
- Per-agent memory scoping
- Memory Palace visualization UI
- Auto-start Ollama and auto-pull embedding models

#### Security (7 layers)
- Prompt injection scanner — dual TypeScript + Rust detection, 30+ patterns
- Command risk classifier — 30+ danger patterns across 5 risk levels
- Human-in-the-Loop (HIL) approval for side-effect tools
- Per-agent tool policies — allowlist, denylist, or unrestricted mode
- Container sandboxing via Docker (bollard) with CAP_DROP ALL, memory/CPU limits
- Browser network policy — domain allowlist/blocklist
- Credential vault — OS keychain + AES-256-GCM encrypted SQLite
- Path traversal protection on all filesystem tools
- ReDoS protection on allowlist/denylist regex matching
- Security settings stored in encrypted SQLite (not localStorage)

#### Channel Bridges (11 platforms)
- Telegram, Discord, IRC (TLS), Slack, Matrix (E2EE via vodozemac), Mattermost (HTTPS enforced), Nextcloud Talk (HTTPS enforced), Nostr (NIP-04 encrypted DMs, secp256k1), Twitch, WebChat (session-cookie auth, TLS), WhatsApp (via Evolution API + Docker)
- User approval flows, per-agent routing, uniform start/stop/config
- Channel agents get full tool/soul/memory parity with UI chat
- Auto-reconnect configured channels on startup

#### Built-in Tools & Skills
- 35+ skills across 9 categories with instruction injection
- Community skills — browse, install, and manage from skills.sh ecosystem
- Web browsing — search, read, screenshot, interactive browser
- Google Workspace — Gmail, Calendar, Drive, Sheets with OAuth2
- Coinbase CDP wallet integration with Ed25519 JWT signing
- DEX/Uniswap V3 + Solana/Jupiter trading
- TOML skill manifests with auto-loading
- MCP client + dynamic tool registry
- Slash commands system (20 commands + autocomplete)

#### Views & UI
- Today dashboard with tasks, weather, and quick actions
- Tasks hub — Kanban board with agent auto-work and cron scheduling
- Automations — autonomous agent heartbeat with scheduling UI
- Research notebook — agent-powered web research with export
- Mail — IMAP/SMTP with AI actions (summarize, draft, extract tasks)
- Projects — local file browser
- Memory Palace — visualization with map/remember/delete tabs
- Agent editor — tabbed with model selector, skills toggles, custom instructions
- Squads view with live message board
- Settings — atomic tabbed modules (providers, models, engine, security, webhooks)
- Command palette (Cmd+K) — quick agent/view switcher
- Notification center with bell icon + drawer
- Webhook event log
- Activity timeline in Today sidebar
- Voice mode with ElevenLabs TTS

#### Infrastructure
- Tauri v2 desktop app (macOS, Windows, Linux)
- SQLite with AES-256-GCM encryption for all persistent data
- Ed25519 device identity for scoped auth
- VS Code-inspired chat patterns (delete-on-retry, request queue, yield signal)
- Comprehensive error boundary with crash recovery
- Logger with pluggable file transport for crash persistence
- Circuit breakers and retry logic for all providers and bridges

#### Engineering Quality
- CI/CD pipeline: Rust (check + test + clippy -D warnings), TypeScript (tsc + eslint + vitest + prettier), Security (cargo audit + npm audit)
- 327 vitest tests + 77 Rust unit tests + 4 integration test files
- Atomic module architecture — all views decomposed into atoms/molecules/organisms
- Zero ESLint warnings policy
- Typed error handling throughout (EngineError replaces Result<T, String>)

#### Documentation
- Mintlify docs site with guides for all features
- ARCHITECTURE.md — full system design
- SECURITY.md — 7-layer defense-in-depth documentation
- CONTRIBUTING.md — contributor guide
- ENTERPRISE_PLAN.md — hardening audit and roadmap

### Providers (as of 0.1.0)
- **Ollama** — any local model (auto-detected, fully offline)
- **OpenAI** — GPT-4.1, GPT-4.1 mini, GPT-4.1 nano, o3, o4-mini
- **Anthropic** — Claude Opus 4, Sonnet 4, Sonnet 4 Thinking, Haiku 3.5
- **Google Gemini** — Gemini 3.1 Pro, 3 Pro, 3 Flash (Preview), 2.5 Pro/Flash/Flash-Lite
- **OpenRouter** — 100+ models via meta-provider routing
- **DeepSeek** — deepseek-chat, deepseek-reasoner
- **xAI (Grok)** — grok-3, grok-3-mini
- **Mistral** — mistral-large, codestral, pixtral-large
- **Moonshot/Kimi** — moonshot-v1 models
- **Custom** — any OpenAI-compatible endpoint

[Unreleased]: https://github.com/OpenPawz/openpawz/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/OpenPawz/openpawz/releases/tag/v0.1.0
