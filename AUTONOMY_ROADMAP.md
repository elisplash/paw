# OpenPawz Platform Roadmap — Agent Autonomy & Extensibility

> Close the gap with OpenClaw-style platforms by enabling autonomous agent scripting,
> dynamic tool discovery, and external event triggers — without undoing any of the
> enterprise hardening work (530 tests, 3-job CI, 0 clippy warnings, 0 CVEs).

**Golden rule: all 530 existing tests must pass at every step. New features get new tests.**

---

## Will this match OpenClaw?

Yes — and exceed it. OpenClaw is a server-side gateway that gives agents unrestricted tool access.
These 5 features give OpenPawz the same agent freedom, but delivered through a native desktop app
with optional security layers instead of none. The user chooses their risk level.

| Capability | OpenClaw | OpenPawz today | OpenPawz after roadmap |
|---|---|---|---|
| Agent runs arbitrary code | ✅ Always | ⚠️ Per-call approval | ✅ Auto-approve mode |
| Agent writes + runs scripts in loop | ✅ Unrestricted | ❌ Blocked by HIL | ✅ Docker sandbox + auto-approve |
| Dynamic tool discovery (MCP) | ✅ Plugin servers | ❌ Zero code | ✅ MCP client |
| External systems trigger agents | ✅ API endpoints | ⚠️ WhatsApp only | ✅ Generic webhook |
| Remote channel code execution | ✅ No restrictions | ❌ Auto-denied | ✅ Configurable per-channel |
| Security layers | ❌ None | ✅ 7 layers | ✅ 7 layers (opt-out per agent) |

---

## High-Level TODO

- [ ] **Phase A** — Auto-approve mode per agent *(small, high impact)*
- [ ] **Phase B** — Session-level approval *(small, good UX middle ground)*
- [ ] **Phase C** — Per-channel dangerous tool policy *(small)*
- [ ] **Phase D** — Generic inbound webhook endpoint *(medium)*
- [ ] **Phase E** — MCP client + dynamic tool registry *(large, highest strategic value)*

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
- [ ] Add `auto_approve_all: bool` field to agent config (DB schema + types)
- [ ] In `agent_loop/mod.rs` — before HIL gate, check agent's `auto_approve_all` flag
- [ ] If true, skip `ToolRequest` event — execute immediately
- [ ] Cron task execution inherits agent's `auto_approve_all` setting
- [ ] Emit `EngineEvent::ToolAutoApproved` for audit trail (log which tool was auto-approved)

**TypeScript frontend (`src/`):**
- [ ] Add toggle in agent settings UI with warning dialog
- [ ] Warning text: "This agent will execute all tools without asking — including file writes, shell commands, and API calls. Use with container sandbox enabled."
- [ ] Show visual indicator on agent card when auto-approve is active (e.g., yellow border)

**Tests:**
- [ ] Rust: test that `auto_approve_all=true` skips HIL
- [ ] Rust: test that `auto_approve_all=false` still requires approval (existing behavior)
- [ ] Rust: test cron task respects agent's auto-approve setting
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

**Rust backend:**
- [ ] Add `session_auto_approved: bool` to in-memory session state (not persisted)
- [ ] New `EngineEvent::ToolRequestApproveAll` variant — frontend can send "approve all" response
- [ ] When received, set `session_auto_approved = true` for that session
- [ ] HIL gate checks session flag before emitting `ToolRequest`

**TypeScript frontend:**
- [ ] Add "Approve All" button alongside "Approve" / "Deny" in tool approval modal
- [ ] Show indicator in chat header when session is in auto-approve mode
- [ ] Reset on new session / page reload

**Tests:**
- [ ] Rust: test session flag enables auto-approve for subsequent calls
- [ ] Rust: test flag resets on new session
- [ ] TypeScript: test "Approve All" button sets session state

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
- [ ] Add `allow_dangerous_tools: bool` to channel bridge config (default: `false`)
- [ ] In `channels/agent.rs` auto-approver, check bridge config before auto-denying
- [ ] If `allow_dangerous_tools=true` AND agent has `auto_approve_all=true`, approve tool
- [ ] Log warnings when dangerous tools execute via remote channel

**TypeScript frontend:**
- [ ] Add toggle in channel settings with warning
- [ ] Warning: "Enabling this allows remote users to trigger shell commands, file writes, and API calls through this channel. Only enable on private channels you control."

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
- [ ] New `webhook_server` module — lightweight HTTP listener (use `axum` or extend existing `hyper` usage)
- [ ] Configurable port, optional TLS, optional auth token
- [ ] `POST /webhook/:agent_id` — accepts JSON body, triggers agent turn with body as user message
- [ ] `POST /webhook/:agent_id/tool/:tool_name` — triggers specific tool execution
- [ ] Rate limiting per IP / per agent
- [ ] Response: returns agent's text response (synchronous) or job ID (async)
- [ ] Tauri command to start/stop webhook server

**TypeScript frontend:**
- [ ] Webhook settings panel — enable/disable, port, auth token display
- [ ] Show URL + curl example for each agent
- [ ] Activity log of incoming webhook hits

**Tests:**
- [ ] Rust: test webhook receives POST, routes to correct agent
- [ ] Rust: test auth token validation (reject unauthorized)
- [ ] Rust: test rate limiting
- [ ] Rust: test agent response returned correctly

**Files to create:**
- `src-tauri/src/engine/webhook_server.rs` — new module
- `src-tauri/src/commands/webhook.rs` — Tauri IPC commands

**Files to modify:**
- `src-tauri/src/lib.rs` — register commands, start server
- `src-tauri/src/engine/mod.rs` — module declaration
- `src/views/settings.ts` or new `src/views/webhooks.ts` — UI

---

## Phase E — MCP Client + Dynamic Tool Registry

**Goal:** Let agents discover and use tools from external MCP servers at runtime. This is the strategic game-changer — it turns OpenPawz from a closed tool set into an open platform.

### What is MCP?
Anthropic's Model Context Protocol — a JSON-RPC standard where tool servers expose capabilities.
Users install MCP servers (e.g., `@modelcontextprotocol/server-github`) and agents discover
available tools at runtime. No Rust code changes needed to add new capabilities.

### What to build

**Rust backend:**
- [ ] MCP client implementation — JSON-RPC over stdio or HTTP+SSE transport
- [ ] `McpServerConfig` type — name, command/URL, args, env vars
- [ ] Server lifecycle management — spawn/connect on startup, health checks, restart on crash
- [ ] `tools/list` — query connected servers for available tools
- [ ] `tools/call` — proxy tool calls from agent loop to MCP server
- [ ] Dynamic tool injection — merge MCP tools into agent's available tool list per-session
- [ ] Tool schema conversion — MCP tool schemas → OpenPawz tool format for LLM
- [ ] Per-agent MCP server assignment (not all agents need all servers)
- [ ] Credential passthrough — inject skill credentials as MCP server env vars

**TypeScript frontend:**
- [ ] MCP settings panel — add/remove/configure servers
- [ ] Server status indicators (connected, error, disconnected)
- [ ] Per-agent MCP server selection
- [ ] Tool browser — show dynamically discovered tools with schemas
- [ ] "Add from registry" — search community MCP servers

**Tests:**
- [ ] Rust: test MCP client connects and lists tools (mock server)
- [ ] Rust: test tool call proxy — request goes to server, response comes back
- [ ] Rust: test dynamic tool merging into agent tool list
- [ ] Rust: test server crash recovery
- [ ] Rust: test per-agent server filtering
- [ ] TypeScript: test MCP config UI, server status display

**Files to create:**
- `src-tauri/src/engine/mcp/mod.rs` — module root
- `src-tauri/src/engine/mcp/client.rs` — JSON-RPC client
- `src-tauri/src/engine/mcp/transport.rs` — stdio + HTTP+SSE transports
- `src-tauri/src/engine/mcp/types.rs` — protocol types
- `src-tauri/src/engine/mcp/registry.rs` — server lifecycle management
- `src-tauri/src/commands/mcp.rs` — Tauri IPC commands
- `src/views/mcp.ts` or section in `src/views/settings.ts` — UI

**Files to modify:**
- `src-tauri/src/engine/tools/mod.rs` — merge MCP tools into `builtins()`
- `src-tauri/src/engine/agent_loop/mod.rs` — tool dispatch includes MCP tools
- `src-tauri/src/lib.rs` — register MCP commands

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

Phases A, B, and C combined unlock the "agent writes its own scripts" vision.
Phase E is the strategic moat — it turns OpenPawz into a platform, not just an app.
