# Architecture

> Pawz is a Tauri v2 native desktop app — Rust backend, TypeScript frontend, IPC bridge.  
> ~86k LOC total (39k Rust + 35k TypeScript + 12k CSS) · 602 tests · 3-job CI · 0 clippy warnings

---

## Overview

```
┌─────────────────────────────────────────────────────────────┐
│  Pawz Desktop App                                           │
│                                                             │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  Frontend (TypeScript, vanilla DOM)                   │  │
│  │  • 20+ views (agents, tasks, mail, research, etc.)    │  │
│  │  • 7 feature modules (atomic design pattern)          │  │
│  │  • Material Symbols icon library                      │  │
│  └──────────────────┬────────────────────────────────────┘  │
│                     │ Tauri IPC (158 structured commands)    │
│  ┌──────────────────▼────────────────────────────────────┐  │
│  │  Rust Backend Engine                                  │  │
│  │  • Agent loop with SSE streaming                      │  │
│  │  • Tool executor with human-in-the-loop approval      │  │
│  │  • 11 channel bridges                                 │  │
│  │  • 3 native AI providers (+ 7 via model routing)      │  │
│  │  • SQLite persistence + OS keychain                   │  │
│  │  • Docker container sandbox (bollard crate)           │  │
│  └──────────────────┬────────────────────────────────────┘  │
│                     │                                       │
│                     ▼                                       │
│               Operating System                              │
└─────────────────────────────────────────────────────────────┘
```

No Node.js backend, no gateway process, no open network ports. Every operation flows through Tauri IPC commands between the frontend and the Rust engine.

---

## Directory Structure

```
src/                          # TypeScript frontend
├── main.ts                   # App bootstrap, event listeners, IPC bridge
├── engine.ts                 # Engine bridge (state, config, IPC helpers)
├── engine-bridge.ts          # Tauri event/command wrappers
├── security.ts               # Command risk classifier, injection scanner
├── types.ts                  # Shared TypeScript types
├── styles.css                # All application styles
├── db.ts                     # SQLite helpers (Web SQL via Tauri plugin)
├── workspace.ts              # Workspace management
├── views/                    # UI views (one file per page)
│   ├── agents.ts             # Agent CRUD, avatars, mini-chat, dock
│   ├── mail.ts               # Email client (IMAP/SMTP)
│   ├── projects.ts           # Project workspaces
│   ├── memory-palace.ts      # Memory visualization
│   ├── skills.ts             # Skill vault
│   ├── research.ts           # Research workflow
│   ├── tasks.ts              # Kanban board
│   ├── trading.ts            # Crypto trading dashboard
│   ├── orchestrator.ts       # Multi-agent orchestration
│   ├── settings*.ts          # Settings tabs (10 files)
│   └── ...
├── features/                 # Feature modules (atomic design)
│   ├── slash-commands/       # 20 commands with autocomplete
│   ├── container-sandbox/    # Docker sandbox config
│   ├── prompt-injection/     # Injection detection (30+ patterns)
│   ├── memory-intelligence/  # Smart memory operations
│   ├── agent-policies/       # Per-agent tool policies
│   ├── channel-routing/      # Rule-based channel routing
│   ├── session-compaction/   # AI summarization
│   └── browser-sandbox/      # Browser profiles, screenshots, network policy
├── components/               # Shared UI components
│   ├── helpers.ts            # DOM helpers, escaping, formatting
│   └── toast.ts              # Toast notifications
└── assets/
    ├── avatars/              # 50 Pawz Boi PNGs (96×96)
    └── fonts/                # Material Symbols woff2

src-tauri/                    # Rust backend
├── src/
│   ├── main.rs               # Tauri app entry point
│   ├── lib.rs                # Command registration, plugin setup
│   └── engine/               # Core engine modules
│       ├── mod.rs            # Module exports
│       ├── commands.rs       # 134 Tauri IPC commands
│       ├── tools/            # Tool executor — 22 focused modules
│       │   ├── mod.rs        # Definitions, routing, HIL approval
│       │   ├── agents.rs     # Agent management tools
│       │   ├── agent_comms.rs # Inter-agent messaging tools
│       │   ├── coinbase.rs   # Coinbase trading
│       │   ├── dex.rs        # DEX trading tools
│       │   ├── email.rs      # Email tools
│       │   ├── exec.rs       # Shell execution
│       │   ├── fetch.rs      # HTTP/web fetch
│       │   ├── filesystem.rs # File read/write
│       │   ├── github.rs     # GitHub tools
│       │   ├── integrations.rs # Skill integration tools
│       │   ├── memory.rs     # Memory tools
│       │   ├── skill_output.rs # Skill output/widget tools
│       │   ├── skill_storage.rs # Persistent key-value storage tools
│       │   ├── skills_tools.rs # Community skill tools
│       │   ├── slack.rs      # Slack tools
│       │   ├── solana.rs     # Solana tools
│       │   ├── soul.rs       # Soul/personality tools
│       │   ├── squads.rs     # Agent squad management tools
│       │   ├── request_tools.rs # Tool RAG: semantic tool discovery meta-tool
│       │   ├── tasks.rs      # Task management
│       │   ├── telegram.rs   # Telegram tools
│       │   └── web.rs        # Browser automation tools
│       ├── providers/        # AI provider abstraction
│       │   ├── mod.rs        # Provider routing
│       │   ├── anthropic.rs  # Anthropic Messages API
│       │   ├── google.rs     # Google Gemini API
│       │   └── openai.rs     # OpenAI Chat Completions API
│       ├── sessions/         # Session management — 16 modules
│       │   ├── mod.rs        # Session orchestration
│       │   ├── sessions.rs   # CRUD, listing, compaction triggers
│       │   ├── messages.rs   # Message persistence
│       │   ├── memories.rs   # Memory CRUD
│       │   ├── config.rs     # Agent/engine config
│       │   ├── embedding.rs  # Embedding storage
│       │   ├── schema.rs     # SQLite schema migrations
│       │   ├── tasks.rs      # Task/activity persistence
│       │   ├── projects.rs   # Project management
│       │   ├── positions.rs  # Trading positions
│       │   ├── trades.rs     # Trade history
│       │   ├── agent_files.rs # Per-agent file tracking
│       │   ├── agent_messages.rs # Inter-agent message persistence
│       │   └── squads.rs     # Agent squad persistence
│       ├── skills/           # Skill vault — 40 built-in skills
│       │   ├── mod.rs        # Skill loading, prompt injection
│       │   ├── builtins.rs   # 40 built-in skill definitions
│       │   ├── crypto.rs     # Credential encryption (AES-256-GCM + keychain)
│       │   ├── vault.rs      # Credential storage/retrieval
│       │   ├── prompt.rs     # Prompt construction
│       │   ├── status.rs     # Readiness checks
│       │   ├── types.rs      # Skill types
│       │   └── community/    # Community skills (skills.sh + PawzHub)
│       │       ├── mod.rs, github.rs, parser.rs
│       │       ├── search.rs, store.rs, types.rs
│       │       └── pawzhub.rs    # PawzHub marketplace browser
│       ├── dex/              # Ethereum DEX trading — 15 modules
│       │   ├── mod.rs        # DEX orchestration
│       │   ├── swap.rs       # Uniswap V2/V3 swaps
│       │   ├── wallet.rs     # HD wallet (BIP-39/44)
│       │   ├── rpc.rs        # JSON-RPC client
│       │   ├── tokens.rs     # ERC-20 operations
│       │   ├── transfer.rs   # ETH/token transfers
│       │   ├── tx.rs         # Transaction building/signing
│       │   ├── rlp.rs        # RLP encoding
│       │   ├── abi.rs        # ABI encoding/decoding
│       │   ├── constants.rs  # Chain constants
│       │   ├── primitives.rs # U256, Address types
│       │   ├── discovery.rs  # Token discovery
│       │   ├── monitoring.rs # Position monitoring
│       │   ├── portfolio.rs  # Portfolio tracking
│       │   └── token_analysis.rs # Token analysis
│       ├── sol_dex/          # Solana DEX trading — 11 modules
│       │   ├── mod.rs        # Solana DEX orchestration
│       │   ├── jupiter.rs    # Jupiter aggregator
│       │   ├── pumpportal.rs # Pump.fun integration
│       │   ├── wallet.rs     # Ed25519 wallet
│       │   ├── rpc.rs        # Solana RPC client
│       │   ├── transaction.rs # Transaction building
│       │   ├── transfer.rs   # SOL/token transfers
│       │   ├── price.rs      # Price feeds
│       │   ├── portfolio.rs  # Portfolio tracking
│       │   ├── helpers.rs    # Shared utilities
│       │   └── constants.rs  # Network constants
│       ├── whatsapp/         # WhatsApp bridge — 7 modules
│       │   ├── mod.rs        # Bridge orchestration
│       │   ├── evolution_api.rs # Evolution API client
│       │   ├── webhook.rs    # Webhook server
│       │   ├── messages.rs   # Message handling
│       │   ├── bridge.rs     # Bridge lifecycle
│       │   ├── config.rs     # Configuration
│       │   └── docker.rs     # Docker management
│       ├── memory/           # Semantic memory — 3 modules
│       │   ├── mod.rs        # Store/search/merge/decay/MMR/facts
│       │   ├── ollama.rs     # Ollama readiness, startup, model pull
│       │   └── embedding.rs  # EmbeddingClient (vector operations)
│       ├── tool_index.rs     # Tool RAG: semantic tool discovery index
│       ├── orchestrator/     # Boss/worker multi-agent orchestration — 5 modules
│       │   ├── mod.rs        # AgentRole enum, orchestration entry points
│       │   ├── tools.rs      # Orchestrator tool definitions
│       │   ├── handlers.rs   # Tool call handlers
│       │   ├── agent_loop.rs # Unified boss/worker agent loop
│       │   └── sub_agent.rs  # Sub-agent spawning
│       ├── agent_loop/       # Core agent conversation loop — 2 modules
│       │   ├── mod.rs        # run_agent_turn (streaming + tool routing)
│       │   └── trading.rs    # Trading auto-approve policy checks
│       ├── channels/         # Shared channel bridge logic — 3 modules
│       │   ├── mod.rs        # Types, config helpers, message splitting
│       │   ├── agent.rs      # run_channel_agent, routed agent dispatch
│       │   └── access.rs     # User access control (approve/deny/remove)
│       ├── nostr/            # Nostr bridge — 3 modules
│       │   ├── mod.rs        # Config, state, keychain, bridge API
│       │   ├── crypto.rs     # NIP-04 encrypt/decrypt, event signing
│       │   └── relay.rs      # WebSocket relay loop
│       ├── webchat/          # WebChat bridge — 4 modules
│       │   ├── mod.rs        # Config, state, public API, WebSocket handler
│       │   ├── server.rs     # TLS acceptor, HTTP server, connection handler
│       │   ├── session.rs    # Session management, cookie auth
│       │   └── html.rs       # Inline chat HTML/JS/CSS
│       ├── compaction.rs     # Session compaction (context summarization)
│       ├── sandbox.rs        # Docker container sandboxing
│       ├── routing.rs        # Channel routing rules
│       ├── injection.rs      # Prompt injection detection (Rust side)
│       ├── state.rs          # Engine state management
│       ├── pricing.rs        # Token pricing
│       ├── chat.rs           # Chat utilities
│       ├── types.rs          # Shared Rust types
│       ├── telegram.rs       # Telegram bridge
│       ├── discord.rs        # Discord bridge
│       ├── slack.rs          # Slack bridge
│       ├── matrix.rs         # Matrix bridge
│       ├── irc.rs            # IRC bridge
│       ├── mattermost.rs     # Mattermost bridge
│       ├── nextcloud.rs      # Nextcloud Talk bridge
│       ├── twitch.rs         # Twitch bridge
│       ├── web.rs            # Browser automation (headless Chrome)
│       ├── events.rs         # Event-driven task trigger dispatcher
│       ├── mcp/              # MCP (Model Context Protocol) client — 5 modules
│       │   ├── mod.rs        # MCP session lifecycle
│       │   ├── client.rs     # JSON-RPC transport
│       │   ├── types.rs      # MCP protocol types
│       │   ├── tools.rs      # Tool schema conversion
│       │   └── registry.rs   # Per-agent MCP server registry
│       ├── toml/             # TOML skill manifest loader — 4 modules
│       │   ├── mod.rs        # Public API
│       │   ├── parser.rs     # TOML parsing and validation
│       │   ├── loader.rs     # Filesystem scanning and hot-reload
│       │   └── types.rs      # TOML manifest types
│   ├── commands/             # Split Tauri command files
│   │   ├── mod.rs            # Command module declarations
│   │   ├── task.rs           # Task/cron command handlers
│   │   ├── webhook.rs        # Generic webhook server commands
│   │   ├── mcp.rs            # MCP client commands
│   │   ├── skill_wizard.rs   # Skill creation wizard
│   │   └── skills.rs         # Skill management commands
│   └── atoms/                # Shared types and error handling
│       ├── types.rs          # All shared data types
│       └── error.rs          # Typed EngineError enum
├── Cargo.toml                # Rust dependencies
├── tauri.conf.json           # Tauri config (CSP, bundle, permissions)
└── capabilities/
    └── default.json          # Filesystem scope, shell permissions
```

---

## Rust Backend

### Agent Loop (`agent_loop/`)

The core conversation loop:
1. Receives user message
2. Injects auto-recalled memories into context
3. Sends to configured AI provider via SSE streaming
4. Parses tool calls from response
5. Routes each tool call through the tool executor (with HIL approval)
6. Loops back with tool results until the agent is done

### Tools (`tools/`)

Tool execution is split across 21 focused modules, each owning a single domain. The `mod.rs` barrel file provides:
- `definitions()` — collects all tool schemas from every module
- `execute_tool()` — routes tool calls to the correct module
- Human-in-the-loop (HIL) approval flow via oneshot channels

Tool flow:
1. Classify risk level (critical/high/medium/low/safe)
2. Check allowlist/denylist patterns
3. If approval needed → emit `ToolRequest` event to frontend
4. Frontend shows approval modal → user decides
5. `engine_approve_tool` resolves the pending approval
6. Execute or deny the tool

Tool modules: `agents`, `agent_comms`, `coinbase`, `dex`, `email`, `exec`, `fetch`, `filesystem`, `github`, `integrations`, `memory`, `request_tools`, `skill_output`, `skill_storage`, `skills_tools`, `slack`, `solana`, `soul`, `squads`, `tasks`, `telegram`, `web`.

Tool categories: `exec`, `web_search`, `web_fetch`, `file_read`, `file_write`, `memory`, `agent`, `agent_comms`, `squads`, `trading`.

### AI Providers (`providers/`)

Three native provider implementations with SSE streaming (each in its own module):
- **OpenAI** — Chat completions API, function calling, multimodal
- **Anthropic** — Messages API, tool use, thinking blocks
- **Google Gemini** — GenerateContent API, function declarations, thought handling

Additional providers are handled via model-prefix routing to OpenAI-compatible endpoints (DeepSeek, xAI, Mistral, Moonshot, Azure).

### Channel Bridges

Each of the 11 bridges follows a uniform pattern:
- `start_*` / `stop_*` — spawn/kill the bridge task
- `get_*_config` / `set_*_config` — read/write bridge configuration
- `*_status` — check if bridge is running
- `approve_user` / `deny_user` / `remove_user` — user access control
- Messages received → routed to the configured agent → response sent back

### Memory (`memory/`)

Hybrid retrieval system:
1. **BM25 full-text search** — SQLite FTS5 virtual table
2. **Vector similarity** — Cosine similarity on Ollama-generated embeddings
3. **Weighted merge** — Combine BM25 and vector scores
4. **MMR re-ranking** — Jaccard-based diversity (lambda=0.7)
5. **Temporal decay** — Exponential decay with 30-day half-life

Auto-recall injects relevant memories into agent context. Auto-capture extracts key facts from conversations.

### Tool RAG — Intent-Stated Retrieval (`tool_index.rs`)

Pawz uses **Tool RAG** (Retrieval-Augmented Generation for tools) to solve the "tool bloat" problem. Instead of dumping all 75+ tool definitions into every LLM request (~7,500 tokens), the agent discovers tools on demand via semantic search — like a library patron asking a librarian for the right book.

```
┌─────────────────────────────────────────────────────────────────┐
│  PATRON  (Cloud LLM — Gemini / Claude / GPT)                   │
│                                                                  │
│  System prompt:                                                  │
│  "You have 17 skill domains. To use one, call request_tools."   │
│                                                                  │
│  Always loaded: memory_store, memory_search, soul_read,         │
│    soul_write, soul_list, self_info, read_file, write_file,     │
│    list_directory, request_tools                                 │
│                                                                  │
│  Token cost: ~800 (vs ~7,500 with all tools)                    │
└──────────────────────┬──────────────────────────────────────────┘
                       │  request_tools("send email to john")
                       ▼
┌─────────────────────────────────────────────────────────────────┐
│  LIBRARIAN  (Ollama local — nomic-embed-text, free, ~50ms)      │
│                                                                  │
│  1. Embed the query → 768-dim vector                            │
│  2. Cosine similarity against tool index                         │
│  3. Domain expansion (email_send → also email_read)             │
│  4. Return matching tool schemas                                 │
│                                                                  │
│  Fallbacks: exact name match, domain request                     │
└──────────────────────┬──────────────────────────────────────────┘
                       │  tools injected into next agent round
                       ▼
┌─────────────────────────────────────────────────────────────────┐
│  LIBRARY  (ToolIndex — in-memory, ~230KB)                       │
│                                                                  │
│  75+ tool definitions stored as embedding vectors                │
│  Grouped into 17 skill domains:                                  │
│    system, filesystem, web, identity, memory, agents,           │
│    communication, squads, tasks, skills, dashboard, storage,    │
│    email, messaging, github, integrations, trading              │
│                                                                  │
│  Built once on first request_tools call, persists in memory     │
└─────────────────────────────────────────────────────────────────┘
```

**How it works — round by round:**

```
Round 1: User says "Email john about the quarterly report"
  Agent has: 10 core tools (including request_tools)
  Agent calls: request_tools({"query": "email sending capabilities"})
  Librarian: embeds query → cosine search → returns email_send, email_read

Round 2: Tools hot-loaded into active round
  Agent now has: 10 core + email_send + email_read
  Agent calls: email_send({to: "john@...", subject: "Q4 Report"})

Round 3: Done ✅  (used 12 tools total, not 75)
```

**Architecture decisions:**
- **Agent-driven discovery**: The LLM forms the search query (it has intent), not a pre-filter guessing from the raw user message
- **Domain expansion**: Matching `email_send` also returns `email_read` — siblings come together
- **Round carryover**: Tools loaded in round N stay available in round N+1 (cleared per chat turn)
- **Swarm bypass**: Swarm/orchestrated agents get all tools (they're autonomous, no time for discovery)
- **Zero-cost search**: Uses the existing Ollama embedding pipeline — no API calls, no cloud costs

**Token savings:** ~5,000–8,500 tokens per request, freeing ~25% of a 32K context window for actual conversation.

**Files:**
- `engine/tool_index.rs` — `ToolIndex` struct, embedding, cosine similarity, domain mapping
- `engine/tools/request_tools.rs` — `request_tools` meta-tool (the librarian call)
- `engine/chat.rs` — `build_chat_tools()` filters to core + loaded tools
- `engine/agent_loop/mod.rs` — hot-loads newly discovered tools between rounds
- `engine/state.rs` — `tool_index` + `loaded_tools` on `EngineState`

### Extensibility Tiers

Pawz has a three-tier extensibility system:

| Tier | Format | Capabilities |
|------|--------|-------------|
| **Skill** (Tier 1) | `SKILL.md` | Prompt-only — Markdown instructions injected into agent context |
| **Integration** (Tier 2) | `pawz-skill.toml` | Credentials + binary detection + agent tools + dashboard widgets |
| **Extension** (Tier 3) | `pawz-skill.toml` | Custom sidebar views + persistent key-value storage |

Built-in integrations are compiled into the Rust binary. Community skills use the [skills.sh](https://skills.sh) ecosystem. The TOML manifest system for Tier 2/3 community integrations and extensions is implemented — TOML loader, PawzHub registry browser, dashboard widgets, skill output persistence, and extension storage are all functional.

### Community Skills (`skills/community/`)

The community skills subsystem connects to the [skills.sh](https://skills.sh) open-source directory:

- **Search** (`search.rs`) — queries the skills.sh API (`/api/search?q=`)
- **Install** (`store.rs`) — fetches SKILL.md from GitHub repos, stores locally
- **Parse** (`parser.rs`) — extracts YAML frontmatter + Markdown body
- **GitHub** (`github.rs`) — browses repo trees for SKILL.md files

Agent tools: `skill_search`, `skill_install`, `skill_list` — agents can find and install skills conversationally.

---

## TypeScript Frontend

### Views

Each view is a standalone TypeScript module that renders into its corresponding HTML container. Views manage their own state and DOM manipulation. No framework — pure `document.getElementById` / `innerHTML`.

### Feature Modules (Atomic Design)

Feature modules follow atoms → molecules → index pattern:
- **Atoms** — Pure functions, constants, type definitions. Zero side effects.
- **Molecules** — Functions that compose atoms and call Tauri IPC. May have side effects.
- **Index** — Barrel exports for the module.

### IPC Bridge

The frontend communicates with the Rust backend exclusively through Tauri's `invoke()` function. The `engine-bridge.ts` module wraps all IPC calls with TypeScript types.

Event-driven updates use Tauri's event system — the backend emits events (e.g., `engine-event` for streaming tokens, `agent-profile-updated` for real-time agent changes) and the frontend subscribes.

---

## Database

SQLite via Tauri's SQL plugin. Tables:

| Table | Purpose |
|-------|---------|
| `agent_modes` | Agent mode presets |
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
| `sessions` | Agent chat sessions |
| `messages` | Session message history |
| `config` | Agent and engine configuration |
| `memories` | Semantic memories (BM25 + vector) |
| `trade_history` | DEX/Solana trade log |
| `positions` | Open trading positions |
| `tasks` | Kanban tasks (with event triggers + persistent mode) |
| `task_activity` | Task activity log |
| `community_skills` | Installed community skills |
| `skill_outputs` | Dashboard widget data from skill output tool |
| `skill_storage` | Persistent key-value storage for extensions |
| `agent_messages` | Inter-agent direct messages and broadcasts |
| `squads` | Agent squad definitions |
| `squad_members` | Squad membership (agent + role) |

Credential fields encrypted with AES-256-GCM. Encryption key stored in OS keychain (macOS Keychain / Linux libsecret / Windows Credential Manager). 12-byte random nonce per field. Auto-migration from legacy XOR format.

---

## Quality

| Metric | Value |
|--------|-------|
| Rust tests | 242 (202 unit + 40 integration) |
| TypeScript tests | 360 (24 test files) |
| CI jobs | 3 parallel (Rust + TS + Security Audit) |
| Clippy warnings | 0 (enforced via `-D warnings`) |
| Known CVEs | 0 (`cargo audit` + `npm audit`) |
| Error handling | 12-variant typed `EngineError` (thiserror 2) |
| Credential encryption | AES-256-GCM |
| IPC commands | 158 |
| SQLite tables | 21 |
