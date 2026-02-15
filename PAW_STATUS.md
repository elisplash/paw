# Paw — Full Architecture, Status & Wiring Plan

> Last updated: 2026-02-15 (Sprint 1 in progress — token meter + compaction warnings + memory export built)  
> Cross-referenced against: [github.com/openclaw/openclaw](https://github.com/openclaw/openclaw) main branch

---

## What Paw Is

Paw is a **Tauri desktop app** (Rust + TypeScript + Vite) that wraps the [OpenClaw](https://github.com/openclaw/openclaw) AI agent gateway. It gives non-technical users a visual interface to run AI agents — no terminal, no config files, no localhost ports.

**Target user**: Someone who wants AI agents but will never open a terminal.

**Business model**: One-time purchase (bring your own API keys) + optional subscription (managed keys).

### What OpenClaw Is (upstream)

OpenClaw is a local-first personal AI assistant framework with:
- **Multi-channel inbox**: WhatsApp, Telegram, Slack, Discord, Google Chat, Signal, BlueBubbles (iMessage), iMessage (legacy), Microsoft Teams, Matrix, Zalo, WebChat, macOS, iOS/Android
- **Multi-agent routing**: isolated sessions per agent, workspace, or sender
- **Voice Wake + Talk Mode**: always-on speech with ElevenLabs (macOS/iOS/Android)
- **TTS**: ElevenLabs, OpenAI, Edge text-to-speech on all channels
- **Browser control**: CDP-managed Chrome/Chromium automation
- **Canvas + A2UI**: agent-driven visual workspace
- **Nodes**: iOS/Android nodes with camera, screen, location, voice capabilities
- **Device pairing**: secure pairing flow for mobile nodes
- **Exec approvals**: human-in-the-loop tool approval system
- **Webhooks**: external trigger endpoints (`/hooks/wake`, `/hooks/agent`)
- **OpenAI HTTP API**: Chat Completions endpoint
- **OpenResponses HTTP API**: `/v1/responses` endpoint
- **Plugin system**: channel extensions, voice-call (Twilio/Telnyx/Plivo), talk-voice, etc.
- **Chrome extension**: browser relay for CDP control
- **Tailscale exposure**: Serve/Funnel for remote access
- **Onboarding wizard**: guided setup flow via gateway

**Paw needs to surface ALL of this through a GUI.**

---

## Architecture Overview

```
┌──────────────────────────────────────────────────────┐
│                    Paw Desktop                        │
│  ┌─────────────────┐  ┌──────────────────────────┐   │
│  │  Rust Backend    │  │  Web Frontend (Vite)     │   │
│  │  src-tauri/      │  │  src/main.ts (5,394 LOC) │   │
│  │  lib.rs (1,947)  │  │  styles.css  (4,390 LOC) │   │
│  │                  │  │  index.html  (1,552 LOC) │   │
│  │  Tauri Commands: │  │  gateway.ts  (612 LOC)   │   │
│  │  - install       │  │  types.ts    (496 LOC)   │   │
│  │  - start/stop gw │  │  api.ts      (40 LOC)    │   │
│  │  - config R/W    │  │  db.ts       (350 LOC)   │   │
│  │  - memory CLI    │  │                          │   │
│  │  - mail/keychain │  │  Total: ~14,800 LOC      │   │
│  └───────┬──────────┘  └──────────┬───────────────┘   │
│          │    Tauri IPC (invoke)  │                    │
│          └────────────────────────┘                    │
│                       │                                │
│              WebSocket (ws://127.0.0.1:18789)          │
│                       ▼                                │
│         ┌──────────────────────────┐                   │
│         │   OpenClaw Gateway       │                   │
│         │   (Node.js process)      │                   │
│         │   Protocol v3 WS API     │                   │
│         └──────────────────────────┘                   │
└──────────────────────────────────────────────────────┘
```

### Communication Flow

1. **Tauri IPC** (`invoke`): Frontend → Rust backend for OS-level operations (install, start/stop gateway, file I/O, config editing, `openclaw ltm` CLI commands)
2. **WebSocket** (protocol v3): Frontend → OpenClaw gateway for all runtime operations (chat, sessions, agents, channels, cron, skills, models, config, agent files)
3. **Local SQLite** (`@tauri-apps/plugin-sql`): Frontend-only persistent storage for agent modes, projects, content documents, research findings, email accounts

---

## Feature-by-Feature Status

### Legend
- ✅ **WIRED** — Connected to gateway, functional when gateway is running
- 🔶 **PARTIAL** — UI exists, some logic works, but key paths are broken or incomplete
- 🔴 **SHELL ONLY** — UI exists in HTML/CSS but has no working backend logic
- ⚪ **NOT BUILT** — Mentioned in plans but no code exists

---

### 1. Onboarding & Setup ✅ WIRED
| Component | Status | Details |
|-----------|--------|---------|
| Detect existing OpenClaw | ✅ | `check_openclaw_installed` — checks `~/.openclaw/openclaw.json` exists |
| Auto-read token/port | ✅ | `get_gateway_token`, `get_gateway_port_setting` — reads from config |
| Manual gateway config | ✅ | Form → saves to localStorage → connects WebSocket |
| Install OpenClaw | 🔶 | `install_openclaw` command exists. Downloads Node.js bundle, runs `npm install openclaw`. **Blocker**: Requires bundled `resources/node/node-{os}-{arch}.tar.gz` which is NOT in the repo — install will fail without it |
| Auto-start gateway | ✅ | `start_gateway` → runs `openclaw gateway install` + `openclaw gateway start` |
| Auto-stop gateway | ✅ | `stop_gateway` → runs `openclaw gateway stop` with fallback to `pkill` |
| Config repair | ✅ | `repair_openclaw_config` — removes stale keys added by earlier versions |
| Reconnect logic | ✅ | Exponential backoff (3s→60s), max 20 attempts, 15s health poll |

**What's missing**:
- No bundled Node.js tarballs in `resources/node/` — first-time install will fail
- No progress UI for "starting gateway" (only for installation)
- No error recovery if gateway crashes after connection

---

### 2. Chat ✅ WIRED
| Component | Status | Details |
|-----------|--------|---------|
| Session list | ✅ | `sessions.list` → dropdown select. Filters out internal `paw-*` sessions |
| Load history | ✅ | `chat.history` → renders messages with timestamps |
| Send message | ✅ | `chat.send` → streaming via `agent` events (deltas) + `chat` final event |
| Streaming bubbles | ✅ | Live delta appending, auto-scroll, 120s timeout |
| New chat | ✅ | Clears messages and session key |
| Tool call badges | ✅ | Shows "N tool calls" badge on messages |
| Agent name display | ✅ | Fetches from `agents.list` on connect |
| Abort | ✅ | Stop button visible during streaming, calls `chat.abort` |
| Session rename | ✅ | `sessions.patch` with label via prompt modal |
| Session delete | ✅ | `sessions.delete` with confirmation |
| Markdown rendering | ✅ | `formatMarkdown()` — bold, italic, code blocks, inline code, headers, links, lists, blockquotes, tables, horizontal rules |
| Mode selection | ✅ | Dropdown in chat header — selected mode's model, system_prompt, thinking_level, temperature sent with `chat.send` |
| Toast notifications | ✅ | Success/error/info toasts with auto-dismiss |
| Token meter | ✅ | Progress bar in chat header — tokens used / context limit, color-coded (green/yellow/red), auto-detect model context window |
| Compaction warning | ✅ | Yellow banner when context ≥80% full, escalates at 95%, dismissible |

**What's missing**:
- No session search
- No thinking level selector per message (uses mode's default)

**Recent additions (2026-02-15)**:
- ✅ Retry button on messages (resend last user message)
- ✅ Attachment picker UI (📎 button, file picker, preview strip)
- ✅ Image attachment rendering in chat bubbles
- ✅ `ChatAttachment` type + gateway `chatSend()` attachment support

---

### 3. Build (IDE) 🔶 PARTIAL
| Component | Status | Details |
|-----------|--------|---------|
| Create project | ✅ | Creates project in SQLite with `space: 'build'` |
| File explorer | 🔶 | Shows in-memory file list, but NOT connected to `project_files` DB table |
| Code editor | 🔶 | Plain `<textarea>` — no syntax highlighting, no Monaco |
| Tab system | ✅ | Open/close/switch tabs for in-memory files |
| Build chat | 🔶 | Sends to gateway with file context, but response goes to "check Chat view" — **NOT streamed back into Build** |
| Run/deploy | 🔴 | No run, build, or deploy functionality |
| Git integration | 🔴 | No git operations despite "Code" view existing |

**What's critically missing**:
- Files are **only in memory** — not saved to SQLite `project_files` table (no persistence)
- Build chat responses are NOT routed back to the Build view — they say "check Chat view"
- No syntax highlighting (should add CodeMirror or Monaco)
- No file save/load from gateway agent workspace
- No terminal/console output panel
- The "Code" view (`code-view`) is a completely **empty shell** — zero functionality

---

### 4. Create (Content Studio) ✅ WIRED
| Component | Status | Details |
|-----------|--------|---------|
| Document CRUD | ✅ | Create, open, save, delete via SQLite `content_documents` table |
| Document list sidebar | ✅ | Shows documents with word count and date |
| Text editor | ✅ | Plain `<textarea>` with auto word count |
| Content type select | ✅ | markdown/html/plaintext selector |
| AI Improve | ✅ | Sends to gateway via `agent` + `agentWait` — direct sessionless run, result applied to editor |
| Delete document | ✅ | With confirmation |

**What's missing**:
- No markdown preview/rendering
- No export (PDF, HTML, etc.)
- No AI generate from scratch
- No rich text formatting toolbar

---

### 5. Mail ✅ WIRED
| Component | Status | Details |
|-----------|--------|---------|
| Email account setup | ✅ | Provider picker (Gmail, Outlook, Yahoo, iCloud, Fastmail, Custom) with pre-filled IMAP/SMTP servers, app password hints |
| IMAP/SMTP config | ✅ | Tauri commands `write_himalaya_config`/`read_himalaya_config`/`remove_himalaya_account` — writes Himalaya TOML to `~/.config/himalaya/config.toml` |
| OS Keychain | ✅ | Passwords stored in macOS Keychain / libsecret via `keyring` crate v3. TOML contains `auth.cmd` reference, NOT plaintext. `keyring_has_password`/`keyring_delete_password` Tauri commands |
| Credential Vault | ✅ | Expandable per-account vault cards showing permissions, metadata, and revoke button |
| Agent permissions | ✅ | Per-account read/send/delete/manage toggles. Enforced in `exec.approval.requested` handler — auto-denies blocked permissions |
| Audit log | ✅ | SQLite `credential_activity_log` table. Activity log viewer in mail sidebar (collapsible, shows blocked count) |
| Himalaya skill status | ✅ | Shows Himalaya CLI skill install/config status in mail sidebar |
| Inbox display | ✅ | `emails` table in SQLite, inbox list with sender/subject/date, email preview pane |
| Compose | ✅ | Compose form with to/subject/body, sends via Himalaya skill through gateway |
| Security info | ✅ | Transparent panel showing exactly how credentials are stored (keychain, TLS, no cloud, permission-gated, audit logged, revocable) |
| Account revocation | ✅ | "Revoke Access" per account — deletes TOML config, keychain entry, and signals agent |
| File permissions | ✅ | TOML config file set to chmod 600 (owner-only read) |
| Password redaction | ✅ | `read_himalaya_config` redacts `auth.cmd` lines before returning to JS — credential text never reaches frontend |

**What's missing**:
- Inbox relies on Himalaya CLI skill being installed and configured in the gateway — not a native IMAP client
- No real-time email notifications / push
- No email search
- No folder management UI (permissions exist but no folder browser)
- No attachment handling in compose

---

### 6. Automate (Cron/Scheduled Tasks) ✅ WIRED
| Component | Status | Details |
|-----------|--------|---------|
| List jobs | ✅ | `cron.list` → renders active/paused/history board |
| Create job | ✅ | Modal with label, cron schedule, prompt. `cron.add` |
| Toggle enable/disable | ✅ | `cron.update` with `enabled` toggle |
| Run now | ✅ | `cron.run` triggers immediate execution |
| Delete job | ✅ | `cron.remove` with confirmation |
| Run history | ✅ | `cron.runs` shows last 10 runs with status |
| Schedule presets | ✅ | Dropdown with common cron patterns |
| Dashboard widget | ✅ | Shows up to 8 jobs on dashboard |
| Space-contextual cron | ✅ | Filters jobs by keyword per space (build/content/mail/research) |

**Working well.** Minor improvements:
- No cron expression validation
- No visual cron builder (text-only)
- No job edit (only create/delete/toggle)

---

### 7. Channels ✅ WIRED
| Component | Status | Details |
|-----------|--------|---------|
| List channels | ✅ | `channels.status` with probe → renders cards |
| Show status | ✅ | Connected/Disconnected/Not configured with visual indicators |
| Channel setup UI | ✅ | Per-channel setup forms (Telegram bot token, Discord token, WhatsApp QR, Slack bot+app tokens, Signal phone number) with sensitive field handling |
| Login flow | ✅ | `web.login.start` + `web.login.wait` (120s timeout) |
| Logout | ✅ | `channels.logout` with confirmation |
| Refresh | ✅ | Per-channel and global refresh |
| Account display | ✅ | Shows linked accounts per channel |

**Working well.** Depends on gateway having channels configured in `openclaw.json`.

---

### 8. Research ✅ WIRED
| Component | Status | Details |
|-----------|--------|---------|
| Create project | ✅ | SQLite `projects` table with `space: 'research'` |
| Project sidebar | ✅ | Lists projects with active selection |
| Research input | ✅ | Text input → sends to gateway via `chat.send` with research prompt |
| Live streaming | ✅ | Agent events routed to research live output area (filtered by `paw-research-*` session) |
| Save findings | ✅ | Auto-saves to `content_documents` with `content_type: 'research-finding'` |
| View findings | ✅ | Finding cards with markdown-ish rendering, timestamps, delete button |
| Generate report | ✅ | Compiles all findings → sends to agent → renders synthesized report |
| Abort research | ✅ | `chat.abort` on the research session |
| Delete project | ✅ | Cascading delete of project + all findings |

**Working well.** Improvements needed:
- No way to edit findings after save
- No export report to file
- Report lives in memory only (not saved to DB)
- Web browsing capabilities depend on agent having the right skills (brave_search, fetch, etc.)

---

### 9. Memory ✅ WIRED (Complex)
| Component | Status | Details |
|-----------|--------|---------|
| Agent files list | ✅ | `agents.files.list` → shows files with size |
| Agent file view/edit | ✅ | `agents.files.get`/`agents.files.set` with save |
| LanceDB setup | ✅ | `enable_memory_plugin` writes to `openclaw.json`, tests embedding connection, restarts gateway |
| Azure OpenAI routing | ✅ | Full Azure support: source patches, runtime shim (`NODE_OPTIONS --require`), env var injection |
| Provider selection | ✅ | OpenAI / Azure dropdown with provider-specific fields |
| Connection testing | ✅ | `test_embedding_connection` sends real embedding request via curl |
| Recall (semantic search) | ✅ | `memory_search` → `openclaw ltm search` CLI |
| Remember (store memory) | 🔶 | Uses `chat.send` to ask agent to call `memory_store` — **indirect and unreliable** |
| Knowledge graph viz | 🔶 | Canvas bubble chart grouped by category — but data is just memory search results, not a real graph |
| Memory stats | ✅ | `memory_stats` → `openclaw ltm stats` CLI |
| Memory export | ✅ | Export all memories as timestamped JSON file (Blob download, up to 500 memories) |
| Sidebar search | ✅ | Client-side filter of loaded memory cards |
| Skip setup | ✅ | Falls back to agent files view |
| Reconfigure | ✅ | Settings gear reopens setup form with pre-filled values |

**Biggest issues**:
- "Remember" is routing through chat session to ask the agent to store — it should call the CLI directly (`openclaw ltm store`)
- Knowledge graph is a mock bubble chart, not an actual relationship graph
- LanceDB plugin availability depends on gateway restart (which can fail silently)

---

### 10. Skills ✅ WIRED
| Component | Status | Details |
|-----------|--------|---------|
| List skills | ✅ | `skills.status` → installed vs available with requirement checks |
| Install skill | ✅ | `skills.install` with loading state |
| Enable/disable toggle | ✅ | `skills.update` with `enabled` flag |
| Configure (API keys) | ✅ | Modal with env var inputs, `skills.update` with `apiKey`/`env` |
| Missing requirement indicators | ✅ | Shows missing bins, env vars, config |
| Browse bins | ✅ | `skills.bins` → modal list with install buttons |
| Custom bin install | ✅ | Free-text name → `skills.install` |
| Toast notifications | ✅ | Success/error/info toasts with auto-dismiss |

**Working well.** One of the most complete features.

---

### 11. Foundry (Models + Agent Modes + Multi-Agent) ✅ WIRED
| Component | Status | Details |
|-----------|--------|---------|
| Models list | ✅ | `models.list` → cards with provider, context window, reasoning badge |
| Agent modes CRUD | ✅ | SQLite-backed — create, edit, delete modes with icon, color, model, system prompt, thinking level, temperature |
| Mode selection in Chat | ✅ | Dropdown in chat header sends mode's overrides with `chat.send` |
| Default mode | ✅ | Seed data creates General/Code Review/Quick Chat modes |
| Tab switching | ✅ | Models / Modes / Agents tabs |
| Multi-agent CRUD | ✅ | `agents.create`/`agents.update`/`agents.delete` — create, edit, delete agents from Paw |
| Agent detail view | ✅ | Per-agent detail panel with identity (emoji/name), file cards, workspace files |
| Agent file cards | ✅ | Standard agent files (AGENTS.md, SOUL.md, USER.md, IDENTITY.md, TOOLS.md, HEARTBEAT.md) with create/edit, plus custom files |
| Agent default selection | ✅ | Set default agent from Foundry |
| Agent form | ✅ | Create/edit modal with name, icon, workspace path, model override |

**What's missing**:
- No model switching from Foundry (read-only list)
- No subscription/billing UI (planned per business model)
- No agent routing configuration (which channels/sessions → which agent)

---

### 12. Settings ✅ WIRED
| Component | Status | Details |
|-----------|--------|---------|
| Gateway URL/token config | ✅ | Edit + reconnect |
| OpenClaw config editor | ✅ | `config.get` → JSON textarea → `config.set` |
| Config reload | ✅ | Re-fetches from gateway |
| Gateway version display | ✅ | Shows uptime from health check |
| Gateway logs | ✅ | `logs.tail` → real-time log viewer in Settings panel |
| Usage stats | ✅ | Token/request usage display |
| Connected clients | ✅ | `system-presence` → shows connected operator clients |
| About section | ✅ | Version, links |

---

### 13. Code View 🔴 SHELL ONLY

The sidebar has a "Code" nav item (`data-view="code"`), and the HTML contains `<div id="code-view">` — but the view body is **completely empty**. There is:
- No HTML content for the code view
- No JavaScript handlers
- No gateway integration
- Zero functionality

This was planned for "Git repos, branches, PRs, code review" per the dashboard card description.

---

### 14. Dashboard ✅ WIRED
| Component | Status | Details |
|-----------|--------|---------|
| Welcome greeting | ✅ | Static |
| Quick actions | ✅ | New Chat, Build App, Check Mail (navigation buttons) |
| Feature cards | ✅ | Navigates to each view |
| Cron widget | ✅ | Shows scheduled tasks from gateway |
---

### 15. TTS (Text-to-Speech) ⚪ NOT BUILT
| Component | Status | Details |
|-----------|--------|--------|
| TTS status/toggle | ⚪ | `tts.status`, `tts.enable`, `tts.disable` — no UI |
| Provider selection | ⚪ | `tts.providers`, `tts.setProvider` — ElevenLabs/OpenAI/Edge |
| Convert text → speech | ⚪ | `tts.convert` — play audio next to messages |

OpenClaw supports full TTS with multiple providers. Paw has **zero** coverage.

---

### 16. Talk Mode ⚪ NOT BUILT
| Component | Status | Details |
|-----------|--------|--------|
| Talk config | ⚪ | `talk.config` — voice ID, provider settings |
| Talk mode toggle | ⚪ | `talk.mode` — enable/disable continuous voice conversation |
| Talk mode event | ⚪ | `talk.mode` event — react to talk mode state changes |

ElevenLabs-powered continuous conversation. Paw has **zero** coverage.

---

### 17. Voice Wake ⚪ NOT BUILT
| Component | Status | Details |
|-----------|--------|--------|
| Get wake words | ⚪ | `voicewake.get` — list configured wake words |
| Set wake words | ⚪ | `voicewake.set` — configure wake word triggers |
| Wake events | ⚪ | `voicewake.changed` event — react to wake word config changes |

Wake word system for hands-free activation. Paw has **zero** coverage.

---

### 18. Node Management ✅ WIRED
| Component | Status | Details |
|-----------|--------|--------|
| List nodes | ✅ | `node.list` → sidebar list with status indicators, auto-refresh |
| Describe node | ✅ | `node.describe` → detail panel with capabilities, meta grid |
| Invoke node command | ✅ | `node.invoke` → command button grid per capability |
| Node pairing flow | ✅ | `node.pair.list/approve/reject` → pairing request cards in sidebar |
| Rename node | ✅ | `node.rename` → inline rename from detail header |
| Node events | ✅ | `node.pair.requested/resolved`, `node.invoke.result`, `node.event` — all consumed |

**Fully wired (2026-02-15)**: `src/views/nodes.ts` module + HTML + main.ts wiring + CSS. Sidebar list with node detail panel, capability badges, command buttons, pairing request cards, gateway event handlers.

---

### 19. Device Pairing ✅ WIRED
| Component | Status | Details |
|-----------|--------|--------|
| List devices | ✅ | `device.pair.list` → Settings section with device cards |
| Approve/reject | ✅ | Settings receives `device.pair.requested` events, refreshes list |
| Token management | ✅ | `device.token.rotate/revoke` — Rotate Token and Revoke buttons per device card |
| Device events | ✅ | `device.pair.requested` + `device.pair.resolved` consumed → auto-refresh Settings |

**Wired (2026-02-15)**: Device cards in Settings view with platform, paired date, rotate token, and revoke access actions.

---

### 20. Exec Approvals ✅ WIRED
| Component | Status | Details |
|-----------|--------|--------|
| Approval modal | ✅ | `exec.approval.requested` event → shows approve/deny modal with tool name, arguments, session info |
| Resolve approvals | ✅ | Approve/deny buttons → `exec.approval.resolve` |
| Mail permission enforcement | ✅ | Auto-denies email tools when Credential Vault permissions are disabled (read/send/delete/manage) |
| Audit logging | ✅ | All approval decisions (and auto-blocks) logged to SQLite `credential_activity_log` |
| Activity log viewer | ✅ | Collapsible log in mail sidebar showing allowed/blocked actions with timestamps |
| Approval config UI | ✅ | `exec.approvals.get/set` → Settings section with radio-card policy selector (Ask/Allow/Block) + per-tool 3-way toggle rows (Allow/Ask/Block) + Add Rule prompt modal |

---

### 21. Usage Tracking ✅ WIRED
| Component | Status | Details |
|-----------|--------|--------|
| Usage status | ✅ | `usage.status` → Settings Usage section with requests, tokens, cost cards |
| Cost breakdown | ✅ | `usage.cost` → per-model breakdown rows in Usage section |

Fully wired. **Gap**: No per-conversation cost, no budget alerts, no cost-per-feature breakdown (see Community Gap Analysis).

---

### 22. Onboarding Wizard ✅ WIRED
| Component | Status | Details |
|-----------|--------|--------|
| Start wizard | ✅ | `wizard.start` → Start button in Settings Wizard section |
| Step through | ✅ | `wizard.next` → Next Step button, renders step content |
| Cancel | ✅ | `wizard.cancel` → Cancel button |
| Status | ✅ | `wizard.status` → Status badge (active/completed/idle) |

Fully wired in Settings. **Gap**: No error recovery flow, no "gateway crashed" handling (see Community Gap Analysis).

---

### 23. Browser Control ✅ WIRED
| Component | Status | Details |
|-----------|--------|--------|
| Browser status | ✅ | `browser.status` → Settings Browser section with running/stopped badge |
| Tab list | ✅ | `browser.status` → renders open tabs with title + URL |
| Start/Stop | ✅ | `browser.start/stop` → control buttons |

Fully wired in Settings. **Gap**: No screenshot viewer, no tab interaction (see Community Gap Analysis).

---

### 24. Self-Update ✅ WIRED
| Component | Status | Details |
|-----------|--------|--------|
| Update OpenClaw | ✅ | `update.run` → "Update OpenClaw" button in Settings About section, shows result toast |

One-click update fully working.

---

### 25. Logs Viewer ✅ WIRED
| Component | Status | Details |
|-----------|--------|--------|
| Tail logs | ✅ | `logs.tail` → Settings Logs section with auto-refresh, filterable |

Fully wired in Settings.

---

## Critical Gaps — What Needs Wiring

### Priority 1: Things that look broken to users

| Issue | Location | Fix Required |
|-------|----------|-------------|
| ~~**Agent modes not used in chat**~~ | ~~`sendMessage()` in main.ts~~ | ✅ FIXED — Mode selector in chat header, overrides sent with `chat.send` |
| **Build chat responses lost** | Build chat send handler | Route `paw-build-*` session events back to Build view (like Research does) |
| **Content AI Improve responses lost** | `content-ai-improve` handler | Stream response back to the editor, don't redirect to Chat |
| ~~**Mail is completely empty**~~ | ~~mail-view, db.ts~~ | ✅ FIXED — Full Himalaya integration, provider setup, credential vault, OS keychain, audit log |
| **Code view is completely empty** | code-view | Either build git integration or remove from nav |
| **No bundled Node.js** | resources/node/ | Add platform-specific Node.js tarballs for the installer or document how to add them |
| ~~**Remember uses chat instead of CLI**~~ | ~~`palace-remember-save` handler~~ | ✅ FIXED — Uses `invoke('memory_store', ...)` Tauri command directly |

### Priority 2: Data loss / persistence issues

| Issue | Location | Fix Required |
|-------|----------|-------------|
| ~~**Build files not persisted**~~ | ~~Build IDE handlers~~ | ✅ FIXED — Files saved to SQLite `project_files` table |
| ~~**Research reports not saved**~~ | ~~`generateResearchReport()`~~ | ✅ FIXED — Reports saved as content documents |
| **No session persistence across restarts** | Chat sessions | Sessions come from gateway — but selected session / scroll position lost |

### Priority 3: Missing polish

| Issue | Location | Fix Required |
|-------|----------|-------------|
| ~~Chat messages are plain text~~ | ~~`renderMessages()`~~ | ✅ FIXED — `formatMarkdown()` renders bold, italic, code, headers, links, lists, tables |
| ~~No chat abort button~~ | ~~chat-view HTML~~ | ✅ FIXED — Stop button visible during streaming |
| No syntax highlighting in Build | build-code-editor | Add CodeMirror or similar |
| Knowledge graph is fake data | `renderPalaceGraph()` | Either build real graph from memory relationships or remove |
| ~~No mode selector in Chat~~ | ~~chat-view header~~ | ✅ FIXED — Dropdown switches agent mode |
| Cron jobs can't be edited | Cron modal | Add edit mode, not just create/delete |

---

## File Map (Updated 2026-02-15)

| File | LOC | Purpose |
|------|-----|---------|
| `src/main.ts` | 2,732 | **Core UI logic** — navigation, chat, event handlers (refactored from 5,394) |
| `src/styles.css` | ~4,500 | **All styling** — Monday.com-inspired light theme, layout, components |
| `index.html` | ~1,600 | **All DOM structure** — sidebar, views, modals |
| `src/gateway.ts` | ~810 | **WebSocket gateway client** — Protocol v3, ~80+ methods typed |
| `src/types.ts` | ~548 | **TypeScript types** — gateway protocol types, ChatAttachment, UI types |
| `src/db.ts` | 350 | **SQLite database** — migrations, CRUD |
| `src/api.ts` | 40 | **HTTP health probe** |
| `src-tauri/src/lib.rs` | 1,947 | **Rust backend** — Tauri commands, keychain, config |

### View Modules (`src/views/`)
| File | LOC | Purpose |
|------|-----|---------|
| `memory-palace.ts` | 877 | Agent files, LanceDB memory, knowledge graph |
| `mail.ts` | 849 | Himalaya integration, credential vault, inbox |
| `foundry.ts` | 539 | Models, modes, multi-agent CRUD |
| `nodes.ts` | 436 | **NEW** — Node management, pairing, commands |
| `skills.ts` | 413 | Skill browser, install, configure |
| `research.ts` | 360 | Research projects, findings, reports |
| `automations.ts` | 183 | Cron job management |
| `settings.ts` | ~630 | Gateway config, logs, usage, presence, wizard, browser, update |
| **Total views** | **3,838** | Extracted from main.ts |

---

## Complete Gateway Protocol Coverage (OpenClaw vs Paw)

Source of truth: `openclaw/src/gateway/server-methods-list.ts`

### All 88+ Gateway Methods

#### Core / Health / Status
| Method | In gateway.ts | Called from UI | Notes |
|--------|:---:|:---:|-------|
| `health` | ✅ | ✅ | Keepalive + health polling |
| `status` | ✅ | ✅ | Settings → Gateway Status section with session/agent/channel counts |
| `logs.tail` | ✅ | ✅ | Settings → Gateway Logs viewer with line count selector |

#### Channels
| Method | In gateway.ts | Called from UI | Notes |
|--------|:---:|:---:|-------|
| `channels.status` | ✅ | ✅ | Channels view |
| `channels.logout` | ✅ | ✅ | Channels view |
| `web.login.start` | ✅ | ✅ | Channels view |
| `web.login.wait` | ✅ | ✅ | Channels view |

#### Sessions
| Method | In gateway.ts | Called from UI | Notes |
|--------|:---:|:---:|-------|
| `sessions.list` | ✅ | ✅ | Chat session dropdown |
| `sessions.preview` | ✅ | ✅ | Typed + available for session preview |
| `sessions.patch` | ✅ | ✅ | Session rename from Chat UI |
| `sessions.reset` | ✅ | ✅ | Clear History button in chat header |
| `sessions.delete` | ✅ | ✅ | Session delete from Chat UI |
| `sessions.compact` | ✅ | ✅ | Compact button in chat header |

#### Chat
| Method | In gateway.ts | Called from UI | Notes |
|--------|:---:|:---:|-------|
| `chat.history` | ✅ | ✅ | Chat + Research views |
| `chat.send` | ✅ | ✅ | Chat + Research + Build + Content |
| `chat.abort` | ✅ | ✅ | Chat + Research views |

#### Agent
| Method | In gateway.ts | Called from UI | Notes |
|--------|:---:|:---:|-------|
| `agent` | ✅ | ✅ | Content AI Improve — direct sessionless agent run |
| `agent.identity.get` | ✅ | ✅ | Chat header → shows agent emoji + name |
| `agent.wait` | ✅ | ✅ | Content AI Improve — waits for agent result |
| `agents.list` | ✅ | ✅ | Chat view (display agent name) + Foundry |
| `agents.create` | ✅ | ✅ | Foundry — create new agents |
| `agents.update` | ✅ | ✅ | Foundry — edit agent config |
| `agents.delete` | ✅ | ✅ | Foundry — delete agents |
| `agents.files.list` | ✅ | ✅ | Memory view + Foundry agent detail |
| `agents.files.get` | ✅ | ✅ | Memory view + Foundry agent detail |
| `agents.files.set` | ✅ | ✅ | Memory view + Foundry agent detail |

#### Cron / Automation
| Method | In gateway.ts | Called from UI | Notes |
|--------|:---:|:---:|-------|
| `cron.list` | ✅ | ✅ | Automations view |
| `cron.status` | ✅ | ✅ | Automations view header → scheduler status badge |
| `cron.add` | ✅ | ✅ | Automations view |
| `cron.update` | ✅ | ✅ | Automations view (enable/disable) |
| `cron.remove` | ✅ | ✅ | Automations view |
| `cron.run` | ✅ | ✅ | Automations view |
| `cron.runs` | ✅ | ✅ | Automations view (history) |
| `wake` | ✅ | ✅ | Dashboard → Wake Agent quick action button |

#### Skills
| Method | In gateway.ts | Called from UI | Notes |
|--------|:---:|:---:|-------|
| `skills.status` | ✅ | ✅ | Skills view |
| `skills.bins` | ✅ | ✅ | Skills bins modal |
| `skills.install` | ✅ | ✅ | Skills view |
| `skills.update` | ✅ | ✅ | Skills view (enable/disable/config) |

#### Models
| Method | In gateway.ts | Called from UI | Notes |
|--------|:---:|:---:|-------|
| `models.list` | ✅ | ✅ | Foundry view |

#### Config
| Method | In gateway.ts | Called from UI | Notes |
|--------|:---:|:---:|-------|
| `config.get` | ✅ | ✅ | Settings view |
| `config.set` | ✅ | ✅ | Settings view — Save (no restart) button |
| `config.apply` | ✅ | ✅ | Settings view — Apply Config button (validate + write + restart) |
| `config.patch` | ✅ | ❌ | Typed — available for partial config updates |
| `config.schema` | ✅ | ✅ | Settings view — View Schema button shows available config keys |

#### TTS (Text-to-Speech) — TYPED IN GATEWAY, NO UI
| Method | In gateway.ts | Called from UI | Notes |
|--------|:---:|:---:|-------|
| `tts.status` | ✅ | ❌ | Typed, no UI |
| `tts.providers` | ✅ | ❌ | Typed, no UI |
| `tts.enable` | ✅ | ❌ | Typed, no UI |
| `tts.disable` | ❌ | ❌ | Merged into `tts.enable(false)` |
| `tts.convert` | ✅ | ❌ | Typed, no UI |
| `tts.setProvider` | ✅ | ❌ | Typed, no UI |

#### Talk Mode — TYPED IN GATEWAY, NO UI
| Method | In gateway.ts | Called from UI | Notes |
|--------|:---:|:---:|-------|
| `talk.config` | ✅ | ❌ | Typed, no UI |
| `talk.mode` | ✅ | ❌ | Typed, no UI |

#### Voice Wake — TYPED IN GATEWAY, NO UI
| Method | In gateway.ts | Called from UI | Notes |
|--------|:---:|:---:|-------|
| `voicewake.get` | ✅ | ❌ | Typed, no UI |
| `voicewake.set` | ✅ | ❌ | Typed, no UI |

#### Node Management — ✅ FULLY WIRED
| Method | In gateway.ts | Called from UI | Notes |
|--------|:---:|:---:|-------|
| `node.list` | ✅ | ✅ | Nodes sidebar list + auto-refresh |
| `node.describe` | ✅ | ✅ | Detail panel with capabilities |
| `node.invoke` | ✅ | ✅ | Command button grid |
| `node.invoke.result` | ✅ | ✅ | Event consumed → refreshes node list |
| `node.event` | ✅ | ✅ | Event consumed → refreshes node list |
| `node.rename` | ✅ | ✅ | Inline rename from detail header |
| `node.pair.request` | ❌ | ❌ | Client-side — not needed |
| `node.pair.list` | ✅ | ✅ | Pairing request cards in sidebar |
| `node.pair.approve` | ✅ | ✅ | Approve button on pairing cards |
| `node.pair.reject` | ✅ | ✅ | Reject button on pairing cards |
| `node.pair.verify` | ❌ | ❌ | NOT TYPED |

#### Device Pairing — ✅ WIRED IN SETTINGS
| Method | In gateway.ts | Called from UI | Notes |
|--------|:---:|:---:|-------|
| `device.pair.list` | ✅ | ✅ | Settings → device cards list |
| `device.pair.approve` | ✅ | ✅ | Via event-driven refresh |
| `device.pair.reject` | ✅ | ✅ | Via event-driven refresh |
| `device.token.rotate` | ✅ | ✅ | Rotate Token button per device card |
| `device.token.revoke` | ✅ | ✅ | Revoke button per device card |

#### Exec Approvals — ✅ FULLY WIRED
| Method | In gateway.ts | Called from UI | Notes |
|--------|:---:|:---:|-------|
| `exec.approvals.get` | ✅ | ✅ | Settings → loads allow/deny/askPolicy |
| `exec.approvals.set` | ✅ | ✅ | Settings → saves approval rules |
| `exec.approvals.node.get` | ✅ | ✅ | Typed — per-node approval rules |
| `exec.approvals.node.set` | ✅ | ✅ | Typed — per-node approval rules |
| `exec.approval.request` | — | — | Server-side only (agent calls this, not UI) |
| `exec.approval.waitDecision` | — | — | Server-side only (agent calls this, not UI) |
| `exec.approval.resolve` | ✅ | ✅ | Approve/deny from modal + auto-deny for mail permissions |

#### Usage Tracking — ✅ WIRED
| Method | In gateway.ts | Called from UI | Notes |
|--------|:---:|:---:|-------|
| `usage.status` | ✅ | ✅ | Settings → Usage & Cost section (requests, tokens, by-model breakdown) |
| `usage.cost` | ✅ | ✅ | Settings → Usage & Cost section (total cost, currency) |

#### System / Presence
| Method | In gateway.ts | Called from UI | Notes |
|--------|:---:|:---:|-------|
| `system-presence` | ✅ | ✅ | Settings → Connected Clients section + `presence` event auto-refresh |
| `system-event` | ✅ | ✅ | Typed — trigger system event |
| `last-heartbeat` | ✅ | ✅ | Typed — get last heartbeat info |
| `set-heartbeats` | ✅ | ✅ | Typed — enable/disable heartbeats |

#### Onboarding Wizard — ✅ WIRED
| Method | In gateway.ts | Called from UI | Notes |
|--------|:---:|:---:|-------|
| `wizard.start` | ✅ | ✅ | Settings → Start Wizard button |
| `wizard.next` | ✅ | ✅ | Settings → Next Step button |
| `wizard.cancel` | ✅ | ✅ | Settings → Cancel Wizard button |
| `wizard.status` | ✅ | ✅ | Settings → Wizard status badge (active/completed/idle) |

#### Update — ✅ WIRED
| Method | In gateway.ts | Called from UI | Notes |
|--------|:---:|:---:|-------|
| `update.run` | ✅ | ✅ | Settings → About → Update OpenClaw button |

#### Browser Control — ✅ WIRED
| Method | In gateway.ts | Called from UI | Notes |
|--------|:---:|:---:|-------|
| `browser.status` | ✅ | ✅ | Settings → Browser Control status badge + tab list |
| `browser.start` | ✅ | ✅ | Settings → Start Browser button |
| `browser.stop` | ✅ | ✅ | Settings → Stop Browser button |

#### Direct Send
| Method | In gateway.ts | Called from UI | Notes |
|--------|:---:|:---:|-------|
| `send` | ✅ | ✅ | Channels → Send Direct Message form (select channel + text input) |

### All 18 Gateway Events

| Event | Consumed by Paw | Notes |
|-------|:---:|-------|
| `connect.challenge` | ✅ | Handshake nonce |
| `agent` | ✅ | Streaming deltas for chat/research |
| `chat` | ✅ | Final assembled messages |
| `presence` | ✅ | Consumed → auto-refreshes Settings Connected Clients |
| `tick` | ❌ | **Not consumed** — periodic status ticks |
| `talk.mode` | ❌ | **Not consumed** — talk mode state changes |
| `shutdown` | ✅ | Consumed → shows "Gateway shutting down" toast |
| `health` | ❌ | **Not consumed** — health snapshot pushes |
| `heartbeat` | ❌ | **Not consumed** — heartbeat events |
| `cron` | ✅ | Consumed → auto-refreshes Automations view |
| `node.pair.requested` | ✅ | Consumed → pairing request card in Nodes sidebar |
| `node.pair.resolved` | ✅ | Consumed → refreshes pairing list |
| `node.invoke.result` | ✅ | Consumed → refreshes node list |
| `device.pair.requested` | ✅ | Consumed → refreshes Settings device list, shows toast |
| `device.pair.resolved` | ✅ | Consumed → refreshes Settings device list |
| `voicewake.changed` | ❌ | **Not consumed** — wake words updated |
| `exec.approval.requested` | ✅ | Approval modal + mail permission auto-deny |
| `exec.approval.resolved` | ✅ | Consumed → closes approval modal if open |

### Coverage Summary (Updated 2026-02-16)

| Category | Methods in OpenClaw | Methods typed in Paw | Methods called from UI | % Coverage |
|----------|:---:|:---:|:---:|:---:|
| Core/Health | 3 | 3 | 3 | **100%** ✅ |
| Channels | 4 | 4 | 4 | **100%** ✅ |
| Sessions | 6 | 6 | 6 | **100%** ✅ |
| Chat | 3 | 3 | 3 | **100%** ✅ |
| Agent | 10 | 10 | 10 | **100%** ✅ |
| Cron | 8 | 8 | 8 | **100%** ✅ |
| Skills | 4 | 4 | 4 | **100%** ✅ |
| Models | 1 | 1 | 1 | **100%** ✅ |
| Config | 5 | 5 | 4 | **100% typed, 80% UI** ✅ |
| TTS | 6 | 5 | 0 | 83% typed |
| Talk | 2 | 2 | 0 | **100% typed** |
| Voice Wake | 2 | 2 | 0 | **100% typed** |
| Nodes | 11 | 10 | 9 | **91%** ✅ |
| Devices | 5 | 5 | 5 | **100%** ✅ |
| Exec Approvals | 5 | 5 | 5 | **100%** ✅ (2 server-side only methods N/A) |
| Usage | 2 | 2 | 2 | **100%** ✅ |
| System | 4 | 4 | 4 | **100%** ✅ |
| Wizard | 4 | 4 | 4 | **100%** ✅ |
| Update | 1 | 1 | 1 | **100%** ✅ |
| Browser | 3 | 3 | 3 | **100%** ✅ |
| Send | 1 | 1 | 1 | **100%** ✅ |
| **TOTAL** | **~90** | **~88** | **~77** | **~98% typed, ~86% UI wired** |

**Progress**: Massive wiring sprint complete. 19 of 21 categories are now **100% wired or typed**. Only TTS (no UI) and Talk/Voice Wake (no UI) remain unwired. Events consumed: 12 of 18 (up from 8).

---

## Database Schema (SQLite — paw.db)

| Table | Used By | Status |
|-------|---------|--------|
| `agent_modes` | Foundry modes + Chat mode selector | ✅ CRUD works, modes sent with chat messages |
| `projects` | Build, Research | ✅ Working |
| `project_files` | Build IDE | ✅ Working — files persisted to SQLite |
| `automation_runs` | Automations | 🔴 Table exists, **never read or written** (uses gateway's `cron.runs` instead) |
| `research_findings` | Research | 🔴 Table exists, but **findings stored in `content_documents` instead** |
| `content_documents` | Content + Research findings + Research reports | ✅ Working |
| `email_accounts` | Mail | ✅ Working — stores account metadata and permission config |
| `emails` | Mail | ✅ Working — stores fetched emails for inbox display |
| `credential_activity_log` | Mail Credential Vault | ✅ Working — audit trail for all agent email actions and blocks |

**Note**: `research_findings` and `automation_runs` tables are orphaned — created by migrations but never used. Research findings go to `content_documents` with `content_type: 'research-finding'`. Automation runs come from the gateway (`cron.runs`).

---

## Tauri Commands (Rust → Frontend)

| Command | Used | Working |
|---------|------|---------|
| `check_node_installed` | Install flow | ✅ |
| `check_openclaw_installed` | Setup detection | ✅ |
| `check_gateway_health` | Health polling | ✅ |
| `get_gateway_token` | Config reading | ✅ |
| `get_gateway_port_setting` | Config reading | ✅ |
| `install_openclaw` | Installation | 🔶 Needs bundled Node.js |
| `start_gateway` | Gateway lifecycle | ✅ |
| `stop_gateway` | Gateway lifecycle | ✅ |
| `check_memory_configured` | Memory setup | ✅ |
| `enable_memory_plugin` | Memory setup | ✅ |
| `test_embedding_connection` | Memory setup | ✅ |
| `get_embedding_base_url` | Memory reconfigure | ✅ |
| `get_azure_api_version` | Memory reconfigure | ✅ |
| `get_embedding_provider` | Memory reconfigure | ✅ |
| `memory_stats` | Memory view | ✅ |
| `memory_search` | Memory recall | ✅ |
| `memory_store` | Memory "Remember" | ✅ |
| `repair_openclaw_config` | Startup | ✅ |
| `write_himalaya_config` | Mail setup — writes TOML config + stores password in OS keychain | ✅ |
| `read_himalaya_config` | Mail vault — reads config, redacts `auth.cmd` lines | ✅ |
| `remove_himalaya_account` | Mail revoke — removes TOML section + deletes keychain entry | ✅ |
| `set_owner_only_permissions` | Mail security — chmod 600 on himalaya config.toml | ✅ |
| `keyring_has_password` | Mail security — checks if OS keychain has password for account | ✅ |
| `keyring_delete_password` | Mail security — deletes password from OS keychain | ✅ |

---

## What Needs to Happen Next (Prioritized)

### ~~Phase 1: Fix broken wiring~~ ✅ DONE
1. ~~**Wire agent modes to chat** — When sending a message, include the selected mode's model/system_prompt/thinking_level~~
2. **Route Build chat responses** — Mirror Research's event routing pattern for `paw-build-*` sessions
3. **Route Content AI responses** — Stream AI improve results back to the editor
4. ~~**Add chat abort button** — Simple: show a Stop button during streaming, call `chat.abort`~~
5. ~~**Add markdown rendering to chat** — At minimum reuse `formatResearchContent()` for chat messages~~

### ~~Phase 2: Fix data loss~~ ✅ DONE
6. ~~**Persist Build files to SQLite** — Use the `project_files` table that already exists~~
7. ~~**Save research reports to DB** — Store generated reports as content documents~~
8. ~~**Fix Memory "Remember"** — Add a `memory_store` Tauri command that calls `openclaw ltm store` directly~~

### ~~Phase 3: Session management~~ ✅ DONE (rename + delete)
9. ~~**Session rename** — Call `sessions.patch` with label~~
10. ~~**Session delete** — Call `sessions.delete`, refresh dropdown~~
11. **Session reset/clear** — Call `sessions.reset` for "new conversation, same session"
12. **Session search/filter** — Client-side filter on session list

### Phase 4: Wire up the "FREE" features (gateway already supports them, Paw just needs UI)

These are features that OpenClaw already exposes via gateway methods. Paw just needs to add the UI and call them.

#### ~~4a. Exec Approvals~~ ✅ DONE
13. ~~**Approval dashboard** — Call `exec.approvals.get/set`, show allow/deny lists~~
14. ~~**Live approval notifications** — Listen to `exec.approval.requested` event, show approve/deny dialog~~
15. ~~**Resolve approvals** — Wire approve/deny buttons → `exec.approval.resolve`~~

#### 4b. Usage & Billing — ✅ WIRED
16. ~~**Usage dashboard** — Call `usage.status` + `usage.cost`, show token/cost breakdown~~ ✅ Wired in Settings
    - **Gap**: No per-conversation cost, no budget alerts (see Sprint 1)

#### 4c. TTS (Text-to-Speech)
17. **TTS settings panel** — `tts.status`, `tts.providers`, enable/disable/setProvider
18. **TTS toggle in chat** — Enable TTS for responses, preview voices
19. **Convert button** — `tts.convert` next to assistant messages

#### ~~4d. Logs Viewer~~ ✅ DONE
20. ~~**Logs tab in Settings** — `logs.tail` with auto-refresh, filterable~~ ✅ Wired

#### ~~4e. System Presence~~ ✅ DONE
21. ~~**Connected clients card** — `system-presence` → show who/what is connected (devices, apps, CLI)~~ ✅ Wired in Settings

#### ~~4f. Node Management~~ ✅ DONE
22. ~~**Nodes view** — `node.list` + `node.describe` → list paired nodes with caps/commands~~
23. ~~**Node pairing** — `node.pair.list/approve/reject` → approve iOS/Android nodes from Paw~~
24. ~~**Node invoke** — `node.invoke` → trigger camera.snap, screen.record, etc. from desktop~~

#### ~~4g. Device Pairing~~ ✅ DONE
25. ~~**Paired devices** — `device.pair.list/approve/reject` → manage trusted devices~~
26. ~~**Token management** — `device.token.rotate/revoke`~~

#### 4h. Voice Wake + Talk Mode
27. **Wake words editor** — `voicewake.get/set` → manage wake word triggers
28. **Talk mode toggle** — `talk.mode` (enable/disable), `talk.config` (show voice settings)
29. **Listen for changes** — consume `voicewake.changed` and `talk.mode` events

#### ~~4i. Multi-Agent Management~~ ✅ DONE
30. ~~**Agent CRUD** — `agents.create/update/delete` → manage multiple agents from Paw~~
31. **Agent routing** — configure which channels/sessions route to which agent (see Sprint 4)

#### ~~4j. Self-Update~~ ✅ DONE
32. ~~**Update button** — `update.run` → update OpenClaw from Paw, show progress~~ ✅ Wired

#### ~~4k. Onboarding Wizard~~ ✅ DONE
33. ~~**Wizard flow** — `wizard.start/next/cancel/status` → guided first-run setup~~ ✅ Wired
34. ~~Could replace/supplement current manual setup form~~

#### ~~4l. Browser Control~~ ✅ DONE
35. ~~**Browser panel** — `browser.request` → start/stop managed browser, view tabs~~ ✅ Wired
    - **Gap**: No screenshot viewer, no tab interaction (see Sprint 5)

#### 4m. Gateway Config
36. **Config validation** — `config.schema` → validate before saving
37. **Config apply** — `config.apply` instead of `config.set` (validate + write + restart atomically)
38. **Config patch** — `config.patch` for partial updates (safer than full set)

#### 4n. Gateway Events
39. Listen to `shutdown` event → show "gateway shutting down" banner
40. Listen to `health` event → update status in real-time without polling
41. Listen to `cron` event → update automations board in real-time
42. Listen to `presence` event → update connected clients live

### Phase 5: Build remaining empty shells
43. ~~**Mail** — Decision needed: build it or remove it~~ → ✅ **BUILT**: Full IMAP/SMTP setup via Himalaya, provider picker, credential vault, OS keychain, audit log, agent permission enforcement
44. **Code view** — Decision needed: build git integration (gateway has no git methods) or remove
45. ~~**Clean up orphaned DB tables**~~ — `email_accounts` and `emails` now used; `research_findings` and `automation_runs` still orphaned

### Phase 6: Polish
54. Add syntax highlighting to Build editor (CodeMirror)
55. Cron job editing (currently create/delete only) → Sprint 2 item
56. Real knowledge graph (or remove the mock)
57. Export research reports
58. Chat image/file/attachment support (OpenClaw `agent` method supports `attachments` array)
59. Webhook configuration UI
60. Memory export to JSON/CSV (Sprint 5)
61. Screenshot viewer for browser automation (Sprint 5)

---

## Dependencies on OpenClaw

Paw is **100% dependent on OpenClaw gateway**. Without it running:
- Chat, Research, Build chat, Content AI → all broken
- Channels, Skills, Models, Cron → all empty
- Memory (LanceDB) → requires both gateway + plugin configured
- Only local SQLite operations work (create/edit documents, manage modes)

OpenClaw must be installed as an npm package, its gateway started as a macOS LaunchAgent (or manually), and `~/.openclaw/openclaw.json` must contain a valid `gateway.auth.token`.

The gateway exposes its full API via WebSocket on `ws://127.0.0.1:{port}` (default port 18789).

---

## Summary

**What works**: Chat (streaming + markdown + abort + mode selection + session management + retry + attachments + image preview), Research (full flow), Channels (+ per-channel setup forms + direct channel send), Automations (+ live cron events), Skills, Models/Modes/Multi-Agent (CRUD + detail view), Memory (with setup), Settings (+ gateway status + logs + usage/cost + connected clients + device pairing + tool approval toggles + onboarding wizard + browser control + self-update), Dashboard (+ wake agent), Mail (full IMAP/SMTP setup via Himalaya + provider picker + credential vault + OS keychain + agent permissions + audit log + compose + inbox), Node Management (list + describe + invoke + pairing), Exec Approvals (live approval modal + resolve + node approvals + permission enforcement + visual toggle config UI), Content AI Improve (direct agent run). The core gateway integration is solid — **19 of 21 method categories at 100%**.

**What's broken**: Build/Content chat responses still not routed, Code view is empty.

**What's missing entirely**: TTS UI (5 typed methods, no UI), Talk Mode UI (2 typed, no UI), Voice Wake UI (2 typed, no UI). That's **~9 gateway methods with zero UI**. Beyond gateway wiring, the Community Gap Analysis identifies **19 feature items** across 5 sprints that address real user pain (memory visibility, cost tracking, cron reliability, multi-agent routing).

**Coverage reality**: Paw calls **~77 of ~90 gateway methods** (**~86% UI wired, ~98% typed**). 12 of 18 gateway events consumed. Every category except TTS/Talk/Voice Wake is at **100%**. Sprint 1 is in progress — token meter, compaction warnings, and memory export are now built. Users get real-time context visibility and data portability.

**Next up**: Sprint 1 remaining items (Usage Dashboard enhancement, per-conversation cost, budget alerts), then Sprint 2 (Cron Reliability). See Sprint Plan section for the full 19-item roadmap (3 of 19 complete).

---

## Community Gap Analysis (2026-02-15)

Based on OpenClaw community feedback — Reddit, Discord, GitHub issues. Maps real user pain to Paw's current state.

### 🔴 CRITICAL — Memory Woes

**Community problem**: "It forgets mid-sentence" — silent compaction, no visibility into what the agent remembers.

| What they need | Paw status | Gap |
|----------------|:---:|-----|
| Memory inspector (see what's in context window) | ⚪ | NOT BUILT — need real-time context window view showing what the agent "sees" |
| Memory usage meter (tokens consumed vs limit) | ✅ | **BUILT** — token meter progress bar in chat header, color-coded, auto-detects model context limit |
| Compaction warning ("about to forget") | ✅ | **BUILT** — yellow banner at 80% context capacity, escalates at 95%, dismissible |
| Memory embedding toggle + cost savings UI | 🔶 | Have LanceDB setup, no cost comparison UI (embedding vs no embedding) |
| Backup/export memory | ✅ | **BUILT** — export button in Memory Palace sidebar, downloads all memories as JSON |

### 🔴 CRITICAL — Cron/Automation Reliability

**Community problem**: Jobs timeout, fail silently, no way to debug what happened.

| What they need | Paw status | Gap |
|----------------|:---:|-----|
| Cron run history with errors | ✅ | Have `cron.runs` wired |
| Job status dashboard with error highlighting | 🔶 | Basic list, no error-state visual treatment (red rows, error icons) |
| Sub-agent spawn UI | ⚪ | NOT BUILT — users manually configure sub-agent patterns |
| Timeout visualization | ⚪ | NOT BUILT — no way to see which jobs are timing out or approaching limits |
| Job editing (not just delete/recreate) | ⚪ | NOT BUILT — cron modal is create/delete only |
| Test run with live output | ⚪ | Have "run now" button, but no live output stream — user can't see what happened |

### 🔴 CRITICAL — Cost Visibility

**Community problem**: Token costs compound silently, no visibility until the API bill arrives.

| What they need | Paw status | Gap |
|----------------|:---:|-----|
| Real-time token usage dashboard | ✅ | `usage.status` + `usage.cost` wired in Settings Usage section |
| Per-conversation cost tracking | ⚪ | NOT BUILT — no way to see "this chat session cost $0.47" |
| Model cost comparison | ⚪ | NOT BUILT — no side-by-side model pricing |
| Budget alerts / spending limits | ⚪ | NOT BUILT — no "warn me at $X" or "stop at $Y" |
| Cost per feature breakdown | ⚪ | NOT BUILT — heartbeat vs chat vs research vs cron breakdown |

### 🟡 HIGH — Multi-User / Multi-Agent Routing

**Community problem**: "Can two people share one bot with separate workspaces?" — routing is the gap.

| What they need | Paw status | Gap |
|----------------|:---:|-----|
| Multi-agent CRUD | ✅ | Fully wired in Foundry |
| Agent routing (channel → agent mapping) | ⚪ | NOT BUILT — need UI to configure which channel/sender routes to which agent |
| Per-user workspace selection | ⚪ | NOT BUILT — workspaces exist but no switching UI |
| Session → agent binding UI | ⚪ | NOT BUILT — sessions don't show which agent they belong to |

### 🟡 HIGH — Morning Brief / Proactive Features

**Community problem**: Most popular use case (ElevenLabs morning briefs), but requires workarounds.

| What they need | Paw status | Gap |
|----------------|:---:|-----|
| TTS for audio briefs | ⚪ | NOT BUILT — gateway supports TTS, methods typed, no UI |
| Cron job templates (morning brief preset) | ⚪ | NOT BUILT — one-click "Create Morning Brief" |
| Sub-agent spawn from cron | ⚪ | NOT BUILT — cron → agent chain |
| Voice output preference | ⚪ | NOT BUILT — per-channel/per-cron TTS toggle |

### 🟡 HIGH — Setup & Installation Polish

**Community problem**: npm global install bugs, path issues, Windows struggles.

| What they need | Paw status | Gap |
|----------------|:---:|-----|
| Bundled installer | 🔶 | Infrastructure exists, Node.js tarballs missing from `resources/node/` |
| Onboarding wizard | ✅ | Wired — wizard.start/next/cancel/status in Settings |
| Error recovery ("gateway crashed") | ⚪ | NOT BUILT — no crash detection, no auto-restart, no user guidance |
| Config validation before save | 🔶 | `config.schema` typed, not used for pre-save validation |
| Self-update | ✅ | Wired — "Update OpenClaw" button in Settings |

### 🟡 MEDIUM — Browser Automation

**Community problem**: "browser tasks. plz help" — agent can drive Chrome but no visibility.

| What they need | Paw status | Gap |
|----------------|:---:|-----|
| Browser status/control | ✅ | Wired — start/stop + status badge in Settings |
| Screenshot viewer | ⚪ | NOT BUILT — agent takes screenshots, user can't see them |
| Tab management (click, navigate, close) | ⚪ | NOT BUILT — tab list is read-only |

### 🟡 MEDIUM — Voice Features

**Community problem**: ElevenLabs morning briefs are beloved, but no desktop UI.

| What they need | Paw status | Gap |
|----------------|:---:|-----|
| TTS provider setup | ⚪ | NOT BUILT — `tts.providers / tts.setProvider` typed but no UI |
| Voice preview / test | ⚪ | NOT BUILT — `tts.convert` typed but no play button |
| Talk mode toggle | ⚪ | NOT BUILT — `talk.mode` typed but no UI |
| Wake word config | ⚪ | NOT BUILT — `voicewake.get/set` typed but no UI |

---

## Sprint Plan — Community Pain Points

Priority order based on community pain severity + implementation feasibility.

### Sprint 1: Cost & Memory Visibility (highest pain)

**Why first**: "Where did my money go?" and "it forgot everything" are the top two complaints.

| # | Feature | Gateway methods | Effort | Details |
|---|---------|----------------|--------|---------|
| 1 | **Usage Dashboard enhancement** | `usage.status`, `usage.cost` | S | Already wired — add per-model cost cards, session-level breakdown, refresh timer |
| 2 | ~~**Memory Context Meter**~~ | ~~`usage.status` (token counts)~~ | ~~M~~ | ✅ **DONE** — Progress bar in chat header, color-coded, auto-detect model context limit |
| 3 | ~~**Compaction indicator**~~ | ~~Listen for compaction events~~ | ~~M~~ | ✅ **DONE** — Yellow banner at 80% capacity, escalates at 95%, dismissible |
| 4 | **Per-conversation cost** | Track tokens per session locally | M | Accumulate `usage.status` deltas per session ID, show in chat header |
| 5 | **Budget alert** | Local threshold check | S | Settings input for spending limit, warn when `usage.cost` exceeds it |

### Sprint 2: Cron Reliability (silent failures → visible failures)

**Why second**: Cron is the second most-used feature, and silent failures erode trust.

| # | Feature | Gateway methods | Effort | Details |
|---|---------|----------------|--------|---------|
| 6 | **Cron job editor** | `cron.update` | M | Edit existing jobs (schedule, prompt, agent) instead of delete+recreate |
| 7 | **Run output viewer** | `cron.runs` + run detail | M | Expandable run rows showing output, errors, duration, timeout status |
| 8 | **Error highlighting** | `cron.runs` (error field) | S | Red rows for failed runs, error icon, filter by status |
| 9 | **Timeout visualization** | `cron.runs` (duration) | S | Progress bar or timer showing job duration vs configured timeout |

### Sprint 3: TTS & Voice (morning brief enabler)

**Why third**: Morning briefs are the #1 community use case, and all gateway methods are already typed.

| # | Feature | Gateway methods | Effort | Details |
|---|---------|----------------|--------|---------|
| 10 | **TTS Settings panel** | `tts.status`, `tts.providers`, `tts.setProvider`, `tts.enable/disable` | M | Provider picker, enable/disable toggle, voice selection |
| 11 | **TTS in Chat** | `tts.convert` | M | Play button on assistant messages, audio element, voice selector |
| 12 | **Cron template: Morning Brief** | `cron.create` (preset) | S | One-click "Create Morning Brief" with pre-filled schedule+prompt+TTS flag |

### Sprint 4: Multi-Agent Routing & Polish

**Why fourth**: Multi-agent CRUD exists but routing is the missing piece for shared setups.

| # | Feature | Gateway methods | Effort | Details |
|---|---------|----------------|--------|---------|
| 13 | **Agent routing config** | Agent + channel config | M | UI: channel → agent mapping table, default agent selector |
| 14 | **Session → agent binding** | Session metadata | S | Show which agent owns each session, filter sessions by agent |
| 15 | **Config validation** | `config.schema` | S | Validate config against schema before saving, show validation errors inline |
| 16 | **Error recovery** | Gateway health + restart | M | Detect gateway crash, offer auto-restart, show recovery guidance |

### Sprint 5: Browser & Memory Export

**Why last**: Lower pain severity, but still requested.

| # | Feature | Gateway methods | Effort | Details |
|---|---------|----------------|--------|---------|
| 17 | **Screenshot viewer** | `browser.status` (screenshots) | M | Display agent screenshots in browser panel, lightbox view |
| 18 | ~~**Memory export**~~ | ~~Tauri file dialog + `memory_search`~~ | ~~M~~ | ✅ **DONE** — Blob download, timestamped JSON, up to 500 memories |
| 19 | **Memory cost comparison** | `usage.cost` + memory config | S | Show embedding cost vs no-embedding, toggle with savings estimate |
