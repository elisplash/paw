# Pawz CODE Fix Plan

Generated from the full codebase audit. Issues are ordered by impact on daily use.
Each phase should be completed before moving to the next.

---

## Phase 1 — Daily Use Blockers

Issues that affect every coding session right now.

---

### 1.1 Extension icon is missing

**Problem:**
`extension.ts` sets `participant.iconPath = vscode.Uri.joinPath(context.extensionUri, 'images', 'icon.png')` but there is no `images/` folder in `pawz-code/vscode-extension/`. The participant loads without an icon. This is the first thing a user sees.

**Fix:**
Add a `pawz-code/vscode-extension/images/icon.png` (128×128 or 256×256). Can be a copy of the existing `images/pawz-favicon.png` from the repo root. Also add the `images` folder to the VSIX via `package.json` `files` field.

**Files affected:**
- `pawz-code/vscode-extension/package.json` — add `"files": ["out/**", "images/**"]`
- `pawz-code/vscode-extension/images/icon.png` — create

**Dependencies:** None.

---

### 1.2 Cancellation does not stop the server-side agent loop

**Problem:**
`extension.ts` wires `token.onCancellationRequested(() => abortController.abort())` which cuts the SSE connection client-side. But it never calls `POST /runs/cancel`. The daemon keeps running the agent loop — executing tools, calling the model, burning tokens — until it either finishes or hits `max_rounds`. For a 20-round agent loop this can waste significant time and money after the user presses Stop.

The `run_id` is available in the first `tool_request` or `complete` event that arrives over the SSE stream, but `pawz-client.ts` currently only calls `onEvent` and does not expose the `run_id` to the caller.

**Fix:**
1. In `pawz-client.ts`: track the `run_id` from the first event that carries it and expose it (either via a callback parameter change or by resolving a returned `runId` promise).
2. In `extension.ts`: when `token.onCancellationRequested` fires, POST `{ run_id }` to `/runs/cancel` before (or alongside) `abortController.abort()`.

**Files affected:**
- `pawz-code/vscode-extension/src/pawz-client.ts`
- `pawz-code/vscode-extension/src/extension.ts`

**Dependencies:** None. The `/runs/cancel` endpoint already exists on the daemon.

---

### 1.3 Mid-stream connection drop is silent — user sees truncated response with no error

**Problem:**
If the daemon crashes or restarts while the SSE stream is in progress, `reader.read()` in `pawz-client.ts` returns `{ done: true }` immediately. `streamChat()` resolves successfully. The user sees whatever text arrived before the drop with no indication anything went wrong — no error message, no retry prompt.

**Fix:**
In `pawz-client.ts`, track whether a `complete` or `error` event was received. After `done: true`, if neither event arrived, throw an error (e.g. `"Connection to pawz-code dropped before run completed"`) so `extension.ts` catches it and surfaces a warning message with a retry button.

**Files affected:**
- `pawz-code/vscode-extension/src/pawz-client.ts`
- `pawz-code/vscode-extension/src/extension.ts` (already has error-handling path that shows retry button)

**Dependencies:** None.

---

### 1.4 README does not document the `claude_code` provider

**Problem:**
`README.md` documents `anthropic`, `openai`, and Ollama providers but has no mention of `provider = "claude_code"`. Users who want to use their Claude Max subscription without a separate API key will not know the option exists.

**Fix:**
Add a row to the provider table and a short explanation: no `api_key` needed, requires `claude login` first, optionally set `claude_binary_path`.

**Files affected:**
- `pawz-code/README.md`

**Dependencies:** None.

---

## Phase 2 — Reliability Hardening

Issues that cause silent failures or data loss under real use.

---

### 2.1 Poisoned mutex crashes the daemon permanently

**Problem:**
Every DB access in `memory.rs`, `engram.rs`, `protocols.rs`, and `state.rs` uses `.lock().unwrap()`. If any async task panics while holding the lock, the mutex becomes poisoned. Every subsequent `.unwrap()` on that mutex will also panic — causing the daemon to crash on the next request and every request after, until manually restarted.

**Fix:**
Replace `.lock().unwrap()` with `.lock().unwrap_or_else(|e| e.into_inner())` (poison recovery) across all mutex accesses in the daemon. This recovers the inner value even from a poisoned mutex, preventing a cascade crash.

**Files affected:**
- `pawz-code/server/src/state.rs`
- `pawz-code/server/src/memory.rs`
- `pawz-code/server/src/engram.rs`
- `pawz-code/server/src/protocols.rs`

**Dependencies:** None.

---

### 2.2 SSE broadcast channel silently drops events under load

**Problem:**
`broadcast::channel::<String>(1024)` in `state.rs` holds a maximum of 1024 unread messages. Under concurrent load (multiple active runs, slow SSE clients), the sender will start getting `SendError` results. `fire()` discards the error with `let _ = ...`. Events — including `complete` and `error` events — can be silently dropped. The client hangs waiting for a `complete` that never arrives.

**Fix:**
Two complementary changes:
1. Increase the channel capacity to at least 4096 or make it configurable.
2. Log a warning when `fire()` fails to send (receiver lagged or disconnected), so failures are at least visible in logs.

**Files affected:**
- `pawz-code/server/src/state.rs`

**Dependencies:** None.

---

### 2.3 Connection heartbeat has no backoff — hammers a dead daemon

**Problem:**
`ConnectionStateManager` in `connection-state.ts` polls `/status` every 15 seconds regardless of how long the daemon has been unreachable. If the daemon is down for an hour, the extension makes 240 wasted fetch requests with no user-visible change.

**Fix:**
Add exponential backoff in `startHeartbeat()`: after each consecutive failure, double the interval up to a max of ~2 minutes. Reset to 15s on a successful connection. Keep a consecutive-failure counter.

**Files affected:**
- `pawz-code/vscode-extension/src/connection-state.ts`

**Dependencies:** None.

---

### 2.4 `apply_patch` depends on system `patch` binary — absent on stock macOS

**Problem:**
`tool_apply_patch()` in `tools.rs` calls the system `patch` binary. On macOS without Xcode CLI tools, `patch` is not installed. The agent will attempt to use this tool and get a confusing spawn error (`No such file or directory (os error 2)`) that doesn't explain what's missing.

**Fix:**
Before spawning `patch`, check if the binary exists using `which patch` or Rust's `std::process::Command` with a quick `--version` call. If it's absent, return a clear error: `"apply_patch requires the system patch binary. On macOS: xcode-select --install"`. Also consider implementing a pure-Rust line-by-line patch fallback for simple unified diffs, to avoid the binary dependency entirely.

**Files affected:**
- `pawz-code/server/src/tools.rs`

**Dependencies:** None.

---

### 2.5 `claude_code` provider panic path on stdout capture

**Problem:**
In `claude_code.rs`, `child.stdout.take().expect("stdout not captured")` and `child.stderr.take().expect("stderr not captured")` will panic if the OS somehow fails to pipe the streams despite `Stdio::piped()` being set. While rare, `expect()` panics propagate as async task panics that poison the mutex (see 2.1).

**Fix:**
Replace both `expect()` calls with `ok_or_else(|| anyhow::anyhow!(...))` and propagate as `Result` errors using `?`, matching the error-handling pattern used elsewhere in the file.

**Files affected:**
- `pawz-code/server/src/claude_code.rs`

**Dependencies:** Ideally fix 2.1 first, but this is independently safe to fix now.

---

## Phase 3 — Complete the Architecture Promises

Features documented in `ARCHITECTURE.md` that are scaffolded but not wired, or fully absent.

---

### 3.1 Model role routing exists in config but is never called

**Problem:**
`config.rs` has the full `ModelRoles` struct (`fast`, `cheap`, `planner`, `coder`, `review`, `long_context`) and `model_for_role()`. `ARCHITECTURE.md` explicitly calls out role-based routing as a design requirement. However, `agent.rs` always passes `state.config.model` to the provider — `model_for_role()` is never called anywhere.

**Fix:**
In `agent.rs`, use `classify_request()` to pick the appropriate model role, then call `state.config.model_for_role(role)` to resolve the actual model name. Pass the resolved model to `provider::call_streaming()` (which will require `Config` to accept a model override, or pass the model string directly into the provider call). Suggested mapping:
- `Conversational` → `fast`
- `Exploration` → default
- `Edit` → `coder`
- `Execution` → default
- `Architecture` → `long_context`
- `Memory` → `cheap`

**Files affected:**
- `pawz-code/server/src/agent.rs`
- `pawz-code/server/src/provider.rs` (add model override parameter to `call_streaming`)
- `pawz-code/server/src/config.rs` (no changes needed — the infrastructure is already there)

**Dependencies:** None.

---

### 3.2 `run_tests` and `lint_check` tools are missing

**Problem:**
`ARCHITECTURE.md` lists `run_tests` and `lint_check` as required next tools. They are not implemented in `tools.rs`. The agent cannot run the test suite or linter without falling back to raw `exec` calls, which are less structured and don't benefit from tool-renderer progress labels.

**Fix:**
Implement `run_tests` and `lint_check` in `tools.rs`:
- `run_tests`: detect test runner (cargo test, npm test, pytest, go test) based on workspace root contents, run with timeout, return structured output (pass/fail count, failing test names)
- `lint_check`: detect linter (clippy, eslint, tsc, ruff) based on workspace, run, return structured output

Both tools should accept an optional `path` (defaults to `workspace_root`) and `timeout_secs`.

Add corresponding entries to `all_tools()` and `execute()`, and add `stream.progress()` labels in `tool-renderer.ts`.

**Files affected:**
- `pawz-code/server/src/tools.rs`
- `pawz-code/vscode-extension/src/tool-renderer.ts`

**Dependencies:** None. Can be done before or after 3.1.

---

### 3.3 `read_diagnostics` tool is missing

**Problem:**
`ARCHITECTURE.md` lists `read_diagnostics` as a required tool for reading VS Code language server diagnostics (errors, warnings). Without it, the agent cannot see compile errors or type errors unless the user pastes them or the agent runs a build manually.

**Fix:**
This is more complex because VS Code diagnostics are available in the extension context, not the daemon. The fix has two parts:
1. In `extension.ts`, inject active diagnostics into the workspace context string built by `buildWorkspaceContext()` (read from `vscode.languages.getDiagnostics()`)
2. Optionally: add a `read_diagnostics` tool in `tools.rs` that runs the build/type-check command and parses the output, for daemon-side access

The extension-side injection (part 1) is lower effort and higher value.

**Files affected:**
- `pawz-code/vscode-extension/src/extension.ts` (`buildWorkspaceContext()`)

**Dependencies:** None.

---

### 3.4 Status display uses a text document — not a native VS Code pattern

**Problem:**
`pawz-code.showStatus` opens a markdown text document with `vscode.workspace.openTextDocument()` + `vscode.window.showTextDocument()`. This creates a file tab in the editor, which is jarring and pollutes the editor tab bar. It also doesn't auto-close.

**Fix:**
Replace with `vscode.window.showInformationMessage()` for the short connected/disconnected status, or a `QuickPick` panel for the detailed status with model/provider/memory/engram stats. Both are standard VS Code patterns that don't open editor tabs.

**Files affected:**
- `pawz-code/vscode-extension/src/extension.ts`

**Dependencies:** None.

---

## Phase 4 — Polish and Finish

Remaining architecture items that are lower priority but represent meaningful gaps vs. the architecture document.

---

### 4.1 Always-on / startup-at-login not implemented

**Problem:**
`ARCHITECTURE.md` states the daemon must start automatically on login and survive Tauri window closure. Currently the user must manually run `cargo run` or the compiled binary every time they restart their machine. This violates the "always-on" core principle.

**Fix:**
- **macOS:** Generate a launchd plist at `~/Library/LaunchAgents/io.pawz.pawzcode.plist` that points at the compiled binary and sets `RunAtLoad = true`. Can be generated/registered from the Tauri app or as a post-install step.
- **Linux:** Generate a systemd user unit at `~/.config/systemd/user/pawz-code.service`.
- The Tauri app's "Start at Login" toggle should write/remove these files.

**Files affected:**
- `pawz-code/app/src/main.ts` (add start-at-login toggle logic)
- New: `pawz-code/packaging/launchd/io.pawz.pawzcode.plist.template`
- New: `pawz-code/packaging/systemd/pawz-code.service.template`

**Dependencies:** Phase 3 work should be complete first since the daemon binary needs to be stable before registering it as a service.

---

### 4.2 Tauri control app is a placeholder

**Problem:**
`pawz-code/app/` has a working status polling UI in `main.ts` and `index.html`, but:
- `openConfig()` just logs a message instead of opening the config file
- There is no start/stop service control
- There is no memory/engram inspection panel beyond the stats shown
- The `load_config` Tauri command referenced in `main.ts` does not exist in the Tauri Rust backend

**Fix:**
Implement the Tauri backend commands:
- `load_config` — read `~/.pawz-code/config.toml` and return it
- `save_config` — write updated config back
- `open_config_file` — open the config file in the system default editor (`open` on macOS, `xdg-open` on Linux)
- `start_daemon` / `stop_daemon` — spawn/kill the daemon process

**Files affected:**
- `pawz-code/app/src/main.ts`
- `pawz-code/app/index.html`
- New: `pawz-code/app/src-tauri/src/main.rs` (Tauri backend commands)

**Dependencies:** Depends on 4.1 (startup-at-login) for full service control.

---

### 4.3 Session journaling and crash recovery not implemented

**Problem:**
`ARCHITECTURE.md` calls for session crash recovery — if the daemon dies mid-task, the user should be able to resume. The `sessions` table exists in the DB schema but nothing writes to it. There is no session title, summary, or recovery data stored.

**Fix:**
1. At agent loop start, write a session record with `title` = first 80 chars of the user message and `updated_at`.
2. After each agent round, update `sessions.summary` with the rolling task summary from `reduction::rolling_task_summary()`.
3. In `extension.ts`, optionally persist the last `session_id` in VS Code workspace state so it's offered as a resume option on next chat.

**Files affected:**
- `pawz-code/server/src/agent.rs`
- `pawz-code/server/src/memory.rs` (add session write/update functions)

**Dependencies:** None, but low priority relative to Phases 1–3.

---

### 4.4 Operating modes (observe/suggest/edit/autonomous) not implemented

**Problem:**
`ARCHITECTURE.md` defines four operating modes. Currently the daemon always runs in full autonomous mode (all tools pre-approved). There is no way to put it in observe or suggest mode.

**Fix:**
Add `operating_mode` to `Config` (`"observe" | "suggest" | "edit" | "autonomous"`, default `"autonomous"`). In `agent.rs`, gate `ToolAutoApproved` events and actual tool execution on the mode. For `suggest` mode, emit tool calls as `ToolRequest` events and wait for client approval before executing. This requires a new event type and a corresponding approval endpoint.

This is a substantial feature. Leave for last — it changes the fundamental agent loop contract.

**Files affected:**
- `pawz-code/server/src/config.rs`
- `pawz-code/server/src/agent.rs`
- `pawz-code/server/src/types.rs` (new event variants)
- `pawz-code/server/src/main.rs` (new approval endpoint)
- `pawz-code/vscode-extension/src/extension.ts`
- `pawz-code/vscode-extension/src/tool-renderer.ts`
- `pawz-code/schemas/events.json`

**Dependencies:** All other phases should be complete before this — it's a breaking change to the agent loop.

---

### 4.5 Webhook endpoints not implemented

**Problem:**
`ARCHITECTURE.md` calls for `POST /webhook/...` endpoints for external triggers (automation hooks, CI integration, etc.). The daemon currently only accepts chat requests from VS Code.

**Fix:**
Add a generic webhook route in `main.rs` that accepts a message payload and feeds it into the agent loop as if it came from a chat request. Authentication via the same bearer token. This enables CI/CD triggers, n8n automation, and external coding workflows.

**Files affected:**
- `pawz-code/server/src/main.rs`
- `pawz-code/schemas/requests.json` (add webhook request schema)

**Dependencies:** Phases 1–3 should be stable first.

---

## Summary Table

| # | Issue | Phase | Effort | Impact |
|---|-------|-------|--------|--------|
| 1.1 | Missing extension icon | 1 | Low | Every user |
| 1.2 | Cancellation doesn't stop daemon | 1 | Medium | Every Stop press |
| 1.3 | Silent stream truncation on daemon drop | 1 | Low | Any instability |
| 1.4 | README missing claude_code docs | 1 | Low | New users |
| 2.1 | Poisoned mutex crashes daemon | 2 | Low | Any panic |
| 2.2 | SSE broadcast drops events silently | 2 | Low | Concurrent load |
| 2.3 | No heartbeat backoff | 2 | Low | Extended outages |
| 2.4 | apply_patch needs system patch binary | 2 | Medium | macOS users |
| 2.5 | claude_code panic on stdout capture | 2 | Low | Rare OS failures |
| 3.1 | Model role routing never called | 3 | Medium | Token efficiency |
| 3.2 | run_tests / lint_check tools missing | 3 | Medium | Coding workflows |
| 3.3 | read_diagnostics missing | 3 | Low | Coding workflows |
| 3.4 | Status uses text document not native panel | 3 | Low | UX polish |
| 4.1 | No startup-at-login | 4 | High | Always-on promise |
| 4.2 | Tauri app is a placeholder | 4 | High | Power users |
| 4.3 | No session crash recovery | 4 | Medium | Long tasks |
| 4.4 | Operating modes not implemented | 4 | High | Safety model |
| 4.5 | No webhook endpoints | 4 | Medium | Automation |
