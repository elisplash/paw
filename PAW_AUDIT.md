# Pawz vs OpenClaw — Full Competitive Audit

> Last updated: 2025-02-17 (P1 complete, P2 complete)
> Source: OpenClaw docs (docs.openclaw.ai) + full Pawz codebase audit

---

## Executive Summary

Pawz and OpenClaw solve the same problem (personal AI agent gateway) with fundamentally different architectures. OpenClaw is a **Node.js gateway process** (server-side, CLI-first). Pawz is a **Tauri v2 native desktop app** (Rust backend, zero open ports). This architectural difference is Pawz's strongest competitive advantage — and also its biggest constraint.

**Bottom line**: Pawz already surpasses OpenClaw in several areas (channel count, security depth, native desktop UX, tool breadth). But there are ~15 critical gaps that must be closed for feature parity, and ~10 areas where Pawz can leapfrog OpenClaw entirely.

---

## 1. Feature Parity Matrix

### Where Pawz WINS (advantages over OpenClaw)

| Feature | Pawz | OpenClaw | Pawz Advantage |
|---------|------|----------|----------------|
| **Channel count** | 10 (Discord, Telegram, Slack, Matrix, IRC, Nostr, Twitch, Mattermost, Nextcloud, WebChat) | 7 core (WhatsApp, Telegram, Discord, Slack, iMessage, Mattermost, Matrix) | **+3 channels**: IRC, Nostr, Twitch, Nextcloud |
| **Security depth** | 30+ danger pattern classifier, risk-tiered approval modal, command allowlist/denylist, network exfiltration detection, npm risk scoring, AES-256-GCM encryption at rest, crash watchdog, per-project filesystem scope, sensitive path blocking | `openclaw security audit` CLI, per-agent tool policies, Docker sandbox, prompt injection guidance | **Far deeper security UX** — OpenClaw has more infra-level sandboxing, but Pawz has richer user-facing security controls |
| **Architecture** | Native Rust backend, zero open ports, zero auth tokens, direct IPC | Node.js process listening on WebSocket with auth token | **No network attack surface** — IPC only |
| **Tool approval UX** | Risk-classified modal (red DANGER for critical, type "ALLOW" for sudo), auto-deny rules, session overrides with timer, unified audit log with export | Simple approve/deny with askMode/askFallback | **Much richer HIL experience** |
| **Credential management** | OS keychain (macOS Keychain / libsecret) + encrypted vault + audit trail + per-account permission toggles | Config file with env vars | **Production-grade credential security** |
| **Skill ecosystem** | 37 skills across 9 categories, encrypted credential vault, instruction injection with live credentials, custom instruction editor, binary/env detection | AgentSkills-compatible, ClawHub registry for discovery/install | **More built-in skills**, but OpenClaw has discovery advantage |
| **Multi-agent orchestration** | Boss/worker pattern with `delegate_task`, `check_agent_status`, `send_agent_message`, `project_complete` tools; per-agent model routing; confirmed_model from API response | Bindings-based routing (channel→agent), per-agent workspaces/sessions/auth | **Richer orchestration primitives** — OpenClaw has better routing |
| **Tasks system** | Kanban board with 6 columns, drag-and-drop, multi-agent parallel execution, cron scheduling, live activity feed, agent role management (lead/collaborator) | No equivalent | **Unique feature** — OpenClaw has nothing like this |
| **Research view** | Full research workflow: project → streaming research → findings → synthesis report | No equivalent | **Unique feature** |
| **Email integration** | Full IMAP/SMTP via Himalaya, provider picker (Gmail/Outlook/Yahoo/iCloud/Fastmail), OS keychain credentials, per-account agent permissions, audit log | No built-in email | **Unique feature** |
| **Provider model confirmation** | API response confirms actual model used (✓ label in UI) | No equivalent | **Unique feature** |

### Where OpenClaw WINS (gaps Pawz must close)

| Feature | OpenClaw | Pawz Status | Priority |
|---------|----------|-------------|----------|
| **WhatsApp channel** | Full via Baileys library (QR login, media, groups) | Missing entirely | P1 — CRITICAL (separate phase) |
| **Docker sandboxing** | Modes (off/non-main/all), scope (session/agent/shared), workspace bind mounts, custom images | ✅ bollard crate, ephemeral containers, cap_drop ALL, memory/CPU limits | P1 — DONE |
| **Slash commands** | 25+ commands: `/model`, `/think`, `/verbose`, `/reasoning`, `/compact`, `/exec`, `/mesh`, `/web`, `/img`, `/tts`, `/remember`, `/forget` | ✅ 20 commands with autocomplete | P1 — DONE |
| **Session compaction** | Auto-compaction when context fills, `/compact` command, pre-compaction memory flush, pruning | ✅ AI-powered compaction with threshold detection | P1 — DONE |
| **Memory: Hybrid search** | BM25 + vector search, MMR re-ranking, temporal decay, conversation-aware filtering | Ollama embeddings + cosine similarity only, keyword fallback | P2 — HIGH |
| **TTS / Voice** | ElevenLabs, OpenAI, Edge TTS on all channels; Talk Mode (continuous voice conversation); Voice Wake (wake words) | Gateway methods typed, zero UI built | P2 — HIGH |
| **ClawHub skill registry** | Public discovery/install/publish marketplace | No skill discovery — 37 hardcoded skills | P2 — MEDIUM |
| **Browser: Managed profiles** | CDP, Playwright, Chrome extension relay, profile management, snapshot/actions system | Basic headless_chrome crate, no profiles, no Playwright, no extension relay | P2 — MEDIUM |
| **Session isolation (dmScope)** | Per-channel-peer isolation, identity links, privacy controls | All sessions share same context | P2 — MEDIUM |
| **Mobile nodes** | iOS/Android companion apps with Canvas, camera, screen recording, location, SMS, system.run | Node management UI (list/invoke/pair) but no mobile app | P3 — FUTURE |
| **Tailscale exposure** | Serve/Funnel for remote access from outside local network | Desktop-only, no remote access | P3 — FUTURE |
| **OpenResponses API** | `/v1/responses` HTTP endpoint for external tool integration | No HTTP API surface | P3 — FUTURE |
| **iMessage channel** | Via BlueBubbles or legacy macOS bridge | Missing | P3 — FUTURE |
| **Google Chat channel** | Full integration | Missing | P3 — FUTURE |
| **Signal channel** | Via signal-cli | Missing | P3 — FUTURE |
| **Canvas / A2UI** | Agent-driven visual workspace | No equivalent | P3 — FUTURE |

---

## 2. Deep-Dive Gap Analysis

### 2.1 CRITICAL: Docker/Container Sandboxing

OpenClaw's sandboxing is its strongest security differentiator:
- **Modes**: `off`, `non-main` (sandbox non-default agents), `all` (sandbox everything)
- **Scope**: `session` (per-conversation container), `agent` (per-agent container), `shared` (single container)
- **Workspace access**: Mount agent workspace into container
- **Custom images**: Use `docker.image` config for bespoke environments

**Pawz gap**: The agent runs `sh -c "{command}"` directly on the host OS via `tool_executor.rs`. The only protection is the HIL approval modal. A user who clicks "Allow" on a destructive command has no safety net.

**Recommendation**: Implement container sandboxing in Rust via `bollard` (Docker API client crate). Offer 3 modes:
1. **Off** — current behavior, direct execution
2. **Sandboxed** — all `exec` calls run inside a Docker container with mounted workspace
3. **Strict** — container with read-only filesystem except designated workspace paths

### 2.2 CRITICAL: WhatsApp Channel (SEPARATE PHASE)

WhatsApp is the world's most popular messaging platform. OpenClaw supports it via the Baileys library. Pawz has zero WhatsApp integration. This is its own dedicated phase due to complexity.

### 2.3 HIGH: Slash Commands

OpenClaw has a rich slash command system: `/model`, `/think`, `/verbose`, `/compact`, `/remember`, `/forget`, `/web`, `/img`, `/exec`, `/mesh`.

**Pawz gap**: Zero slash command support.

**Recommendation**: Implement a slash command parser in the chat input — purely frontend work.

### 2.4 HIGH: Session Compaction

OpenClaw automatically compacts conversation history when context fills up with pre-compaction memory flush, configurable thresholds, and pruning strategies.

**Pawz gap**: Only has session clear. No selective compaction, no pre-trim memory save.

**Recommendation**: Implement compaction in the agent loop — check token count, summarize older messages, flush key facts to memory.

### 2.5 HIGH: Memory System Improvements

| Feature | OpenClaw | Pawz |
|---------|----------|------|
| Hybrid search | BM25 + vector with MMR re-ranking | ✅ BM25 FTS5 + vector, weighted merge, MMR |
| Temporal decay | Yes — recent memories weighted higher | ✅ Exponential decay (30-day half-life) |
| Conversation-aware | Filters by session/agent | ✅ Per-agent memory scope |

### 2.6 HIGH: Text-to-Speech

TTS enables OpenClaw's most popular use case: morning briefs. Pawz has zero TTS UI.

### 2.7 MEDIUM: Browser Automation Depth

OpenClaw has managed profiles, Playwright, Chrome Extension relay. Pawz has basic headless_chrome only.

---

## 3. Feature Comparison by Category

### 3.1 Channels

| Channel | Pawz | OpenClaw |
|---------|:----:|:--------:|
| Discord | ✅ | ✅ |
| Telegram | ✅ | ✅ |
| Slack | ✅ | ✅ |
| Matrix | ✅ | ✅ |
| Mattermost | ✅ | ✅ (plugin) |
| IRC | ✅ | ❌ |
| Nostr | ✅ | ✅ |
| Twitch | ✅ | ❌ |
| Nextcloud Talk | ✅ | ❌ |
| WebChat | ✅ | ✅ |
| WhatsApp | ❌ | ✅ |
| iMessage | ❌ | ✅ |
| Google Chat | ❌ | ✅ |
| Signal | ❌ | ✅ |
| Microsoft Teams | ❌ | ✅ |
| Zalo | ❌ | ✅ |

**Score**: Pawz 10 / OpenClaw 13

### 3.2 AI Providers

| Provider | Pawz | OpenClaw |
|----------|:----:|:--------:|
| OpenAI | ✅ | ✅ |
| Anthropic | ✅ | ✅ |
| Google Gemini | ✅ | ✅ |
| OpenRouter | ✅ | ✅ |
| Ollama | ✅ | ✅ |
| AWS Bedrock | ❌ | ✅ |
| GitHub Copilot | ❌ | ✅ |
| Custom OpenAI-compat | ✅ | ✅ |

### 3.3 Security

| Feature | Pawz | OpenClaw |
|---------|:----:|:--------:|
| Tool approval (HIL) | ✅ (risk-classified) | ✅ (basic) |
| Command risk classification | ✅ (30+ patterns, 5 levels) | ❌ |
| Type-to-confirm for critical | ✅ | ❌ |
| Command allowlist/denylist | ✅ | ✅ |
| Auto-deny privilege escalation | ✅ | ❌ |
| Network exfiltration detection | ✅ | ❌ |
| Npm package risk scoring | ✅ | ❌ |
| Docker sandboxing | ✅ | ✅ |
| Per-agent tool policies | ✅ | ✅ |
| Per-agent access profiles | ✅ | ✅ |
| Prompt injection defense | ✅ | ✅ (docs) |
| OS keychain credentials | ✅ | ❌ |
| AES-256-GCM encryption at rest | ✅ | ❌ |
| Credential activity audit log | ✅ | ❌ |
| CSP (Content Security Policy) | ✅ | N/A |
| Filesystem sandbox | ✅ (Tauri + per-project) | ✅ (Docker) |
| Session override timer | ✅ | ❌ |
| Crash watchdog + auto-restart | ✅ | ❌ |

### 3.4 Memory & Knowledge

| Feature | Pawz | OpenClaw |
|---------|:----:|:--------:|
| Long-term memory store | ✅ (SQLite) | ✅ (Markdown/QMD) |
| Vector search | ✅ (Ollama embeddings) | ✅ |
| BM25 text search | ✅ | ✅ |
| Hybrid BM25+vector | ✅ | ✅ |
| MMR re-ranking | ✅ | ✅ |
| Temporal decay | ✅ | ✅ |
| Auto-recall | ✅ | ✅ |
| Auto-capture | ✅ | ✅ |
| Memory export | ✅ | ❌ |
| Memory Palace UI | ✅ | ❌ |

### 3.5 Multi-Agent

| Feature | Pawz | OpenClaw |
|---------|:----:|:--------:|
| Multi-agent CRUD | ✅ | ✅ |
| Boss/worker orchestration | ✅ | ❌ |
| Channel→agent routing | ✅ | ✅ |
| Per-agent workspaces | ❌ | ✅ |
| Per-agent model selection | ✅ | ✅ |
| Per-agent tool policies | ✅ | ✅ |
| Kanban task board | ✅ | ❌ |
| Multi-agent parallel execution | ✅ | ❌ |

---

## 4. Missing Features Inventory

### Must Have (P1) — Feature Parity Gaps

1. **Docker/Container Sandboxing** — The single biggest security gap
2. **Per-Agent Tool Policies** — Allow/deny specific tools per agent
3. **Prompt Injection Detection** — Scan incoming messages for injection attempts
4. **Slash Commands** — In-conversation agent control
5. **Session Compaction** — Auto-summarize when context fills
6. **Per-Agent Channel Routing** — Configure which channels route to which agent
7. **WhatsApp Channel** — SEPARATE PHASE due to complexity

### Should Have (P2) — Competitive Advantage

8. **BM25 + Hybrid Memory Search** — SQLite FTS5
9. **TTS (Text-to-Speech)** — OpenAI/ElevenLabs
10. **Memory Temporal Decay + MMR** — Better recall
11. **Managed Browser Profiles** — Persistent browser state
12. **Per-Agent Workspaces** — Isolated filesystem scope per agent
13. **Session Isolation (dmScope)** — Per-channel-per-peer isolation
14. **Outbound Domain Allowlist** — Network sandboxing

### Nice to Have (P3) — Leapfrog

15. **Skill Marketplace (PawHub)**
16. **Canvas / Visual Workspace**
17. **Mobile Companion App**
18. **Talk Mode / Voice Wake**
19. **Remote Access**
20. **Additional Channels** (iMessage, Signal, Google Chat, Teams)

---

## 5. Prioritized Roadmap

### Phase 1: Security & UX Parity (P1)

| # | Feature | Effort | Status |
|---|---------|--------|--------|
| 1 | **Container Sandboxing** — `bollard` crate, ephemeral Docker containers, cap_drop ALL, memory/CPU limits | L | ✅ DONE (2c45553) |
| 2 | **Per-Agent Tool Policies** — allowlist/denylist/presets per agent | M | ✅ DONE (fd06206) |
| 3 | **Prompt Injection Scanner** — 30+ patterns, 4 severities, dual TS+Rust impl | S | ✅ DONE (d1eb0f6) |
| 4 | **Slash Commands** — 20 commands, autocomplete, session overrides | M | ✅ DONE (90e2d20) |
| 5 | **Session Compaction** — AI-powered summarization, threshold detection | M | ✅ DONE (31cfe8d) |
| 6 | **Per-Agent Channel Routing** — rule-based routing with UI config | M | ✅ DONE (110d860) |

### Phase 1.5: WhatsApp (dedicated phase)

| # | Feature | Effort | Status |
|---|---------|--------|--------|
| 7 | **WhatsApp Channel** — Baileys-compatible or bridge approach | XL | ❌ NOT STARTED |

### Phase 2: Memory & Intelligence

| # | Feature | Effort | Status |
|---|---------|--------|--------|
| 8 | **BM25 Full-Text Search** — SQLite FTS5 virtual table, BM25 scoring | S | ✅ DONE (66ae16f) |
| 9 | **Temporal Decay** — exponential decay with 30-day half-life | S | ✅ DONE (66ae16f) |
| 10 | **MMR Re-ranking** — Jaccard-based diversity, lambda=0.7 | S | ✅ DONE (66ae16f) |
| 11 | **Per-Agent Memory Scope** — agent_id column, filtered search | M | ✅ DONE (66ae16f) |

### Phase 3: Voice & TTS

| # | Feature | Effort | Status |
|---|---------|--------|--------|
| 12 | **OpenAI TTS** — 🔊 button on messages | M | ❌ NOT STARTED |
| 13 | **ElevenLabs TTS** — premium voice | M | ❌ NOT STARTED |
| 14 | **Talk Mode** — continuous voice | L | ❌ NOT STARTED |
| 15 | **Morning Brief Template** — one-click cron | S | ❌ NOT STARTED |

### Phase 4: Browser & Sandbox

| # | Feature | Effort | Status |
|---|---------|--------|--------|
| 16 | **Managed Browser Profiles** — persistent Chrome state | M | ❌ NOT STARTED |
| 17 | **Screenshot Viewer** — display agent screenshots | S | ❌ NOT STARTED |
| 18 | **Per-Agent Workspaces** — isolated filesystem | M | ❌ NOT STARTED |
| 19 | **Outbound Domain Allowlist** — network security | M | ❌ NOT STARTED |

### Phase 5: Ecosystem & Platform

| # | Feature | Effort | Status |
|---|---------|--------|--------|
| 20 | **PawHub Skill Registry** | XL | ❌ NOT STARTED |
| 21 | **Additional Channels** | M each | ❌ NOT STARTED |
| 22 | **Canvas / Visual Workspace** | XL | ❌ NOT STARTED |
| 23 | **Mobile Companion** | XL | ❌ NOT STARTED |

---

## 6. Codebase Statistics

| Metric | Pawz | OpenClaw |
|--------|------|----------|
| Backend language | Rust | Node.js/TypeScript |
| Backend LOC | ~18,206 (20 engine files) | Unknown |
| Frontend LOC | ~20,045 (30+ TS files) | Vite + Lit SPA |
| Total LOC | ~38,251 | Unknown |
| Channels | 10 | 13+ |
| Built-in tools | 26+ | ~20+ |
| Built-in skills | 37 | Variable |
| AI providers | 6 | 7+ |
| Database | SQLite (11 tables, WAL) | SQLite + Markdown |

---

## 7. Architecture Note: Atomic Refactor

Starting with P1 features, all new code follows **atomic design** principles:
- **Atoms**: Pure functions, single-responsibility utilities (parsers, validators, formatters)
- **Molecules**: Composed atoms that form a single feature unit (slash command parser + executor)
- **Organisms**: Full feature modules that combine molecules (sandboxing system, compaction system)
- **No new code in main.ts** — each feature gets its own module space

Frontend: `src/features/{feature}/` with atoms.ts, molecules.ts, index.ts
Backend: `src-tauri/src/engine/{feature}/` or `src-tauri/src/engine/{feature}.rs`

---

**TL;DR**: ~~Close the 6 P1 gaps~~ **All 6 P1 features are DONE. All 4 P2 Memory & Intelligence features are DONE.** WhatsApp remains as Phase 1.5. Next up: Phase 3 (Voice & TTS).
