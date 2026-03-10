# Pawz CODE Architecture

## Product definition

Pawz CODE is a standalone, always-on coding system built as a side project separate from OpenPawz.

It is not a mode inside OpenPawz and must not depend on the OpenPawz runtime to function. It exists to do one job extremely well:

- autonomous coding
- codebase understanding
- low-token operation
- persistent coding memory
- strong VS Code-native workflow

The primary user experience is inside VS Code chat. The desktop app is intentionally small and exists mainly to keep the system running, configured, and observable.

---

## Core principles

1. **Separate product boundary**
   - Pawz CODE does not modify or depend on the OpenPawz app runtime.
   - OpenPawz remains untouched except as a source of reusable backend ideas and components.

2. **Always-on daemon**
   - The core runtime is a persistent local service.
   - It should continue running even if the desktop window is closed.
   - It should be able to start on login and recover from failure.

3. **VS Code first**
   - VS Code is the main cockpit.
   - The extension is the primary operator interface.
   - Chat, diffs, context gathering, and coding workflows should be optimized for VS Code.

4. **Token efficiency is a first-class feature**
   - Context compression, protocol summarization, memory recall, and workspace reduction are mandatory.
   - The system should prefer structured compressed context over raw file dumps whenever possible.

5. **Own identity and persistence**
   - Pawz CODE has its own memory, engram, protocols, logs, and configuration.
   - It must not share these directly with OpenPawz.

6. **Small, powerful UI**
   - The desktop app is a compact controller, not a second full chat surface.
   - Its job is power, status, settings, logs, and maintenance.

---

## Product shape

Pawz CODE is made of three major layers:

### 1. Always-on backend daemon
Location:
- `pawz-code/server/`

Responsibilities:
- agent loop
- tool execution
- model/provider routing
- memory and engram retrieval
- token reduction pipeline
- coding protocols
- webhook/API server
- session persistence
- reliability and recovery

This is the real brain of the product.

### 2. VS Code extension
Location:
- `pawz-code/vscode-extension/`

Responsibilities:
- register chat participant
- collect workspace context
- stream model/tool events into VS Code chat
- show diffs
- open files and navigate results
- reconnect to daemon when needed
- surface health and connection state

This is the main operator experience.

### 3. Small Tauri control app
Location:
- `pawz-code/app/`

Responsibilities:
- start/stop service
- configure providers and models
- toggle startup-at-login
- show daemon health
- show logs
- inspect memory/engram status
- manage protocols and settings
- act as a tiny control panel only

This is the power switch and control surface.

---

## Guiding statement

**VS Code is the cockpit. The daemon is the brain. The Tauri app is the power switch.**

---

## Target directory layout

```text
pawz-code/
  ARCHITECTURE.md
  README.md
  docs/
    protocols.md
    runtime.md
    migration-plan.md

  server/
    Cargo.toml
    src/
      main.rs
      config.rs
      state.rs
      provider.rs
      agent.rs
      memory.rs
      tools.rs
      types.rs
      protocols/
      reduction/
      reliability/

  vscode-extension/
    package.json
    tsconfig.json
    src/
      extension.ts
      pawz-client.ts
      tool-renderer.ts
      connection-state.ts
      workspace-context.ts

  app/
    src-tauri/
    src/
    package.json

  shared/
    schemas/
    examples/
```

---

## Persistent storage model

All long-lived Pawz CODE state should live under:

- `~/.pawz-code/`

Suggested layout:

```text
~/.pawz-code/
  config.toml
  memory.db
  logs/
  sessions/
  engram/
  protocols/
  cache/
  models.json
  health.json
```

### Storage responsibilities

- `config.toml`
  - bind address
  - port
  - auth token
  - provider settings
  - model routing settings
  - startup options
  - workspace policy defaults

- `memory.db`
  - long-term coding memory
  - repo memories
  - task summaries
  - operator preferences

- `engram/`
  - codebase summaries
  - architecture snapshots
  - compressed structural maps
  - protocol-ready retrieval artifacts

- `sessions/`
  - resumable run state
  - rolling summaries
  - crash recovery data

- `logs/`
  - daemon logs
  - tool execution logs
  - error traces

---

## Always-on runtime requirements

Pawz CODE must be reliable enough to function as a persistent coding companion.

### Required behaviors

- start automatically with user login
- run in background continuously
- survive Tauri window closure
- recover from transient failure
- expose health endpoint
- maintain persistent memory across restarts
- reconnect cleanly from VS Code
- avoid losing long task state when a stream breaks

### Reliability features

- local watchdog / restart policy
- connection heartbeat
- request timeout handling
- retry logic for model calls where safe
- session journaling
- streaming completion/error guarantees
- run cancellation support
- graceful shutdown hooks

### Service model

The daemon should be treated as a local coding service, not just a CLI that happens to expose HTTP.

---

## VS Code-first design requirements

Pawz CODE should be fully tuned for VS Code use.

### Required extension capabilities

- chat participant as main entry point
- active file awareness
- selection awareness
- workspace root awareness
- diff preview flow
- file open/navigation support
- connection status visibility
- retry/reconnect behavior
- low-friction coding loop

### Desired future capabilities

- diagnostics integration
- test/lint result rendering
- patch review panels
- symbol-aware navigation
- task/runs panel
- memory peek inside VS Code

### UX rule

The extension should feel like a coding-native agent, not a generic chatbot client.

---

## Token reduction architecture

Token efficiency is a defining feature, not an optimization pass.

### Goal

Preserve the OpenPawz strength in aggressive token savings while tuning it specifically for coding workflows.

### Required pipeline

Every model call should flow through a reduction pipeline:

1. request classification
2. workspace relevance filtering
3. memory recall filtering
4. protocol selection
5. structural summarization
6. rolling task summary merge
7. compressed prompt assembly

### Candidate subsystems

- `reduction/workspace_map.rs`
- `reduction/file_summary.rs`
- `reduction/protocol_summary.rs`
- `reduction/task_rollup.rs`
- `reduction/memory_selection.rs`

### Design rules

- never send full workspace context by default
- prefer repo maps over raw file dumps
- prefer summaries over repeated history
- recall only relevant memory
- route cheap subproblems to cheaper models first
- keep a rolling task state so large coding tasks remain stable over time

---

## Memory and Engram model

Pawz CODE needs persistent knowledge designed specifically for coding.

### Memory categories

1. **Operator memory**
   - preferences
   - coding style
   - safety preferences
   - workflows

2. **Repository memory**
   - conventions
   - architecture facts
   - common commands
   - known pitfalls

3. **Task memory**
   - in-progress summaries
   - long-running goals
   - recent decisions

4. **Protocol memory**
   - preferred execution patterns
   - successful repair loops
   - validation habits

### Engram purpose

Engram should hold compressed higher-order understanding of a codebase:
- architecture map
- module relationships
- key entrypoints
- high-value patterns
- recurring implementation idioms
- stable summaries useful across sessions

### Design rule

Memory stores facts. Engram stores compressed understanding.

---

## Model/provider architecture

Pawz CODE must support direct APIs, custom backends, and cheap task routing.

### Minimum provider support

- Anthropic
- OpenAI-compatible APIs
- local providers such as Ollama
- custom webhook/API bridges
- future internal model routers

### Model roles

Instead of one active model, the system should support role-based routing:

- `fast_model`
- `cheap_model`
- `planner_model`
- `coder_model`
- `review_model`
- `long_context_model`

### Routing rule

Use the cheapest capable model for each subtask.

Examples:
- file classification → cheap model
- prompt compression → cheap or local model
- patch generation → coder model
- final review → review model
- large architectural reasoning → long-context model

---

## Protocol architecture

Protocols should be first-class configurable modules rather than hidden prompt fragments.

### Core protocol packs

- coding protocol
- edit protocol
- repo safety protocol
- token reduction protocol
- verification protocol
- long-task protocol
- memory write protocol
- diff review protocol

### Protocol responsibilities

- shape the reasoning loop
- keep tool use disciplined
- preserve low-token operation
- enforce verification before claiming success
- improve consistency across runs

### Implementation direction

- versioned protocol files
- composable protocol stack by task type
- configurable from Tauri control app
- visible in logs and run metadata

---

## Tooling architecture

The current standalone tool set is a strong base, but Pawz CODE needs coding-specific expansion.

### Existing good baseline

- `exec`
- `read_file`
- `write_file`
- `list_directory`
- `grep`
- `fetch`
- `remember`
- `recall`

### Required next tools

- `apply_patch`
- `git_status`
- `git_diff`
- `run_tests`
- `lint_check`
- `workspace_map`
- `file_summary`
- `search_symbols`
- `read_diagnostics`

### Tooling rule

Prefer precise code-editing and code-inspection tools over raw shell usage when possible. Shell remains available, but specialized tools are cheaper and safer.

---

## Safety model

Pawz CODE is powerful, so its safety model should be explicit.

### Baseline policies

- workspace-scoped by default
- sensitive path blocking
- path traversal blocking
- command pattern blocking
- configurable write permissions
- configurable dangerous mode

### Recommended operating modes

- `observe`
- `suggest`
- `edit`
- `autonomous`

### Intent

The product should be able to run powerfully for trusted local use while still preserving strong safeguards around secrets and destructive actions.

---

## Tauri app design

The Tauri UI should stay intentionally small.

### Required panels

- service status
- start/stop toggle
- startup-at-login toggle
- provider/model settings
- memory status
- engram status
- protocol settings
- logs
- health checks

### Explicit non-goal

Do not build a second full chat-first desktop app. VS Code is the main operator interface.

---

## API and webhook surface

Pawz CODE should expose stable endpoints for local and future external integrations.

### Minimum endpoints

- `GET /health`
- `POST /chat/stream`
- `GET /status`
- `POST /runs/cancel`
- `GET /memory/search`
- `POST /webhook/...` for external triggers

### Goals

- local extension integration
- future automation hooks
- model/webhook bridging
- external coding workflows if needed later

---

## Migration plan

### Phase 1 — establish side-project boundary

- keep OpenPawz untouched
- treat `pawz-code/` as the real product root for this effort
- move the root `vscode-extension/` implementation into `pawz-code/vscode-extension/`
- stop shipping from the root extension path

### Phase 2 — lock architecture and contracts

- define shared event schema
- define request/response schema
- define tool result schema
- define diff schema
- define connection/reliability behavior

### Phase 3 — strengthen the daemon

- add protocol loader
- add token reduction pipeline
- add reliability features
- add richer coding tools
- add model role routing
- add memory/engram separation

### Phase 4 — build the small Tauri control app

- service control
- status UI
- provider setup
- logs
- startup behavior
- protocol and memory inspection

### Phase 5 — port only the right OpenPawz backend powers

**Must port**
- token reduction systems
- useful backend tools
- protocol patterns
- secure execution patterns
- memory practices

**Adapt**
- agent loop behavior
- event streaming
- state persistence
- provider abstraction

**Leave behind**
- channels
- unrelated workflows
- main app runtime coupling
- broad platform features not related to coding

### Phase 6 — reliability and VS Code polish

- reconnect logic
- status indicators
- diagnostics integration
- robust diff flow
- stable long-running sessions

---

## Non-negotiable product outcomes

Pawz CODE must:

- stay separate from OpenPawz runtime
- remain always on
- be optimized for VS Code use
- preserve token-saving advantages
- preserve persistent memory and engram behavior
- remain small in UI but powerful in backend behavior
- be cheap enough to run heavily for coding work

---

## Final summary

Pawz CODE is a standalone Tauri-powered coding agent platform with an always-on daemon, a VS Code-first operator experience, its own memory and engram systems, configurable protocols, low-token context reduction, and a compact desktop control shell.

It should inherit the best backend powers from OpenPawz without inheriting OpenPawz itself.
