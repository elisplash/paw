---
sidebar_position: 1
title: Architecture
---

# Architecture

Pawz is a Tauri v2 desktop application with a Rust backend and TypeScript frontend.

## System overview

```
┌─────────────────────────────────────────────────┐
│                  Tauri Shell                     │
│  ┌───────────────────────────────────────────┐  │
│  │            TypeScript Frontend             │  │
│  │  Views · Feature Modules · IPC Bridge      │  │
│  └──────────────────┬────────────────────────┘  │
│                     │ IPC (invoke/events)        │
│  ┌──────────────────┴────────────────────────┐  │
│  │              Rust Engine                   │  │
│  │  Agent Loop · Providers · Channels         │  │
│  │  Memory · Skills · Orchestrator · Sandbox  │  │
│  └──────────────────┬────────────────────────┘  │
│                     │                            │
│  ┌──────────────────┴────────────────────────┐  │
│  │           SQLite Database                  │  │
│  │  sessions · messages · memories · tasks    │  │
│  │  skills · agents · projects · trades       │  │
│  └───────────────────────────────────────────┘  │
└─────────────────────────────────────────────────┘
```

## Frontend (`src/`)

Vanilla TypeScript with DOM manipulation — no framework. Each view is a module that renders into the main content area.

### Views

| View | File | Purpose |
|------|------|---------|
| Today | `views/today.ts` | Dashboard with quick actions |
| Agents | `views/agents.ts` | Create, edit, manage agents |
| Tasks | `views/tasks.ts` | Kanban task board |
| Skills | `views/skills.ts` | Skill vault management |
| Research | `views/research.ts` | Research workflow |
| Orchestrator | `views/orchestrator.ts` | Multi-agent projects |
| Memory Palace | `views/memory-palace.ts` | Memory browser/search |
| Mail | `views/mail.ts` | Email integration |
| Trading | `views/trading.ts` | Trading dashboard |
| Automations | `views/automations.ts` | Cron task management |
| Settings | `views/settings*.ts` | 12 settings tabs |

### Feature modules (`src/features/`)

Atomic design pattern — each feature has `atoms.ts` (data/state), `molecules.ts` (UI components), and `index.ts` (exports).

| Module | Purpose |
|--------|---------|
| `agent-policies` | Tool allowlist/denylist per agent |
| `browser-sandbox` | Browser config and network policy |
| `channel-routing` | Message routing rules |
| `container-sandbox` | Docker sandbox config |
| `memory-intelligence` | Memory search and config |
| `prompt-injection` | Injection detection patterns |
| `session-compaction` | Session summarization |
| `slash-commands` | Chat slash command definitions |

### IPC bridge (`src/engine-bridge.ts`)

All communication with the Rust backend goes through Tauri's `invoke()` and event system. The bridge provides typed wrappers for every engine command.

## Backend (`src-tauri/src/engine/`)

Rust async engine built on Tokio.

### Core modules

| Module | Purpose |
|--------|---------|
| `agent_loop.rs` | Main agent conversation loop |
| `providers.rs` | AI provider factory and routing |
| `channels.rs` | Channel bridge message handler |
| `memory.rs` | Hybrid search (BM25 + vector + temporal) |
| `skills.rs` | Skill catalog and credential vault |
| `orchestrator.rs` | Multi-agent project execution |
| `sandbox.rs` | Docker container execution |
| `tool_executor.rs` | Tool dispatch and execution |
| `sessions.rs` | Session and message persistence |
| `compaction.rs` | Session compaction/summarization |
| `routing.rs` | Channel-to-agent routing |
| `injection.rs` | Prompt injection detection |

### Channel bridges

Each channel has its own bridge module:

`telegram.rs` · `discord.rs` · `slack.rs` · `matrix.rs` · `irc.rs` · `mattermost.rs` · `nextcloud.rs` · `nostr.rs` · `twitch.rs` · `webchat.rs` · `web.rs`

### Provider implementations

| Implementation | Providers |
|---------------|-----------|
| `OpenAiProvider` | OpenAI, Ollama, OpenRouter, DeepSeek, Grok, Mistral, Moonshot, Custom |
| `AnthropicProvider` | Anthropic |
| `GoogleProvider` | Google Gemini |

## Database

SQLite with these key tables:

| Table | Contents |
|-------|----------|
| `sessions` | Chat sessions per agent |
| `messages` | All messages with role and tool calls |
| `memories` | Stored facts with embeddings |
| `tasks` | Kanban tasks with cron schedules |
| `skill_credentials` | Encrypted API keys |
| `skill_state` | Enabled/disabled per skill |
| `projects` | Orchestrator projects |
| `positions` | Trading positions |

## Events

The engine communicates with the frontend via Tauri events:

| Event | Purpose |
|-------|---------|
| `engine-event` | Streaming deltas, tool requests, completions, errors |
| `project-event` | Project/agent finished notifications |
| `agent-profile-updated` | Agent profile changes |

## Technology stack

| Layer | Technology |
|-------|-----------|
| Shell | Tauri v2 |
| Frontend | TypeScript, vanilla DOM |
| Backend | Rust, Tokio async |
| Database | SQLite (rusqlite) |
| HTTP | reqwest |
| Docker | bollard |
| Icons | Material Symbols Outlined |
| Fonts | Inter, JetBrains Mono |
