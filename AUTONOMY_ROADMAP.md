# OpenPawz Platform Roadmap — Agent Autonomy & Extensibility

> Close the gap with OpenClaw-style platforms by enabling autonomous agent scripting,
> dynamic tool discovery, and external event triggers — without undoing any of the
> enterprise hardening work (530 tests, 3-job CI, 0 clippy warnings, 0 CVEs).

**Golden rule: all 530 existing tests must pass at every step. New features get new tests.**

---

## Will this match OpenClaw?

Yes — and exceed it. OpenClaw is a server-side gateway that gives agents unrestricted tool access.
These 6 phases give OpenPawz the same agent freedom, but delivered through a native desktop app
with optional security layers instead of none. The user chooses their risk level.

| Capability | OpenClaw | OpenPawz today | OpenPawz after roadmap |
|---|---|---|---|
| Agent runs arbitrary code | ✅ Always | ⚠️ Per-call approval | ✅ Auto-approve mode |
| Agent writes + runs scripts in loop | ✅ Unrestricted | ❌ Blocked by HIL | ✅ Docker sandbox + auto-approve |
| Dynamic tool discovery (MCP) | ✅ Plugin servers | ❌ Zero code | ✅ MCP client |
| External systems trigger agents | ✅ API endpoints | ⚠️ WhatsApp only | ✅ Generic webhook |
| Remote channel code execution | ✅ No restrictions | ❌ Auto-denied | ✅ Configurable per-channel |
| Community marketplace | ⚠️ ClawHub (48% junk) | ⚠️ skills.sh (Tier 1 only) | ✅ PawzHub (3 tiers, CI-validated, MCP servers) |
| Security layers | ❌ None | ✅ 7 layers | ✅ 7 layers (opt-out per agent) |

---

## High-Level TODO

- [x] **Phase A** — Auto-approve mode per agent *(small, high impact)* ✅
- [x] **Phase B** — Session-level approval *(small, good UX middle ground)* ✅ (already existed via session override in hil_modal.ts)
- [x] **Phase C** — Per-channel dangerous tool policy *(small)* ✅
- [x] **Phase D** — Generic inbound webhook endpoint *(medium)* ✅
- [x] **Phase E** — MCP client + dynamic tool registry *(large, highest strategic value)* ✅
- [ ] **Phase F** — PawzHub marketplace *(large, builds on all previous phases)*

---

## Phase A — Auto-Approve Mode Per Agent

**Goal:** Let users mark an agent as "fully autonomous" — all tools auto-approved, no HIL popups.

### What exists today
- `auto_approved_tools` list in `agent_loop/mod.rs` (~35 safe tools)
- Per-call HIL: non-safe tools emit `ToolRequest` event, wait on oneshot channel
- Agent policies in frontend: unrestricted/denylist/allowlist modes
- Trading `auto_approve` flag already exists as precedent

### What to build

**Rust backend (`src-tauri/src/`):**
- [x] Add `auto_approve_all: bool` field to `ChatRequest` in `atoms/types.rs` (with `#[serde(default)]`)
- [x] Add `ToolAutoApproved` variant to `EngineEvent` for audit trail
- [x] In `agent_loop/mod.rs` — HIL gate checks `auto_approve_all` flag before approval
- [x] If true, skip `ToolRequest` event — execute immediately with audit log
- [x] Cron tasks pass `false` (safe default); per-chat opt-in via `ChatRequest`
- [x] Emit `EngineEvent::ToolAutoApproved` with session_id, run_id, tool_name, tool_call_id

**TypeScript frontend (`src/`):**
- [x] Add toggle in agent editor (Advanced tab) with ⚠️ warning
- [x] Add toggle in Foundry mode editor with ⚠️ warning
- [x] ⚡ AUTO badge on Foundry mode cards when auto-approve is active
- [x] DB migration v3 adds `auto_approve_all` column to `agent_modes` table
- [x] Bridge reads `agentProfile.autoApproveAll` and sets `auto_approve_all` on `ChatRequest`
- [x] `tool_auto_approved` event translated and displayed in chat stream

**Tests:**
- [ ] Rust: test that `auto_approve_all=true` skips HIL
- [ ] Rust: test that `auto_approve_all=false` still requires approval (existing behavior)
- [ ] Rust: test cron task passes `false` by default
- [ ] TypeScript: test warning dialog renders, toggle state persists

**Files to modify:**
- `src-tauri/src/engine/agent_loop/mod.rs` — HIL gate logic
- `src-tauri/src/engine/types.rs` — agent config struct
- `src-tauri/src/commands/agents.rs` — CRUD commands
- `src/views/agents.ts` or `src/views/settings-agent-defaults.ts` — UI toggle
- DB migration for new column

---

## Phase B — Session-Level Approval ("Approve All For This Session")

**Goal:** Less scary middle ground — user approves once, all subsequent tool calls in that session auto-approve.

### What to build

**Already implemented (discovered during audit):**
- [x] `activateSessionOverride(mins)` / `getSessionOverrideRemaining()` in `security.ts`
- [x] "Allow all…" dropdown button with 30min / 1hr / 2hr options in the HIL approval modal
- [x] Session override banner in `index.html`
- [x] Auto-approve check before showing modal — skips HIL when session override is active
- [x] Reset on timeout / page reload

**Files to modify:**
- `src-tauri/src/engine/agent_loop/mod.rs` — session state + HIL gate
- `src/components/molecules/` — tool approval modal
- `src/engine/` — event handling for new approval type

---

## Phase C — Per-Channel Dangerous Tool Policy

**Goal:** Let power users enable `exec` and other dangerous tools on specific channel bridges.

### What exists today
- `channels/agent.rs` auto-denies all non-safe tools for remote channels
- This is a hardcoded behavior, not configurable

### What to build

**Rust backend:**
- [x] Add `allow_dangerous_tools: bool` (with `#[serde(default)]`) to all 11 channel config structs
  - discord, matrix, nextcloud, whatsapp, irc, telegram, slack, mattermost, nostr, twitch, webchat
- [x] In `channels/agent.rs` `run_channel_agent()` — new `allow_dangerous_tools: bool` param
- [x] Auto-approver conditionally sends `true`/`false` based on flag
- [x] Log warnings when dangerous tools auto-approved via remote channel
- [x] All 11 channel call sites updated to pass `config.allow_dangerous_tools`

**TypeScript frontend:**
- [x] Universal "Allow dangerous tools" toggle in Advanced section of every channel setup modal
- [x] ⚠️ Warning: "When enabled, side-effect tools (file write, shell, etc.) run without human approval for messages from this channel."
- [x] Existing config value loaded and checkbox pre-populated
- [x] Saved in both Telegram custom path and generic channel save path

**Tests:**
- [ ] Rust: test that `allow_dangerous_tools=false` still auto-denies (existing behavior)
- [ ] Rust: test that `allow_dangerous_tools=true` + agent auto-approve allows execution
- [ ] Rust: test that `allow_dangerous_tools=true` alone (without agent auto-approve) still requires HIL

**Files to modify:**
- `src-tauri/src/engine/channels/agent.rs` — auto-approver logic
- `src-tauri/src/engine/channels/` — per-bridge config types
- `src/views/channels.ts` — settings UI

---

## Phase D — Generic Inbound Webhook Endpoint

**Goal:** Let external systems (Zapier, n8n, GitHub Actions, cron jobs) POST to a URL and trigger an agent run.

### What exists today
- WhatsApp webhook listener on port 8086 (hardcoded Evolution API format)
- Webchat WebSocket server with HTTP
- Outbound `webhook_send` tool (agent → external)
- Tailscale Funnel support for exposing services

### What to build

**Rust backend:**
- [x] New `webhook` module — lightweight HTTP listener (raw `tokio::net::TcpListener`, same pattern as webchat/whatsapp)
- [x] Configurable port (default 3940), bind address, auto-generated UUID auth token
- [x] `POST /webhook/:agent_id` — accepts JSON body, triggers agent turn with body as user message
- [x] `POST /webhook/:agent_id/tool/:tool_name` — route reserved (501 Not Implemented, future)
- [x] Rate limiting per IP (token-bucket, 60 req/min default, configurable)
- [x] Response: returns agent's text response synchronously in JSON body
- [x] 6 Tauri commands: start, stop, status, get_config, set_config, regenerate_token
- [x] `GET /webhook/health` — unauthenticated liveness probe
- [x] CORS preflight (OPTIONS) support
- [x] `webhook-status` and `webhook-activity` event emission
- [x] 7 unit tests (config default, rate limiter, request/response serialization)

**TypeScript frontend:**
- [x] Webhook settings panel in Settings → Webhook tab (between Tailscale and Security)
- [x] Status card (running/stopped indicator + start/stop button)
- [x] Config form: bind address, port, auth token (show/hide, copy, regenerate), default agent ID, rate limit, allow_dangerous_tools toggle
- [x] curl example box with copy button
- [x] `WebhookConfig` interface in `engine/atoms/types.ts`
- [x] 6 IPC methods in `PawEngineClient`

**Tests:**
- [x] Rust: 7 unit tests (config default, rate limiter within/unlimited/separate IPs, request deserialization full/minimal, response serialization)
- [ ] Rust: integration test — full HTTP request → agent response (needs mock agent)
- [ ] TypeScript: test settings UI renders correctly

**Files created:**
- `src-tauri/src/engine/webhook.rs` — webhook server module (~375 lines, 7 tests)
- `src-tauri/src/commands/webhook.rs` — 6 Tauri IPC commands
- `src/views/settings-webhook/index.ts` — public API
- `src/views/settings-webhook/molecules.ts` — DOM rendering + IPC
- `src/views/settings-webhook/atoms.ts` — pure helpers

**Files modified:**
- `src-tauri/src/lib.rs` — register 6 webhook commands in `generate_handler![]`
- `src-tauri/src/engine/mod.rs` — `pub mod webhook`
- `src-tauri/src/commands/mod.rs` — `pub mod webhook`
- `src/engine/atoms/types.ts` — `WebhookConfig` interface
- `src/engine/molecules/ipc_client.ts` — 6 webhook IPC methods
- `src/views/settings-tabs.ts` — webhook case in `loadActiveSettingsTab()`
- `index.html` — webhook tab button + panel

---

## Phase E — MCP Client + Dynamic Tool Registry

**Goal:** Let agents discover and use tools from external MCP servers at runtime. This is the strategic game-changer — it turns OpenPawz from a closed tool set into an open platform.

### What is MCP?
Anthropic's Model Context Protocol — a JSON-RPC standard where tool servers expose capabilities.
Users install MCP servers (e.g., `@modelcontextprotocol/server-github`) and agents discover
available tools at runtime. No Rust code changes needed to add new capabilities.

### What to build

**Rust backend:**
- [x] MCP client implementation — JSON-RPC over stdio or HTTP+SSE transport
- [x] `McpServerConfig` type — name, command/URL, args, env vars
- [x] Server lifecycle management — spawn/connect on startup, health checks, restart on crash
- [x] `tools/list` — query connected servers for available tools
- [x] `tools/call` — proxy tool calls from agent loop to MCP server
- [x] Dynamic tool injection — merge MCP tools into agent's available tool list per-session
- [x] Tool schema conversion — MCP tool schemas → OpenPawz tool format for LLM
- [ ] Per-agent MCP server assignment (not all agents need all servers)
- [ ] Credential passthrough — inject skill credentials as MCP server env vars

**TypeScript frontend:**
- [x] MCP settings panel — add/remove/configure servers
- [x] Server status indicators (connected, error, disconnected)
- [ ] Per-agent MCP server selection
- [ ] Tool browser — show dynamically discovered tools with schemas
- [ ] "Add from registry" — search community MCP servers

**Tests:**
- [x] Rust: 18 unit tests (8 types, 3 transport, 3 client, 4 registry)
- [ ] Rust: test MCP client connects and lists tools (mock server)
- [ ] Rust: test tool call proxy — request goes to server, response comes back
- [ ] Rust: test server crash recovery
- [ ] TypeScript: test MCP config UI, server status display

**Files created:**
- `src-tauri/src/engine/mcp/mod.rs` — module root + re-exports
- `src-tauri/src/engine/mcp/types.rs` — MCP protocol types (JSON-RPC 2.0, initialize, tools/list, tools/call), 8 tests
- `src-tauri/src/engine/mcp/transport.rs` — stdio process transport with Content-Length framing, 3 tests
- `src-tauri/src/engine/mcp/client.rs` — MCP client (initialize handshake, tool discovery, tool execution), 3 tests
- `src-tauri/src/engine/mcp/registry.rs` — multi-server lifecycle + tool dispatch + namespacing, 4 tests
- `src-tauri/src/commands/mcp.rs` — 8 Tauri IPC commands (list/save/remove/connect/disconnect/status/refresh/connect-all)
- `src/views/settings-mcp/index.ts` — public API
- `src/views/settings-mcp/molecules.ts` — DOM rendering + IPC (server list, add/edit forms, connect/disconnect)
- `src/views/settings-mcp/atoms.ts` — pure helpers

**Files modified:**
- `src-tauri/src/engine/state.rs` — added `mcp_registry: Arc<tokio::sync::Mutex<McpRegistry>>`
- `src-tauri/src/engine/tools/mod.rs` — added `mcp_tools()` helper + MCP dispatch in `execute_tool()`
- `src-tauri/src/engine/chat.rs` — MCP tools injected into `build_chat_tools()`
- `src-tauri/src/commands/task.rs` — MCP tools injected into task tool building
- `src-tauri/src/engine/channels/agent.rs` — MCP tools injected into channel agent tools
- `src-tauri/src/engine/orchestrator/mod.rs` — MCP tools injected into project boss tools
- `src-tauri/src/engine/orchestrator/sub_agent.rs` — MCP tools injected into project worker tools
- `src-tauri/src/engine/mod.rs` — `pub mod mcp`
- `src-tauri/src/commands/mod.rs` — `pub mod mcp`
- `src-tauri/src/lib.rs` — registered 8 MCP commands in `generate_handler![]`
- `src/engine/atoms/types.ts` — `McpServerConfig`, `McpServerStatus`, `McpTransport` types
- `src/engine/molecules/ipc_client.ts` — 8 MCP IPC methods
- `src/views/settings-tabs.ts` — MCP case in `loadActiveSettingsTab()`
- `index.html` — MCP Servers tab button + panel

---

## Phase F — PawzHub Marketplace

**Goal:** Turn the documented-but-unimplemented PawzHub vision into a working marketplace where users create, share, and install Skills, Integrations, Extensions, and MCP server configs.

> PawzHub is already fully designed in `docs/docs/guides/pawzhub.md`. This phase implements it.
> Each sub-phase is independently shippable — start with the TOML loader, end with one-click publish.

### Why it depends on earlier phases

| Phase | What it gives PawzHub |
|---|---|
| A — Auto-approve | Community skills that call APIs or run CLI actually work without 30 approval clicks |
| B — Session approve | Users can try a new skill with one "Approve All" instead of per-call |
| C — Channel policy | Shareable "channel recipes" (e.g., Slack deployment bot that uses `exec`) |
| D — Webhooks | Shareable "webhook workflows" — skill installs a webhook endpoint + agent instructions |
| E — MCP client | **The big unlock.** PawzHub goes from sharing prompt snippets to sharing real MCP server configs with typed tools |

### F.1 — TOML Manifest Loader *(prerequisite for everything else)*

**What exists:** 40+ built-in skills compiled into Rust binary. Community skills are `SKILL.md` prompt files only.

**What to build:**
- [ ] Scan `~/.paw/skills/*/pawz-skill.toml` on startup
- [ ] Parse TOML into `SkillDefinition` (reuse existing struct + new fields)
- [ ] Hot-reload: watch directory for changes, re-parse without restart
- [ ] Credential fields from TOML → vault UI (same AES-GCM flow as built-ins)
- [ ] `[instructions]` text injected into agent prompt (same as built-ins)
- [ ] `[binary]` detection — check `$PATH` for required CLI tools
- [ ] Per-agent skill scoping (assign TOML skills to specific agents)

**Files to create:**
- `src-tauri/src/engine/skills/toml_loader.rs` — manifest parser + directory scanner

**Files to modify:**
- `src-tauri/src/engine/skills/mod.rs` — merge TOML skills with builtins
- `src-tauri/src/engine/skills/prompt.rs` — include TOML skills in prompt building

### F.2 — Dashboard Widgets

**What exists:** Nothing — the `[widget]` section in TOML manifests is documented but not rendered.

**What to build:**
- [ ] `skill_output` tool — agent persists structured JSON to `skill_outputs` table
- [ ] Widget renderer — 5 types: status, metric, table, log, kv (as documented in pawzhub.md)
- [ ] Today/Dashboard view shows widget cards from enabled skills
- [ ] Auto-refresh: `refresh` interval from manifest triggers periodic agent re-run
- [ ] Widget field types: text, number, badge, datetime, percentage, currency

**Files to create:**
- `src-tauri/src/engine/tools/skill_output.rs` — the `skill_output` tool function
- `src/components/molecules/skill-widget.ts` — widget card renderer

**Files to modify:**
- `src/views/today.ts` — render skill widgets on dashboard
- `src-tauri/src/engine/tools/mod.rs` — register `skill_output` tool

### F.3 — MCP Server Sharing *(requires Phase E)*

**What exists:** After Phase E, agents can connect to MCP servers. But users configure them manually.

**What to build:**
- [ ] New `[mcp]` section in `pawz-skill.toml` — declares an MCP server config
- [ ] Fields: `command`, `args`, `env`, `transport` (stdio/sse), `url`
- [ ] On skill install, auto-register the MCP server with the Phase E registry
- [ ] On skill uninstall, remove the MCP server
- [ ] Credentials from `[[credentials]]` injected as MCP server env vars

```toml
# Example: a PawzHub skill that bundles an MCP server
[skill]
id = "github-mcp"
name = "GitHub (MCP)"
version = "1.0.0"
author = "openpawz"
category = "development"
description = "Full GitHub API via MCP — issues, PRs, repos, actions"

[[credentials]]
key = "GITHUB_TOKEN"
label = "Personal Access Token"
required = true

[mcp]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
transport = "stdio"

[instructions]
text = "GitHub tools are available via MCP. Use them directly."
```

**Files to modify:**
- `src-tauri/src/engine/skills/toml_loader.rs` — parse `[mcp]` section
- `src-tauri/src/engine/mcp/registry.rs` — auto-register from skill install

### F.4 — PawzHub Registry + In-App Browser

**What exists:** Search uses external `skills.sh` API. Install fetches `SKILL.md` from GitHub.

**What to build:**
- [ ] Create `elisplash/pawzhub` GitHub repo with `registry.json`
- [ ] `registry.json` schema: array of `{id, name, description, author, category, version, tier, source_repo, mcp}`
- [ ] GitHub Action: validate PRs (TOML syntax, unique ID, safe format, semver)
- [ ] GitHub Action: rebuild `registry.json` on merge to main
- [ ] In-app browser fetches `registry.json` (replaces/supplements skills.sh)
- [ ] Tier badges: 🔵 Skill, 🟣 Integration, 🟡 Extension, 🔴 MCP Server
- [ ] One-click install: download `pawz-skill.toml` → `~/.paw/skills/{id}/`
- [ ] "Verified" badge for skills tested with the in-app wizard

**Files to create:**
- `src-tauri/src/engine/skills/community/pawzhub.rs` — registry client

**Files to modify:**
- `src-tauri/src/engine/skills/community/search.rs` — add PawzHub as search source
- `src/views/settings-skills/community.ts` — tier badges, MCP indicator

### F.5 — In-App Creation Wizard + One-Click Publish

**What exists:** Nothing — skill creation is manual TOML editing.

**What to build:**
- [ ] Step-by-step wizard: Basic Info → Credentials → Instructions → Widget → MCP → Test → Publish
- [ ] Template starters: REST API, CLI Tool, Web Scraper, MCP Server
- [ ] AI-assisted creation: user says "Create a skill for Notion" → agent generates TOML
- [ ] Live test: enable skill, run agent, verify it works
- [ ] Export: save `pawz-skill.toml` locally
- [ ] Publish: open pre-filled GitHub PR on `elisplash/pawzhub`

**Files to create:**
- `src/views/skill-wizard.ts` — creation wizard UI
- `src-tauri/src/commands/skill_wizard.rs` — TOML generation + GitHub PR

### F.6 — Extensions (Tier 3) — Custom Views + Storage

**What exists:** Nothing — Extension tier is documented but unimplemented.

**What to build:**
- [ ] `[view]` section in TOML — declares a custom sidebar tab
- [ ] `[storage]` section — persistent key-value store per skill
- [ ] View renderer: skill output rendered as a full sidebar tab (not just a widget card)
- [ ] Storage API: `skill_store_set`, `skill_store_get`, `skill_store_list` tools
- [ ] Extension isolation: each extension's storage is namespaced

**Files to create:**
- `src-tauri/src/engine/tools/skill_storage.rs` — persistent KV store tools
- `src/views/extension-view.ts` — custom sidebar tab renderer

---

## Implementation Rules

1. **All 530 existing tests must pass at every commit.** No exceptions.
2. **New features get new tests.** Every phase adds to the test count.
3. **CI must stay green.** `cargo clippy -D warnings`, `cargo audit`, `npm audit` — all clean.
4. **No breaking changes to existing IPC commands.** New commands only, or additive changes.
5. **Database changes use migrations.** New columns with defaults, never drop columns.
6. **Security is opt-out, not opt-in.** Defaults remain safe. Users enable danger explicitly.
7. **Every dangerous feature shows a warning.** User must acknowledge risk before enabling.

---

## Estimated Timeline

| Phase | Effort | Dependencies |
|-------|--------|-------------|
| A — Auto-approve mode | 1-2 days | None |
| B — Session-level approval | 1 day | None (independent of A) |
| C — Per-channel tool policy | 1 day | Phase A (uses same flag) |
| D — Inbound webhooks | 3-5 days | None |
| E — MCP client | 1-2 weeks | None (but most valuable after A) |
| F.1 — TOML manifest loader | 3-5 days | None |
| F.2 — Dashboard widgets | 3-5 days | F.1 |
| F.3 — MCP server sharing | 2-3 days | E + F.1 |
| F.4 — Registry + in-app browser | 3-5 days | F.1 |
| F.5 — Creation wizard + publish | 3-5 days | F.1 + F.4 |
| F.6 — Extensions (Tier 3) | 1-2 weeks | F.1 + F.2 |

Phases A, B, and C combined unlock the "agent writes its own scripts" vision.
Phase E is the strategic moat — it turns OpenPawz into a platform, not just an app.
Phase F is the ecosystem play — it turns OpenPawz users into OpenPawz contributors.
