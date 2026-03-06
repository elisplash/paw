# Engine Module Guide

This guide is for contributors who want a fast mental map of `src-tauri/src/engine/`.

Start with [`mod.rs`](../src-tauri/src/engine/mod.rs): it is the table of contents for the Rust backend. Most contribution work falls into one of four paths:

1. IPC command enters through `src-tauri/src/lib.rs` and `src-tauri/src/commands/`
2. The request is routed into one or more `engine/` modules
3. State is read from or written to `sessions/`, `skills/`, or other storage helpers
4. Results are streamed back to the frontend through Tauri events or command responses

## How To Read The Engine

- Read [`ARCHITECTURE.md`](../ARCHITECTURE.md) first for the full-system view.
- Read [`src-tauri/src/engine/mod.rs`](../src-tauri/src/engine/mod.rs) next to see the top-level modules.
- When debugging a frontend action, trace from the Tauri command in `src-tauri/src/lib.rs` or `src-tauri/src/commands/` into the engine module that owns the behavior.
- When adding behavior, prefer extending an existing module before creating a new top-level module.

## Top-Level Module Map

### Core runtime

- `agent_loop`: Runs the main agent turn loop, including model streaming and tool execution.
- `audit`: Records audit and compliance-style events for actions that should be inspectable later.
- `chat`: Builds chat requests, assembles prompts and tools, and applies loop-safety checks around conversations.
- `events`: Emits runtime events that the frontend can subscribe to.
- `state`: Holds shared engine state used across commands and background tasks.
- `types`: Shared backend types that multiple modules depend on.
- `util`: Small shared helpers that do not justify their own domain module.

### Network and provider integration

- `http`: Centralized HTTP client helpers and request glue used by engine features.
- `oauth`: OAuth helpers and flows for providers or integrations that need token exchange.
- `providers`: Provider abstraction for model vendors such as OpenAI, Anthropic, and Google.
- `pricing`: Token and pricing helpers used to estimate or record model costs.
- `web`: Browser-oriented backend helpers and automation support.

### Persistence and local data

- `paths`: Canonical path helpers so the engine stores data in predictable places.
- `sessions`: Persistent storage for sessions, messages, tasks, dashboards, files, embeddings, and related app data.
- `compaction`: Summarizes long sessions so context stays usable and token use remains bounded.
- `memory`: Semantic memory storage and embedding-side logic.
- `engram`: Higher-level memory intelligence, retrieval, classification, and cognitive-state style features.
- `key_vault`: Secure storage helpers for secrets and credentials.

### Tools, skills, and orchestration

- `tools`: Built-in tool implementations and routing for actions the agent can execute.
- `tool_index`: Semantic lookup index for discovering the right tool at runtime.
- `skills`: Skill loading, validation, prompting, vault integration, and community skill discovery.
- `mcp`: Model Context Protocol support for external tool and skill ecosystems.
- `orchestrator`: Multi-agent coordination for boss/worker style task decomposition.
- `swarm`: Higher-level cooperative agent behaviors beyond a single conversation loop.
- `tasks`: Engine support for task management and task-related operations.

### Channels and bridges

- `channels`: Shared bridge infrastructure such as access control, common config loading, and routed agent execution.
- `telegram`: Telegram-specific bridge implementation with its own numeric user model and Bot API flow.
- `discord`: Discord bridge logic.
- `slack`: Slack bridge logic.
- `matrix`: Matrix bridge logic.
- `irc`: IRC bridge logic.
- `mattermost`: Mattermost bridge logic.
- `nextcloud`: Nextcloud Talk bridge logic.
- `nostr`: Nostr bridge logic, including relay and key handling.
- `twitch`: Twitch bridge logic.
- `webchat`: Embedded web chat bridge and server-side session handling.
- `webhook`: Webhook-triggered integration entry points.

### Automation and integration subsystems

- `n8n_engine`: n8n-specific execution and integration support.
- `routing`: Channel and message routing rules used to decide which agent should answer.
- `injection`: Prompt-injection detection and defensive filtering on the Rust side.
- `sandbox`: Container sandbox support for risky or isolated execution paths.
- `telemetry`: Usage metrics and telemetry collection/export hooks.

### Trading and market features

- `dex`: Ethereum-side DEX trading, portfolio, token, and transaction logic.
- `sol_dex`: Solana-side trading and wallet support.

## Common Contributor Tasks

### Add a new Tauri command

1. Add or extend a function in the owning engine module.
2. Expose the command in `src-tauri/src/commands/` or directly in `src-tauri/src/lib.rs`, depending on the existing pattern.
3. Register the command handler in [`src-tauri/src/lib.rs`](../src-tauri/src/lib.rs).
4. Add a matching frontend `invoke()` wrapper in [`src/engine/molecules/ipc_client.ts`](../src/engine/molecules/ipc_client.ts).

### Debug a frontend action

1. Find the frontend caller in `src/views/`, `src/components/`, or `src/features/`.
2. Follow the `pawEngine.*` method into [`src/engine/molecules/ipc_client.ts`](../src/engine/molecules/ipc_client.ts).
3. Match that `invoke()` name to a Tauri command in the Rust backend.
4. Trace from the command into the owning engine module.

### Add a backend feature safely

- Prefer reusing `sessions/` for persisted data instead of creating ad hoc files.
- Prefer reusing `channels/` helpers if the feature is channel-like.
- Prefer reusing `providers/` or `tools/` abstractions if the feature touches model IO or tool execution.
- Add tests in the same module or nearby integration tests when behavior is non-trivial.

## Where To Go Next

- For frontend structure, read [`docs/frontend-patterns.md`](./frontend-patterns.md).
- For channel-specific contribution work, read [`docs/channel-bridge-guide.md`](./channel-bridge-guide.md).
