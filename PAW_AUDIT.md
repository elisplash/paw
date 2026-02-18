# Pawz — Full System Audit

> Last updated: 2026-02-18  
> Codebase: 51,806 LOC (21,895 Rust + 20,375 TypeScript + 9,536 CSS)  
> Architecture: Tauri v2 native desktop app — Rust backend, zero open ports, IPC only

---

## Executive Summary

Pawz is a **standalone AI agent desktop app** built on Tauri v2 with a pure Rust engine. No gateway dependency, no Node.js process, no open ports. Everything runs through Tauri IPC. The app includes 10 channel bridges, 6 AI providers, 37+ skills, multi-agent orchestration, a Coinbase CDP wallet, and a full trading dashboard.

**Total commits**: 100+  
**All P1 and P2 features complete.** P3+ not started.

---

## 1. What's DONE (Shipped & Working)

### 1.1 Core Engine (Rust)

| Feature | Status | Files | Details |
|---------|--------|-------|---------|
| **Native Rust agent engine** | ✅ | `src-tauri/src/engine/` (19,058 LOC) | Full agent loop, streaming SSE, tool execution |
| **70+ Tauri commands** | ✅ | `commands.rs` (2,260 LOC) | Chat, sessions, memory, skills, config, channels, tasks, trading |
| **Tool executor** | ✅ | `tool_executor.rs` (1,807 LOC) | exec, web, file, memory, agent, trading tools |
| **Session management** | ✅ | `sessions.rs` (1,530 LOC) | SQLite-backed, per-agent sessions, rename/delete/clear |
| **Multi-agent orchestrator** | ✅ | `orchestrator.rs` (1,360 LOC) | Boss/worker pattern, delegate_task, check_agent_status, project_complete |
| **Skill vault** | ✅ | `skills.rs` (1,110 LOC) | 37+ skills, encrypted credential injection, enable/disable |
| **AI providers (3 native)** | ✅ | `providers.rs` (1,079 LOC) | OpenAI, Anthropic, Google — all with SSE streaming, multimodal, retry |
| **Semantic memory** | ✅ | `memory.rs` (992 LOC) | Ollama embeddings, auto-recall, auto-capture, BM25+vector hybrid |
| **Gateway fully removed** | ✅ | — | Zero OpenClaw/Node.js dependency since commit a8796e5 |

### 1.2 AI Providers

| Provider | Kind | Base URL | Status |
|----------|------|----------|--------|
| **Ollama** | `ollama` | `localhost:11434` | ✅ Auto-detected on first run |
| **OpenAI** | `openai` | `api.openai.com/v1` | ✅ GPT-4o, o1, o3-mini |
| **Anthropic** | `anthropic` | `api.anthropic.com` | ✅ Claude Sonnet 4, Opus 4, Haiku |
| **Google Gemini** | `google` | `generativelanguage.googleapis.com` | ✅ Gemini 2.5 Pro/Flash, thought handling |
| **OpenRouter** | `openrouter` | `openrouter.ai/api/v1` | ✅ Meta-provider routing |
| **Custom/Compatible** | `custom` | User-provided | ✅ Any OpenAI-compatible endpoint |

**Additional model routing** (via `resolve_provider_for_model` in commands.rs):
- `moonshot`/`kimi` → Moonshot AI provider (OpenAI-compatible)
- `deepseek` → DeepSeek provider
- `grok` → xAI provider  
- `mistral`/`codestral`/`pixtral` → Mistral provider
- Azure AI → auto-detected via `.azure.com` in base URL, adds `api-version` param

**Multi-provider management**: Add/edit/remove/set-default providers. Per-provider cards with API key, model, URL fields. Model routing for boss vs worker agents with per-specialty overrides.

### 1.3 Channel Bridges (10 platforms)

| Channel | File | Lines | Protocol |
|---------|------|-------|----------|
| Telegram | `telegram.rs` | 774 | Bot API, user approval flow |
| Discord | `discord.rs` | 488 | Gateway WebSocket |
| IRC | `irc.rs` | 390 | Standard IRC |
| Slack | `slack.rs` | 374 | Socket Mode |
| Matrix | `matrix.rs` | 425 | Synapse client-server |
| Mattermost | `mattermost.rs` | 377 | WebSocket + REST |
| Nextcloud Talk | `nextcloud.rs` | 409 | Nextcloud Talk API |
| Nostr | `nostr.rs` | 474 | NIP protocol |
| Twitch | `twitch.rs` | 400 | Twitch IRC |
| WebChat | `webchat.rs` | 545 | Embeddable widget |

Each bridge has uniform commands: `start`, `stop`, `status`, `get_config`, `set_config`, `approve_user`, `deny_user`, `remove_user`. Per-agent channel routing with rule-based config.

### 1.4 Agent System

| Feature | Status | Details |
|---------|--------|---------|
| **Agent CRUD** | ✅ | Create, edit, delete. Both local + backend agents |
| **50 Pawz Boi avatars** | ✅ | 96×96 PNGs in `src/assets/avatars/`, numbered 1-50 |
| **Avatar migration** | ✅ | Auto-converts any non-numeric avatar (old sheets, emojis) to random Pawz Boi |
| **All agents editable** | ✅ | Backend/AI-created agents have Edit button + Options menu (commit 41aecd7) |
| **Mini-chat popups** | ✅ | FB Messenger-style per-agent floating chat windows with SSE streaming |
| **Agent dock** | ✅ | Persistent floating tray at bottom-right, avatar circles, unread badges |
| **Agent self-replication** | ✅ | `create_agent` tool — agents can spawn sub-agents from chat |
| **Backend persistence** | ✅ | SQLite `project_agents` table for orchestrator-created agents |
| **Per-agent model selection** | ✅ | Each agent can use a different AI model |
| **Per-agent tool policies** | ✅ | Allow/deny specific tools per agent |
| **Per-agent channel routing** | ✅ | Configure which channels route to which agent |
| **Per-agent sessions** | ✅ | Each agent keeps its own conversation history |
| **Agent personality** | ✅ | Tone (casual/balanced/formal), Initiative, Detail level |
| **Profile update events** | ✅ | Real-time updates via `agent-profile-updated` Tauri events |

### 1.5 Security

| Feature | Status | Details |
|---------|--------|---------|
| **Command risk classifier** | ✅ | 30+ danger patterns, 5 risk levels (critical→safe) |
| **HIL approval modal** | ✅ | Risk-classified display, type "ALLOW" for critical commands |
| **Command allowlist/denylist** | ✅ | ~90+ safe patterns default, regex-based |
| **Auto-deny critical** | ✅ | Privilege escalation, destructive commands auto-blocked |
| **Network exfiltration detection** | ✅ | 10 patterns (curl|cat, scp outbound, /dev/tcp, etc.) |
| **Prompt injection scanner** | ✅ | 30+ patterns, 4 severities, dual TS+Rust implementation |
| **Container sandboxing** | ✅ | Docker via `bollard` crate, cap_drop ALL, memory/CPU limits |
| **OS keychain credentials** | ✅ | macOS Keychain / libsecret integration |
| **AES-256-GCM encryption** | ✅ | Database field encryption via Web Crypto API |
| **Credential audit log** | ✅ | Per-access audit trail with tool name, allow/deny |
| **Session override timer** | ✅ | Timed "allow all" with auto-expiry |
| **Crash watchdog** | ✅ | Auto-restart on engine failure |
| **CSP** | ✅ | Content Security Policy headers |
| **Filesystem sandbox** | ✅ | Tauri capabilities + per-project scope |
| **npm risk scoring** | ✅ | Risk assessment for npm-related operations |

### 1.6 Memory System

| Feature | Status | Details |
|---------|--------|---------|
| **Long-term memory** | ✅ | SQLite-backed with Ollama embeddings |
| **BM25 full-text search** | ✅ | SQLite FTS5 virtual table |
| **Vector similarity search** | ✅ | Cosine similarity on Ollama embeddings |
| **Hybrid BM25+vector** | ✅ | Weighted merge of both search methods |
| **MMR re-ranking** | ✅ | Jaccard-based diversity, lambda=0.7 |
| **Temporal decay** | ✅ | Exponential decay with 30-day half-life |
| **Per-agent memory scope** | ✅ | `agent_id` column, filtered search |
| **Auto-recall** | ✅ | Relevant memories injected into context |
| **Auto-capture** | ✅ | Key facts extracted and stored automatically |
| **Memory Palace UI** | ✅ | Visual memory browser with search, stats, management |
| **Memory export** | ✅ | Export memories to file |
| **Session compaction** | ✅ | AI-powered summarization when context fills |

### 1.7 Frontend Views (20+ views, 10,863 LOC TypeScript)

| View | Lines | Purpose |
|------|-------|---------|
| `agents.ts` | 1,201 | Agent CRUD, avatars, mini-chat, dock |
| `mail.ts` | 985 | Full email client (IMAP/SMTP via Himalaya) |
| `projects.ts` | 956 | Build/Research/Create project workspaces |
| `memory-palace.ts` | 864 | Memory visualization + management |
| `skills.ts` | 720 | Skill vault — install, configure, credential injection |
| `research.ts` | 710 | Research workflow with findings + synthesis |
| `settings.ts` | 647 | Settings shell/tabs container |
| `settings-models.ts` | 626 | Provider & model management |
| `tasks.ts` | 555 | Kanban board with 6 columns, drag-and-drop, cron scheduling |
| `today.ts` | 518 | Daily dashboard / summary |
| `orchestrator.ts` | 498 | Multi-agent project orchestration UI |
| `automations.ts` | 393 | Cron-based automation jobs |
| `settings-advanced.ts` | 376 | Advanced security & debug settings |
| `foundry.ts` | 229 | Content creation workspace |
| `nodes.ts` | 190 | Node management UI |
| `trading.ts` | 249 | Crypto trading dashboard |

### 1.8 Feature Modules (Atomic Design)

| Module | Lines | Status |
|--------|-------|--------|
| `slash-commands/` | 666 | ✅ 20 commands with autocomplete |
| `container-sandbox/` | 375 | ✅ Docker sandbox config |
| `prompt-injection/` | 374 | ✅ Injection detection (30+ patterns) |
| `memory-intelligence/` | 354 | ✅ Smart memory ops |
| `agent-policies/` | 346 | ✅ Per-agent tool policies |
| `channel-routing/` | 309 | ✅ Rule-based channel routing |
| `session-compaction/` | 181 | ✅ AI summarization |
| `browser-sandbox/` | 0 | ❌ Empty directory — not implemented |

### 1.9 Database (SQLite, 11 tables)

| Table | Purpose |
|-------|---------|
| `agent_modes` | Agent mode presets (General, Code Review, Quick Chat) |
| `projects` | Build/Research/Create projects |
| `project_files` | Files within projects |
| `project_agents` | Backend-created agents (orchestrator) |
| `automation_runs` | Cron execution log |
| `research_findings` | Research discoveries |
| `content_documents` | Content creation docs |
| `email_accounts` | IMAP/SMTP config |
| `emails` | Messages + AI drafts |
| `credential_activity_log` | Credential access audit trail |
| `security_audit_log` | Security event log |
| `security_rules` | User-defined allow/deny patterns |

### 1.10 Unique Features (No OpenClaw Equivalent)

| Feature | Details |
|---------|---------|
| **Coinbase CDP Agentic Wallet** | JWT auth (ES256), prices, balances, trade, transfer, wallet creation |
| **Trading dashboard** | Live crypto trading with auto-trade guidelines, position tracking, portfolio view |
| **Kanban task board** | 6 columns, drag-and-drop, multi-agent parallel execution, cron scheduling |
| **Research view** | Project → streaming research → findings → synthesis report |
| **Email integration** | Full IMAP/SMTP via Himalaya, provider picker, OS keychain credentials |
| **Memory Palace UI** | Visual memory browser unique to Pawz |
| **Agent self-replication** | `create_agent` tool — agents spawn sub-agents from chat |
| **Boss/worker orchestration** | `delegate_task`, `check_agent_status`, `send_agent_message`, `project_complete` |
| **Mini-chat popups** | FB Messenger-style per-agent floating windows |
| **50 custom avatars** | Pawz Boi character art (96×96 PNGs) |
| **Provider model confirmation** | API response confirms actual model used (✓ label in UI) |

### 1.11 UI/Theme

| Element | Current State |
|---------|---------------|
| **Colors** | VS Code neutral darks (`#1e1e1e` base, `#252526` surface, `#2d2d2d` elevated) |
| **Accent** | Neon magenta `#ff00ff` with hover/subtle/muted variants |
| **Fonts** | System font stack for body, Press Start 2P for headers, Share Tech Mono for code |
| **Icons** | Google Material Symbols Outlined — self-hosted woff2, `.ms` class |
| **Avatars** | 50 Pawz Boi 96×96 PNGs, auto-migration from legacy emoji/sheets |
| **Grid/scanlines** | Removed — clean flat design |
| **Theme toggle** | Light/dark mode support |

---

## 2. What's NOT Done (Remaining Work)

### 2.1 P3 — Voice & TTS (NOT STARTED)

| Feature | Effort | Status | Details |
|---------|--------|--------|---------|
| **OpenAI TTS** | M | ❌ | 🔊 button on messages, OpenAI voices |
| **ElevenLabs TTS** | M | ❌ | Premium voice synthesis |
| **Talk Mode** | L | ❌ | Continuous voice conversation |
| **Voice Wake** | M | ❌ | Wake word detection |
| **Morning Brief** | S | ❌ | One-click cron template for daily summary |

`settings-voice.ts` is a 15-line stub. Gateway methods (`tts.*`, `talk.*`, `voicewake.*`) are typed but have zero UI.

### 2.2 P4 — Browser & Sandbox (NOT STARTED)

| Feature | Effort | Status | Details |
|---------|--------|--------|---------|
| **Managed browser profiles** | M | ❌ | Persistent Chrome state, profile picker |
| **Screenshot viewer** | S | ❌ | Display agent browser screenshots in chat |
| **Per-agent workspaces** | M | ❌ | Isolated filesystem scope per agent |
| **Outbound domain allowlist** | M | ❌ | Network-level sandboxing |

`src/features/browser-sandbox/` exists as an empty directory.

### 2.3 P5 — Ecosystem & Platform (NOT STARTED)

| Feature | Effort | Status | Details |
|---------|--------|--------|---------|
| **PawHub skill registry** | XL | ❌ | Public discover/install/publish marketplace |
| **Canvas / visual workspace** | XL | ❌ | Agent-driven visual canvas |
| **Mobile companion app** | XL | ❌ | iOS/Android with camera, location, SMS |
| **Remote access** | M | ❌ | Tailscale Serve/Funnel |
| **OpenResponses API** | M | ❌ | `/v1/responses` HTTP endpoint |

### 2.4 Missing Channels

| Channel | OpenClaw Has | Pawz Status | Priority |
|---------|:------------:|:-----------:|----------|
| WhatsApp | ✅ | ❌ | P1.5 — CRITICAL (dedicated phase) |
| iMessage | ✅ | ❌ | P5 |
| Google Chat | ✅ | ❌ | P5 |
| Signal | ✅ | ❌ | P5 |
| Microsoft Teams | ✅ | ❌ | P5 |
| Zalo | ✅ | ❌ | P5 |

### 2.5 Provider Gaps

| Provider | Routing | UI Kind | Status |
|----------|:-------:|:-------:|--------|
| Kimi / Moonshot | ✅ | ❌ (uses `custom`) | Works via custom provider + model prefix routing |
| DeepSeek | ✅ | ❌ (uses `custom`) | Same — routing exists, no dedicated UI entry |
| xAI (Grok) | ✅ | ❌ (uses `custom`) | Same |
| Mistral | ✅ | ❌ (uses `custom`) | Same |
| AWS Bedrock | ❌ | ❌ | Not implemented |
| GitHub Copilot | ❌ | ❌ | Not implemented |

### 2.6 Minor Gaps & Polish

| Item | Status | Details |
|------|--------|---------|
| **Nodes view** | 🔶 Minimal | 190 lines — basic list/invoke/pair UI, no mobile app to pair with |
| **Trading depth** | 🔶 | 249-line view — needs more position tracking, charts |
| **Skill discovery** | ❌ | 37 hardcoded skills, no marketplace or install-from-URL |
| **Per-agent workspaces** | ❌ | No isolated filesystem per agent |
| **Session isolation (dmScope)** | ❌ | No per-channel-per-peer session isolation |

---

## 3. Competitive Position vs OpenClaw

### Where Pawz WINS

| Area | Pawz Advantage |
|------|----------------|
| **Architecture** | Zero open ports, no auth tokens, native IPC — no network attack surface |
| **Security depth** | 30+ pattern classifier, type-to-confirm, npm scoring, AES-256 encryption, audit logs |
| **Credential security** | OS keychain + encrypted vault + audit trail (vs config file env vars) |
| **Channel count** | +3 unique: IRC, Twitch, Nextcloud Talk |
| **Multi-agent** | Boss/worker orchestration, self-replication, parallel task execution, mini-chat |
| **Trading** | Full Coinbase CDP wallet + trading dashboard (unique feature) |
| **Email** | Full IMAP/SMTP with AI drafts (unique feature) |
| **Tasks** | Kanban board with agent assignment (unique feature) |
| **Research** | Dedicated research workflow (unique feature) |
| **Memory UI** | Memory Palace visualization (unique feature) |
| **Tool approval UX** | Risk-classified modal with visual severity indicators |
| **Desktop UX** | Native app, no terminal needed, one-click everything |

### Where OpenClaw WINS

| Area | OpenClaw Advantage |
|------|-------------------|
| **WhatsApp** | Full support via Baileys (world's #1 messaging platform) |
| **Voice/TTS** | ElevenLabs, OpenAI, Edge TTS, Talk Mode, Voice Wake |
| **Browser automation** | CDP + Playwright + Chrome extension relay + managed profiles |
| **Skill marketplace** | ClawHub registry for discovery/install/publish |
| **Mobile nodes** | iOS/Android companion apps with camera, screen, location |
| **Remote access** | Tailscale Serve/Funnel |
| **Canvas** | Agent-driven visual workspace |
| **Channel count** | +6 channels: WhatsApp, iMessage, Google Chat, Signal, Teams, Zalo |
| **Per-agent workspaces** | Isolated filesystem per agent |

### Score Summary

| Category | Pawz | OpenClaw | Winner |
|----------|:----:|:--------:|:------:|
| Channels | 10 | 16 | OpenClaw |
| AI Providers | 6 (+4 routed) | 7+ | Tie |
| Security | 17 features | 7 features | **Pawz** |
| Memory | 12 features | 8 features | **Pawz** |
| Multi-Agent | 11 features | 6 features | **Pawz** |
| Voice/TTS | 0 | 5 features | OpenClaw |
| Browser | Basic | Advanced | OpenClaw |
| Unique Features | 11 | 3 | **Pawz** |
| Desktop UX | Native app | CLI + web | **Pawz** |

---

## 4. Codebase Statistics

| Metric | Count |
|--------|-------|
| **Total LOC** | 51,806 |
| **Rust engine** | 21,895 LOC (15 modules) |
| **TypeScript frontend** | 20,375 LOC (30+ files) |
| **CSS** | 9,536 LOC |
| **HTML** | 1,978 LOC |
| **Tauri commands** | 70+ |
| **Channel bridges** | 10 |
| **AI providers** | 6 (+ 4 model-prefix routed) |
| **Built-in skills** | 37+ |
| **Avatar assets** | 50 PNGs (96×96) |
| **SQLite tables** | 11+ |
| **Feature modules** | 7 implemented, 1 empty |
| **Frontend views** | 20+ |
| **Total commits** | 100+ |

---

## 5. Full Commit History (Recent → Oldest)

| Commit | Feature |
|--------|---------|
| ee4eac2 | Fix Azure AI Anthropic endpoint (api-version param) |
| 41aecd7 | Make AI-created agents editable |
| 52f7bc9 | Fix ALL non-numeric avatars + memory palace flex-direction |
| 801e2d8 | Migrate old sheet avatars, remove research bg, fix memory layout |
| 0774d37 | Replace pixel sprites with 50 Pawz Boi 96×96 avatars |
| 559209a | Unique sprites for backend agents, black dock backgrounds |
| 2c093b9 | Fix Azure AI Services support |
| 9c1064c | Brighten home card icons (.ms), offset mini-chats past dock |
| 299a459 | Multi-provider settings UI: add/edit/remove/set-default |
| 7a03f3d | Remove grid/scanline overlays, sprite characters |
| f215fa2 | VS Code system font stack |
| 4b62b86 | VS Code neutral dark colors (#1e1e1e) |
| 850a575 | OpenAI-compatible provider presets (Kimi, DeepSeek, xAI, Mistral) |
| 58d75e2 | Autonomous agent heartbeat + automations UI |
| 0457820 | Token meter shows actual context usage |
| 5f96058–dccdc2e | Coinbase v3 API migration (7 commits) |
| 28c8835 | Rename Dave → Pawz |
| 6aee0c1 | Global agent dock tray + mini-chat enhancements |
| 63e2de7 | Per-agent chat sessions |
| 7fed27f | Agent switching fix + agent dropdown |
| e6c854f | Trading dashboard + auto-trade guidelines |
| eb20c35 | Coinbase CDP Agentic Wallet integration |
| 607ac7c | Backend agents in UI + mini-chat popups |
| df82662 | P4 Browser & Sandbox (managed profiles, screenshot viewer) |
| 66ae16f | **P2: Memory & Intelligence** — BM25, temporal decay, MMR, per-agent scope |
| 2c45553 | **P1: Container sandboxing** via Docker (bollard) |
| 110d860 | **P1: Per-agent channel routing** |
| fd06206 | **P1: Per-agent tool policies** |
| 31cfe8d | **P1: Session compaction** |
| 90e2d20 | **P1: Slash commands** (20 commands + autocomplete) |
| d1eb0f6 | **P1: Prompt injection scanner** |
| a8796e5 | Remove gateway.ts — 100% self-contained Rust engine |
| 60b44e6 | Consolidate agent/model systems, remove Foundry Agents tab |
| 680f2e8 | Remove exec from safe_tools, add SQL danger patterns |
| 1128e3b | Channel bridges — 9 platforms |
| d1fa02f | Multi-agent tasks — parallel execution |
| a4fb592 | Tasks Hub — Kanban board, agent auto-work, cron scheduling |
| e7462ec | 35 skills across 9 categories |
| ca5a847 | Rebrand to Pawz — engine-only mode |
| 3051a2e | Phase 3: Soul + Memory — agent files, Ollama embeddings |
| d277c54 | Phase 2: HIL security, session management, token metering |
| 9be0daa | Phase 1: Native Rust agent engine |

---

## 6. Priority Roadmap (What's Next)

### Immediate (P1.5)
1. **WhatsApp channel** — World's #1 messaging platform, massive user demand

### Near-term (P3)
2. **OpenAI TTS** — 🔊 button on messages
3. **ElevenLabs TTS** — Premium voice
4. **Talk Mode** — Continuous voice conversation
5. **Morning Brief template** — One-click cron

### Medium-term (P4)
6. **Managed browser profiles** — Persistent Chrome state
7. **Screenshot viewer** — Display agent screenshots in chat
8. **Per-agent workspaces** — Isolated filesystem scope
9. **Outbound domain allowlist** — Network sandboxing

### Long-term (P5)
10. **PawHub skill marketplace**
11. **Canvas / visual workspace**
12. **Mobile companion app**
13. **Additional channels** (iMessage, Signal, Google Chat, Teams)
14. **Remote access** (Tailscale)
15. **First-class provider UI** for Kimi, DeepSeek, xAI, Mistral

---

**TL;DR**: All P1 security features DONE. All P2 memory intelligence DONE. Gateway fully removed. 10 channel bridges, 6+ providers, 37+ skills, 50 custom avatars, Coinbase trading, multi-agent orchestration — all shipped. Next up: WhatsApp (P1.5), then Voice/TTS (P3).
