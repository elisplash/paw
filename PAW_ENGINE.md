# Paw Agent Engine — Implementation Status

> Last updated: 2026-02-17

## What This Is

Paw now has a **native Rust agent engine** embedded directly in the Tauri backend. It replaces the OpenClaw WebSocket gateway with direct AI API calls, eliminating all networking/JSON/auth issues between the two systems.

**Old architecture:** Frontend → WebSocket → OpenClaw Gateway (Node.js) → AI APIs
**New architecture:** Frontend → Tauri IPC (`invoke()`) → Rust Engine → AI APIs directly

Zero network hop, zero open ports, zero auth tokens in engine mode.

---

## Current Status: Phase 2 COMPLETE ✅

Phase 1 (core engine) was completed 2026-02-16. Phase 2 (security, session management, token metering, error handling, attachments) completed 2026-02-17.

### Phase 2 Features

#### P1: Human-in-the-Loop (HIL) Tool Approval ✅
- **Rust:** `PendingApprovals` map with `tokio::sync::oneshot` channels in `EngineState`. Agent loop emits `ToolRequest` event and pauses until frontend resolves via `engine_approve_tool` command. Configurable timeout (`tool_timeout_secs`, default 120s).
- **Frontend:** `onEngineToolApproval()` / `resolveEngineToolApproval()` in engine-bridge.ts. Full security pipeline in main.ts mirrors gateway approval flow: risk classification (critical/high/medium/low), allowlist/denylist checks, auto-deny for privilege escalation & critical commands, read-only project mode enforcement, session-level overrides, network audit for fetch calls.
- **UX:** Reuses existing approval modal (`#approval-modal`) with same Allow/Deny/Always Allow UI.

#### P2: Session Management (Dual-Mode) ✅
- `loadSessions()` routes to `pawEngine.sessionsList()` in engine mode
- `loadChatHistory()` routes to `pawEngine.chatHistory()` in engine mode
- Session rename → `pawEngine.sessionRename()` in engine mode
- Session delete → `pawEngine.sessionDelete()` in engine mode

#### P3: Token Metering ✅
- **Rust:** `TokenUsage` struct (`input_tokens`, `output_tokens`, `total_tokens`) added to `StreamChunk` and `EngineEvent::Complete`
- **OpenAI:** Parsed from `usage.prompt_tokens` / `usage.completion_tokens` in final SSE chunk (enabled via `stream_options.include_usage`)
- **Anthropic:** Parsed from `message_start` (`input_tokens`) and `message_delta` (`output_tokens`) events
- **Google:** Parsed from `usageMetadata.promptTokenCount` / `candidatesTokenCount`
- **Accumulation:** Agent loop sums usage across all chunks per turn, forwards total in `Complete` event
- **Frontend:** Usage data forwarded through bridge in lifecycle end event

#### P4: Error Handling & Retry ✅
- **Exponential backoff** for all 3 providers: max 3 retries, delays 1s → 2s → 4s
- **Retryable status codes:** 429 (rate limit), 500, 502, 503, 529
- **Non-retryable errors** (401, 403, 404, etc.) fail immediately with clear error messages
- **Provider-level** retry wraps the HTTP request + SSE stream setup

#### P5: Attachment Support ✅
- **Rust:** `ChatAttachment` struct (mime_type, content) added to `ChatRequest`
- **OpenAI format:** Attachments converted to `image_url` content blocks with `data:{mime};base64,{content}` URIs in `format_messages()`
- **Bridge:** `engineChatSend()` passes attachments through from frontend chat options
- **Commands:** `engine_chat_send` parses attachment objects from frontend and prepends as `ContentBlock::ImageUrl` blocks

### Phase 1 Features (Baseline)
- **Dual-mode switching** — Settings → General → Runtime Mode → Engine/Gateway
- **Google Gemini** — tested and working (gemini-2.0-flash confirmed)
- **OpenAI-compatible** — OpenAI, OpenRouter, Ollama, Custom endpoints
- **Anthropic** — Claude models
- **SSE streaming** — Real-time token streaming to chat UI
- **Tool definitions** — exec, fetch, read_file, write_file tools sent to model
- **Tool execution** — Shell commands, HTTP requests, file I/O
- **SQLite sessions** — Conversation history stored in `~/.paw/engine.db`
- **Engine settings UI** — Provider selection, API key, model, base URL config

### Bugs Fixed (Phase 1)
1. **Gemini schema error** — `additionalProperties` not supported by Google; added `sanitize_schema()` recursive stripper
2. **Events silently dropped** — Engine sessions used `paw-{uuid}` which hit the `paw-*` background session filter in main.ts; changed to `eng-{uuid}`

---

## File Map

### Rust Backend (`src-tauri/src/engine/`)

| File | LOC | Purpose |
|------|-----|---------|
| `mod.rs` | 10 | Module declarations |
| `types.rs` | ~420 | All data structures: Message, Role, ToolCall, ToolDefinition, EngineEvent, Session, StoredMessage, ChatRequest/Response, EngineConfig, ProviderConfig, ProviderKind, StreamChunk, **TokenUsage**, **ChatAttachment** |
| `providers.rs` | ~780 | AI provider HTTP clients with SSE streaming. `OpenAiProvider`, `AnthropicProvider`, `GoogleProvider`. `AnyProvider` enum. **Exponential backoff retry** (3 retries on 429/500/502/503/529). **Token usage parsing** per provider. **Attachment content block formatting** (OpenAI) |
| `agent_loop.rs` | ~250 | Core agentic loop: call model → accumulate chunks → if tool calls → **HIL approval wait** → execute → loop back. **Token usage accumulation** across chunks. Emits `engine-event` Tauri events |
| `tool_executor.rs` | ~190 | Tool execution: `exec` (sh -c), `fetch` (reqwest), `read_file`, `write_file`. Output truncation (50KB exec, 50KB fetch, 100KB read) |
| `sessions.rs` | ~285 | SQLite session/message storage via rusqlite. DB at `~/.paw/engine.db` with WAL mode. Tables: sessions, messages, engine_config |
| `commands.rs` | ~380 | 11 Tauri `#[tauri::command]` handlers + `EngineState` struct with **`PendingApprovals`** map. **`engine_approve_tool`** command. **Attachment parsing** in chat_send. Smart provider resolution by model prefix |

### TypeScript Frontend (`src/`)

| File | LOC | Purpose |
|------|-----|---------|
| `engine.ts` | ~200 | `PawEngineClient` class — Tauri `invoke()` wrappers for all 11 commands. **`approveTool()`** method. Event listener system. Exported singleton `pawEngine` |
| `engine-bridge.ts` | ~175 | Translates engine events → gateway-style agent events. `isEngineMode()`, `startEngineBridge()`, `onEngineAgent()`, `engineChatSend()`. **`onEngineToolApproval()`** / **`resolveEngineToolApproval()`** for HIL. **Attachment passthrough**. **Usage data forwarding**. Filters intermediate Complete events |
| `views/settings-engine.ts` | ~124 | Engine settings UI: mode toggle, provider kind, API key, model, base URL, save button |

### Modified Files

| File | Changes |
|------|---------|
| `src-tauri/Cargo.toml` | Added: reqwest 0.12 (json+stream+rustls-tls), tokio 1 (full), tokio-stream 0.1, futures 0.3, uuid 1 (v4), rusqlite 0.31 (bundled) |
| `src-tauri/src/lib.rs` | Added `pub mod engine;`, `EngineState` init with `PendingApprovals`, `.manage(engine_state)`, 11 engine commands in `invoke_handler` |
| `src/main.ts` | Engine-bridge imports, dual-mode `connectGateway()`, `handleAgentEvent()`, dual-mode `sendMessage()`. **Phase 2:** dual-mode `loadSessions()`, `loadChatHistory()`, session rename/delete. **HIL approval handler** (~150 lines) via `onEngineToolApproval()` with full security pipeline. Attachment type handling |
| `index.html` | Runtime Mode section in Settings → General with engine config panel |

---

## Architecture Details

### Event Flow (Engine Mode)
```
User types message
  → main.ts sendMessage()
    → engineChatSend() [engine-bridge.ts]
      → pawEngine.chatSend() [engine.ts]
        → invoke('engine_chat_send') [Tauri IPC]
          → Rust engine_chat_send [commands.rs]
            → spawns async agent_loop::run_agent_turn
              → provider.chat_stream() [SSE to AI API]
              → emits engine-event (Delta/ToolRequest/ToolResult/Complete/Error)
              → on ToolRequest: PAUSES on oneshot channel
            → stores messages in SQLite
  
  engine-event Tauri events
    → PawEngineClient listener [engine.ts]
      → wildcard dispatch
        → translateEngineEvent() [engine-bridge.ts]
          → handleAgentEvent() [main.ts] — chat UI updates
        → onEngineToolApproval handler [main.ts] — for tool_request events
          → security classification [security.ts]
          → approval modal / auto-approve/deny
          → resolveEngineToolApproval() → pawEngine.approveTool()
            → invoke('engine_approve_tool') → oneshot channel resolves
              → agent_loop resumes execution
```

### Session ID Conventions
- `eng-{uuid}` — Engine chat sessions (MUST NOT start with `paw-`)
- `paw-research-*` — Research module background sessions (routed separately)
- `paw-build-*` — Build module sessions (routed separately)
- `paw-*` — Other background sessions (filtered/dropped in main chat handler)

### Provider Resolution (commands.rs)
When no `provider_id` is specified, the engine resolves by model name prefix:
- `claude*` / `anthropic*` → Anthropic provider
- `gemini*` / `google*` → Google provider
- `gpt*` / `o1*` / `o3*` → OpenAI provider
- Fallback → default provider → first configured provider

### EngineConfig (persisted in SQLite)
```json
{
  "providers": [{ "id": "google", "kind": "google", "api_key": "...", "base_url": null, "default_model": "gemini-2.0-flash" }],
  "default_provider": "google",
  "default_model": "gemini-2.0-flash",
  "default_system_prompt": "You are a helpful AI assistant...",
  "max_tool_rounds": 20,
  "tool_timeout_secs": 120
}
```

---

## Phase 3 — What's Needed Next

### Priority 1: Multi-Provider Testing
- Test with Anthropic API key (Claude models)
- Test with OpenAI API key (GPT-4o, etc.)
- Test with OpenRouter
- Test with local Ollama
- Verify HIL approval flow works for each provider
- Verify token metering reports correct values per provider

### Priority 2: Attachment Support — Additional Providers
- Anthropic: base64 image content blocks (`type: "image"`)
- Google: inline data parts (`inlineData: { mimeType, data }`)
- Currently only OpenAI format is implemented

### Priority 3: API Key Validation
- Test API key on save in engine settings (lightweight test call)
- Show success/error feedback in settings UI

### Priority 4: Extended Error Surfacing
- Ensure all error states (auth, rate limit, model not found) display clearly in chat UI
- Consider a dedicated error toast for non-recoverable errors

---

## Build & Test Commands

```bash
# On Mac (development)
cd ~/Desktop/paw
git pull origin main
npm run tauri dev          # First build takes 5-10 min (Rust compilation)

# On Codespaces (code changes)
cd /workspaces/paw
cargo check                # Verify Rust compiles (in src-tauri/)
npx tsc --noEmit           # Verify TypeScript compiles
git add -A && git commit -m "..." && git push

# Logs
# Rust logs appear in the terminal running `npm run tauri dev`
# Frontend logs: Cmd+Option+I → Console tab in the Tauri webview
```

## Key Technical Notes

- **Tauri lib name:** `paw_temp_lib` (set in Cargo.toml)
- **Engine DB path:** `~/.paw/engine.db` (SQLite with WAL mode)
- **Tauri event name:** `engine-event` (all engine events flow through this single channel)
- **Runtime mode storage:** `localStorage.getItem('paw-runtime-mode')` — `'engine'` or `'gateway'`
- **Google Gemini quirk:** Rejects `additionalProperties`, `$schema`, `$ref` in tool schemas — `sanitize_schema()` strips these
- **Session ID prefix:** `eng-` not `paw-` (paw-* gets filtered by background session handler)
- **Existing lib.rs is ~2,660 lines** — all the OpenClaw gateway commands (check_node, install_openclaw, start_gateway, etc.) remain intact for gateway mode
