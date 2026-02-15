# Paw — Full Architecture, Status & Wiring Plan

> Last updated: 2026-02-15  
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

### 21. Usage Tracking ⚪ NOT BUILT
| Component | Status | Details |
|-----------|--------|--------|
| Usage status | ⚪ | `usage.status` — token counts, request counts |
| Cost breakdown | ⚪ | `usage.cost` — dollar cost per model/provider |

Critical for users on pay-per-use API keys. Paw has **zero** coverage.

---

### 22. Onboarding Wizard ⚪ NOT BUILT
| Component | Status | Details |
|-----------|--------|--------|
| Start wizard | ⚪ | `wizard.start` — begin guided setup |
| Step through | ⚪ | `wizard.next` — advance to next step |
| Cancel | ⚪ | `wizard.cancel` |
| Status | ⚪ | `wizard.status` — check wizard state |

OpenClaw's built-in guided setup flow. Could replace or supplement Paw's manual config form. **High priority** for non-technical users.

---

### 23. Browser Control ⚪ NOT BUILT
| Component | Status | Details |
|-----------|--------|--------|
| Browser request | ⚪ | `browser.request` — CDP Chrome control |

Agent-driven browser automation. Single method but powerful feature.

---

### 24. Self-Update ⚪ NOT BUILT
| Component | Status | Details |
|-----------|--------|--------|
| Update OpenClaw | ⚪ | `update.run` — update OpenClaw from within Paw |

One-click update for non-technical users. **High priority**.

---

### 25. Logs Viewer ⚪ NOT BUILT
| Component | Status | Details |
|-----------|--------|--------|
| Tail logs | ⚪ | `logs.tail` typed in gateway.ts but **never called** |

Real-time gateway log viewer for debugging. Could be a Settings tab.
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
| `src/gateway.ts` | 746 | **WebSocket gateway client** — Protocol v3, ~70 methods typed |
| `src/types.ts` | 514 | **TypeScript types** — gateway protocol types, ChatAttachment, UI types |
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
| `settings.ts` | 181 | Gateway config, logs |
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
| `exec.approvals.node.get` | ❌ | ❌ | NOT TYPED |
| `exec.approvals.node.set` | ❌ | ❌ | NOT TYPED |
| `exec.approval.request` | ❌ | ❌ | NOT TYPED |
| `exec.approval.waitDecision` | ❌ | ❌ | NOT TYPED |
| `exec.approval.resolve` | ✅ | ✅ | Approve/deny from modal + auto-deny for mail permissions |

#### Usage Tracking — ENTIRELY MISSING FROM PAW
| Method | In gateway.ts | Called from UI | Notes |
|--------|:---:|:---:|-------|
| `usage.status` | ❌ | ❌ | Token/cost usage stats |
| `usage.cost` | ❌ | ❌ | Billing/cost breakdown |

#### System / Presence
| Method | In gateway.ts | Called from UI | Notes |
|--------|:---:|:---:|-------|
| `system-presence` | ✅ | ❌ | Typed but not called — **no connected clients view** |
| `system-event` | ❌ | ❌ | NOT TYPED — trigger system event |
| `last-heartbeat` | ❌ | ❌ | NOT TYPED |
| `set-heartbeats` | ❌ | ❌ | NOT TYPED |

#### Onboarding Wizard — TYPED IN GATEWAY, NO UI
| Method | In gateway.ts | Called from UI | Notes |
|--------|:---:|:---:|-------|
| `wizard.start` | ✅ | ❌ | Typed, no UI |
| `wizard.next` | ✅ | ❌ | Typed, no UI |
| `wizard.cancel` | ✅ | ❌ | Typed, no UI |
| `wizard.status` | ✅ | ❌ | Typed, no UI |

#### Update — TYPED IN GATEWAY, NO UI
| Method | In gateway.ts | Called from UI | Notes |
|--------|:---:|:---:|-------|
| `update.run` | ✅ | ❌ | Typed, no UI |

#### Browser Control — TYPED IN GATEWAY, NO UI
| Method | In gateway.ts | Called from UI | Notes |
|--------|:---:|:---:|-------|
| `browser.status` | ✅ | ❌ | Typed, no UI |
| `browser.start` | ✅ | ❌ | Typed, no UI |
| `browser.stop` | ✅ | ❌ | Typed, no UI |

#### Direct Send
| Method | In gateway.ts | Called from UI | Notes |
|--------|:---:|:---:|-------|
| `send` | ✅ | ❌ | Typed but not called |

### All 18 Gateway Events

| Event | Consumed by Paw | Notes |
|-------|:---:|-------|
| `connect.challenge` | ✅ | Handshake nonce |
| `agent` | ✅ | Streaming deltas for chat/research |
| `chat` | ✅ | Final assembled messages |
| `presence` | ❌ | **Not consumed** — connected clients updates |
| `tick` | ❌ | **Not consumed** — periodic status ticks |
| `talk.mode` | ❌ | **Not consumed** — talk mode state changes |
| `shutdown` | ❌ | **Not consumed** — gateway shutting down gracefully |
| `health` | ❌ | **Not consumed** — health snapshot pushes |
| `heartbeat` | ❌ | **Not consumed** — heartbeat events |
| `cron` | ❌ | **Not consumed** — cron job fired/completed |
| `node.pair.requested` | ✅ | Consumed → pairing request card in Nodes sidebar |
| `node.pair.resolved` | ✅ | Consumed → refreshes pairing list |
| `node.invoke.result` | ✅ | Consumed → refreshes node list |
| `device.pair.requested` | ✅ | Consumed → refreshes Settings device list, shows toast |
| `device.pair.resolved` | ✅ | Consumed → refreshes Settings device list |
| `voicewake.changed` | ❌ | **Not consumed** — wake words updated |
| `exec.approval.requested` | ✅ | Approval modal + mail permission auto-deny |
| `exec.approval.resolved` | ❌ | **Not consumed** — approval resolved |

### Coverage Summary (Updated 2026-02-16)

| Category | Methods in OpenClaw | Methods typed in Paw | Methods called from UI | % Coverage |
|----------|:---:|:---:|:---:|:---:|
| Core/Health | 3 | 3 | 3 | **100%** ✅ |
| Channels | 4 | 4 | 4 | **100%** |
| Sessions | 6 | 6 | 6 | **100%** ✅ |
| Chat | 3 | 3 | 3 | **100%** |
| Agent | 10 | 10 | 10 | **100%** ✅ |
| Cron | 8 | 8 | 8 | **100%** ✅ |
| Skills | 4 | 4 | 4 | **100%** |
| Models | 1 | 1 | 1 | **100%** |
| Config | 5 | 5 | 4 | **100% typed, 80% UI** ✅ |
| TTS | 6 | 5 | 0 | 83% typed |
| Talk | 2 | 2 | 0 | **100% typed** |
| Voice Wake | 2 | 2 | 0 | **100% typed** |
| Nodes | 11 | 10 | 9 | **91%** ✅ |
| Devices | 5 | 5 | 5 | **100%** ✅ |
| Exec Approvals | 7 | 3 | 3 | 43% (gateway get/set + resolve wired, node approvals NOT TYPED) |
| Usage | 2 | 2 | 0 | **100% typed** |
| System | 4 | 1 | 0 | 25% |
| Wizard | 4 | 4 | 0 | **100% typed** |
| Update | 1 | 1 | 0 | **100% typed** |
| Browser | 3 | 3 | 0 | **100% typed** |
| Send/Agent | 2 | 2 | 0 | **100% typed** |
| **TOTAL** | **~90** | **~75** | **~42** | **~83% typed, ~47% UI wired** |

**Progress**: Gateway client now has ~83% of methods typed. Core/Health, Sessions, Cron, and Config are now **100% wired** (up from 33%/50%/86%/40%). Main remaining gap is voice/TTS UI + exec approval node methods.

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

#### 4b. Usage & Billing
16. **Usage dashboard** — Call `usage.status` + `usage.cost`, show token/cost breakdown

#### 4c. TTS (Text-to-Speech)
17. **TTS settings panel** — `tts.status`, `tts.providers`, enable/disable/setProvider
18. **TTS toggle in chat** — Enable TTS for responses, preview voices
19. **Convert button** — `tts.convert` next to assistant messages

#### 4d. Logs Viewer
20. **Logs tab in Settings** — `logs.tail` with auto-refresh, filterable

#### 4e. System Presence
21. **Connected clients card** — `system-presence` → show who/what is connected (devices, apps, CLI)

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
31. **Agent routing** — configure which channels/sessions route to which agent

#### 4j. Self-Update
32. **Update button** — `update.run` → update OpenClaw from Paw, show progress

#### 4k. Onboarding Wizard
33. **Wizard flow** — `wizard.start/next/cancel/status` → guided first-run setup
34. Could replace/supplement current manual setup form

#### 4l. Browser Control
35. **Browser panel** — `browser.request` → start/stop managed browser, view tabs, take screenshots

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
46. Add syntax highlighting to Build editor (CodeMirror)
47. Cron job editing (currently create/delete only)
48. Real knowledge graph (or remove the mock)
49. Export research reports
50. Chat image/file/attachment support (OpenClaw `agent` method supports `attachments` array)
51. Webhook configuration UI

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

**What works**: Chat (streaming + markdown + abort + mode selection + session management + retry + attachments + image preview), Research (full flow), Channels (+ per-channel setup forms), Automations, Skills, Models/Modes/Multi-Agent (CRUD + detail view), Memory (with setup), Settings (+ gateway logs + usage stats + connected clients + device pairing + tool approval toggle UI), Dashboard, Mail (full IMAP/SMTP setup via Himalaya + provider picker + credential vault + OS keychain + agent permissions + audit log + compose + inbox), Node Management (list + describe + invoke + pairing), Exec Approvals (live approval modal + resolve + permission enforcement + visual toggle config UI). The core gateway integration is solid and expanding.

**What's broken**: Build/Content chat responses still not routed, Code view is empty.

**What's missing entirely**: TTS (6 methods), Talk Mode (2), Voice Wake (2), Usage Tracking (2), Onboarding Wizard (4), Self-Update (1), Browser Control (1). That's **~18 gateway methods with zero UI coverage**.

**Coverage reality**: Paw calls **~44 of ~90 gateway methods** (**~49% UI wired, ~83% typed**). 8 of 18 gateway events are consumed (node.added, node.removed, node.pair.requested, node.pair.resolved, device.pair.requested, device.pair.resolved, exec.approval.requested, agent deltas). Core/Health, Sessions, Agent, Cron, and Config are now **100% wired to UI**. The gateway WebSocket client (`gateway.ts`) is well-structured, and every feature sprint proves that adding new methods is straightforward (add type -> add wrapper -> add UI).

**Security posture**: Mail credentials stored in OS keychain (macOS Keychain / libsecret), Himalaya config.toml chmod 600, passwords never returned to JS frontend, agent email actions enforced via per-account permission toggles, all activity logged to SQLite audit trail.

**Core insight**: Phases 1-17 moved Paw from a demo-quality shell (~26% coverage, broken wiring, empty views) to a functional desktop client (~47% UI wired / ~83% typed, real security, working mail, nodes, device pairing, visual tool approvals, full gateway status/config/session management). The remaining work is mostly Phase 4 "free features" (TTS/voice methods that just need UI) and Phase 6 polish.

**Priority for "works out of the box" goal**: Onboarding Wizard + Self-Update + Usage Tracking are the highest impact remaining items for non-technical users. Exec Approvals config UI is now fully wired with visual toggles.
