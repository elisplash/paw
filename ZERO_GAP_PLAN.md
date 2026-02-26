# Zero-Gap Automation — Architect / Worker Design

> **Vision**: If a tool doesn't exist, build it on the fly. Every agent has access
> to 25,000+ automations — they discover, install, and use them autonomously.
>
> **Architecture**: `Opus (Architect) → MCP → n8n Bridge → Qwen (Worker/Foreman)`
>
> **Status**: Phases 0–5 shipped. Phase 6.1 shipped. Phase 6.2 + 7 in progress.

---

## System Overview

```
┌─────────────────────────────────────────────────────────────────┐
│  OpenPawz Desktop                                               │
│                                                                 │
│  ┌──────────────────────────────────────────────┐               │
│  │  ARCHITECT (Opus 4.6 / Cloud)                │               │
│  │  • Analyzes user intent                      │               │
│  │  • Plans multi-step automations              │               │
│  │  • Evaluates community nodes (stars, safety) │               │
│  │  • Issues Task Orders to Worker              │               │
│  └──────────┬───────────────────────────────────┘               │
│             │  Task Orders (structured JSON)                    │
│  ┌──────────▼───────────────────────────────────┐               │
│  │  WORKER / FOREMAN (Qwen 3.5 / Local Ollama)  │              │
│  │  • Executes MCP tool calls                    │              │
│  │  • Handles n8n node installation              │              │
│  │  • Manages JSON payloads & retries            │              │
│  │  • Zero commentary — pure execution           │              │
│  └──────────┬───────────────────────────────────┘               │
│             │  MCP Protocol (SSE transport)                     │
│  ┌──────────▼───────────────────────────────────┐               │
│  │  n8n ENGINE (Embedded Docker/npx)             │              │
│  │  • 405 built-in service integrations          │              │
│  │  • 25,000+ community nodes (NCNodes)          │              │
│  │  • MCP Server Trigger workflows               │              │
│  │  • REST API for node install/workflow CRUD     │              │
│  └──────────────────────────────────────────────┘               │
└─────────────────────────────────────────────────────────────────┘
```

---

## What Already Exists (Shipped)

These are done, committed, and pushed. No work needed.

### ✅ Phase 1 — MCP Bridge Core
- [x] SSE transport (`src-tauri/src/engine/mcp/transport.rs` — `SseTransport`)
- [x] Transport abstraction (`McpTransportHandle` enum — Stdio + SSE)
- [x] `McpClient` SSE support (`client.rs` — branches on `config.transport`)
- [x] Auto-registration in `McpRegistry` (`register_n8n()`, `N8N_MCP_SERVER_ID`)
- [x] n8n MCP auto-connect on `engine_n8n_ensure_ready`
- [x] Tool routing: `mcp_` prefix → MCP server dispatch in `chat.rs` + `tools/mod.rs`

### ✅ Phase 2 — Community Node API + MCP Workflows
- [x] `engine_n8n_community_packages_list` — GET installed packages
- [x] `engine_n8n_community_packages_install` — POST npm install (120s timeout)
- [x] `engine_n8n_community_packages_uninstall` — DELETE package
- [x] `engine_n8n_deploy_mcp_workflow` — create/update workflow with MCP trigger
- [x] n8n startup env: `N8N_COMMUNITY_PACKAGES_ENABLED=true`
- [x] n8n startup env: `N8N_COMMUNITY_PACKAGES_ALLOW_TOOL_USAGE=true`
- [x] n8n startup default: `mcp_mode: true`
- [x] Frontend: MCP bridge status badge in Integrations view
- [x] Setup guide: auto-deploys MCP workflow after credential save

### ✅ Phase 0 — Foundation (Earlier Commits)
- [x] Credential provisioning fix (direct HashMap pass, bypasses config store)
- [x] Catalog audit: 405 services, 153 validated node types, 0 fake types
- [x] `map_service_to_node_type` expanded from 37→72 mappings

---

## What Needs Building

### ✅ Phase 3 — NCNodes Discovery (`search_ncnodes` tool)
**Goal**: Agents can search 25,000+ community packages and evaluate them.

#### ✅ 3.1 — NCNodes Search Backend
- [x] `engine_n8n_search_ncnodes` command — npm registry search with `n8n-community-node-package` keyword
- [x] `NCNodeResult` struct — package_name, description, author, version, weekly_downloads, last_updated, repository_url, keywords
- [x] Registered in `lib.rs`

#### ✅ 3.2 — NCNodes Search as Agent Tool
- [x] `search_ncnodes` tool definition in `tools/n8n.rs`
- [x] Executor calls `engine_n8n_search_ncnodes` internally
- [x] Added to SAFE_TOOLS in `agent_loop.rs` (read-only, no HIL)

#### ✅ 3.3 — Install Node as Agent Tool
- [x] `install_n8n_node` tool definition
- [x] Executor calls `engine_n8n_community_packages_install` + auto-deploys MCP workflow + auto-refreshes tools
- [x] NOT in SAFE_TOOLS — requires HIL approval

#### ✅ 3.4 — MCP Refresh as Agent Tool
- [x] `mcp_refresh` tool definition
- [x] Executor refreshes tools from MCP registry
- [x] Added to SAFE_TOOLS

---

### ✅ Phase 4 — Architect / Worker Handoff
**Goal**: Opus sends high-level Task Orders, Qwen executes them locally.

#### ✅ 4.1 — Worker Agent Profile (Ollama/Qwen)

The Worker is a standard OpenPawz agent configured with:
- **Provider**: Ollama (local)
- **Model**: `worker-qwen` (custom Modelfile — see Phase 4.3)
- **Role**: `worker`
- **Specialty**: `automation-executor`
- **System Prompt**: Silent executor — no chat, just tool calls

**What already exists**:
- `resolve_provider_for_model()` in `sub_agent.rs` — detects Ollama models by `:` in name
- `ModelRouting.worker_model` — can set default worker model
- `AgentRole::Worker` in `agent_loop.rs` — full worker loop with `report_progress`
- `delegate_task` boss tool — sends task to a named worker agent
- Per-agent model override via `agent.model` field

- [x] Add `automation-executor` to specialty enum in `tools.rs` `create_sub_agent`
- [x] Add model routing preset: when specialty = `automation-executor`, route to Ollama
- [x] Create default worker agent profile (name: `foreman`, model: `worker-qwen:latest`)
- [x] Worker gets `search_ncnodes`, `install_n8n_node`, `mcp_refresh` + all `mcp_*` tools

#### ✅ 4.2 — Task Order Protocol

The Architect doesn't call MCP tools directly for execution-heavy work.
Instead, it delegates via the existing `delegate_task` boss tool:

```
Architect (Opus) calls:
  delegate_task(
    agent_id: "foreman",
    task_description: "Install n8n-nodes-puppeteer, deploy MCP workflow, then use mcp_n8n_puppeteer_screenshot to capture https://example.com",
    context: "The user needs a screenshot of example.com. Package has 142 stars, last updated 3 months ago."
  )

Foreman (Qwen) executes:
  1. install_n8n_node("n8n-nodes-puppeteer")  → waits for HIL approval
  2. mcp_refresh()
  3. mcp_n8n_puppeteer_screenshot({ url: "https://example.com" })
  4. report_progress(status: "done", summary: "Screenshot captured at /tmp/shot.png")
```

**What already exists**: This flow works TODAY with `delegate_task` → `run_sub_agent()`.
The only gap is that worker agents need the n8n-specific tools added to their tool set.

- [x] In `sub_agent.rs`, add n8n tools to worker tool set when specialty = `automation-executor`
- [x] Architect system prompt addition: "For automation tasks, delegate to the foreman agent"

#### ✅ 4.3 — Qwen Modelfile

**File**: `resources/ollama/worker-qwen.Modelfile`

```Dockerfile
FROM qwen3.5:35b-a3b

PARAMETER temperature 0
PARAMETER num_ctx 16384
PARAMETER stop "<|im_end|>"

SYSTEM """
You are the LOCAL FOREMAN (Worker Agent) for OpenPawz.

Your job is to receive Task Orders from the Architect and translate them into
precise MCP tool calls. You are a silent execution unit.

## Rules
1. NEVER engage in conversation. Output tool calls only.
2. If told to install: call `install_n8n_node` with the exact package name.
3. If told to execute: map parameters to the MCP tool schema exactly.
4. If a tool call fails, retry once with corrected parameters.
5. After completing all steps, call `report_progress` with status "done".
6. If blocked, call `report_progress` with status "blocked" and the error.

## Available Tool Prefixes
- `mcp_n8n_*` — Tools from the n8n MCP bridge (25,000+ possible)
- `install_n8n_node` — Install a community package
- `search_ncnodes` — Search for packages
- `mcp_refresh` — Refresh available tools after install
- `report_progress` — Report completion/blockers to Architect
"""
```

- [x] Create Modelfile at `resources/ollama/worker-qwen.Modelfile`
- [x] Add `engine_ollama_setup_worker` command — one-click: check/pull base model + create worker-qwen
- [x] `engine_ollama_list_models`, `engine_ollama_has_model`, `engine_ollama_pull_model`, `engine_ollama_create_model`
- [x] Wire into first-run setup: detect Ollama → offer to create worker model
- [ ] Smaller alternative: `worker-qwen-small` using `qwen3.5:8b` for lighter machines

#### ✅ 4.4 — Auto-Setup Flow

When a user first sets up OpenPawz with Ollama configured:

```
1. Detect Ollama provider is configured
2. Check if `worker-qwen` model exists (ollama list)
3. If not: prompt "Create local automation worker? (Qwen 3.5, ~20GB)"
4. If yes: ollama create worker-qwen -f worker-qwen.Modelfile
5. Create "foreman" agent with model=worker-qwen, specialty=automation-executor
6. Set model_routing.worker_model = "worker-qwen:latest"
7. Done — all delegate_task calls to "foreman" now use local Qwen
```

- [x] `engine_ollama_list_models` — check available models
- [x] `engine_ollama_create_model` — create from Modelfile
- [x] Auto-setup UI in Settings → Providers → Ollama section ("Setup Worker Agent" button)
- [ ] First-run detection in `main.ts`

---

### ✅ Phase 5 — Tool Name Remapping + Agent Access
**Goal**: Clean tool names, per-service access control.

#### ✅ 5.1 — Tool Name Remapping
**File**: `src-tauri/src/engine/mcp/registry.rs`

Current: `mcp_n8n_SendSlackMessage` (raw n8n operation names)
Target: `slack_send_message` (service-native, clean)

```rust
// In mcp_tool_to_paw_def(), when server_id == "n8n":
fn remap_n8n_tool_name(raw_name: &str) -> String {
    // "Gmail_SendEmail" → "gmail_send_email"
    // "Slack_SendMessage" → "slack_send_message"
    // Apply snake_case conversion + service prefix extraction
}
```

- [x] Implement `pascal_to_snake()` with camelCase/PascalCase → snake_case conversion
- [x] n8n-specific branch in `mcp_tool_to_paw_def()`: `mcp_n8n_gmail_send_email` format
- [x] `[n8n automation]` tag in descriptions for n8n tools
- [ ] Build service→prefix mapping from catalog (405 services)
- [ ] Add enriched descriptions from catalog service descriptions
- [ ] Maintain reverse mapping for dispatch (clean name → raw MCP name)

#### 5.2 — Per-Service Agent Access Control
**File**: `src/views/agents.ts` (UI) + `src-tauri/src/engine/tools/mod.rs` (enforcement)

Users can toggle which services each agent can use:
- "Give Luna access to: Slack, Gmail, GitHub" ✅
- "Block trading-bot from: Email, Slack" 🚫

**What already exists**:
- `agent.capabilities: Vec<String>` — per-agent tool whitelist
- `sub_agent.rs` filters tools by capabilities before running worker
- `agent-policies` feature module — per-agent tool policies

- [ ] UI: service toggles on agent config panel
- [ ] Map service IDs to tool name prefixes
- [ ] Filter `mcp_n8n_*` tools by agent's allowed services
- [ ] Inherit from agent groups/squads

---

### Phase 6 — Community Node Browser UI
**Goal**: Visual search, install, and manage 25K+ packages from the app.

#### ✅ 6.1 — Browse View
**Files**: `src/views/integrations/community/` (atoms + molecules + index)

- [x] Atomic file structure: atoms.ts (types/helpers), molecules.ts (DOM/IPC), index.ts (barrel)
- [x] Search input with debounced ncnodes query
- [x] Package cards: name, downloads, description, install button
- [x] Installed packages tab (from `community_packages_list`)
- [x] Install progress indicator (spinner + status)
- [x] Auto-deploy MCP workflow after install
- [x] Uninstall button with confirmation
- [x] CSS: `_community-browser.css` imported in index.css
- [x] Wired as "Community" tab in integrations main tabs

#### 6.2 — On-Demand Auto-Install

When a user clicks a catalog service that requires a community node not yet installed:

```
User clicks "Redis" in catalog
  → Check: is n8n-nodes-redis installed?
  → No: "Redis requires the n8n-nodes-redis package. Install now?"
  → User confirms → install → deploy MCP workflow → ready
```

- [ ] Map catalog services to required community packages
- [ ] Check installed packages against requirements
- [ ] Install prompt with package metadata (stars, size, author)
- [ ] Automatic MCP workflow deployment after install

---

### Phase 7 — End-to-End Verification
**Goal**: Prove the full pipeline works: search → install → deploy → use.

#### 7.1 — MCP Trigger URL Format
The n8n MCP Server Trigger exposes tools per-workflow. Need to verify:
- What's the SSE endpoint URL after workflow activation?
- Is it `{n8n_url}/mcp/{workflow_id}/sse` or something else?
- Does the trigger auto-register on workflow activation?

- [ ] Deploy a test MCP workflow manually
- [ ] Inspect n8n logs for registered MCP endpoint
- [ ] Verify SSE connection from our `SseTransport`
- [ ] Update `register_n8n()` URL format if needed

#### 7.2 — Integration Test Script

```bash
# Full pipeline test:
1. Start n8n engine (ensure_ready)
2. Verify MCP bridge connects
3. Search ncnodes for "puppeteer"
4. Install n8n-nodes-puppeteer
5. Deploy MCP workflow for puppeteer
6. Refresh MCP tools
7. Verify puppeteer tools appear in tool list
8. Execute a puppeteer screenshot
9. Verify result
```

- [ ] Write integration test in `src-tauri/tests/`
- [ ] Add to CI (requires Docker for n8n)

---

## Adaptive Thinking Protocol (Architect — Opus 4.6)

When the Architect receives a task requiring automation:

```
<thinking>
1. Does the required tool exist in current `list_tools`?
   → YES: Execute directly or delegate to Foreman
   → NO: Continue to step 2

2. Search for the capability:
   search_ncnodes("puppeteer browser screenshot")
   
3. Evaluate results (in thinking block):
   - n8n-nodes-puppeteer: ⭐142, updated 2 months ago ✅
   - n8n-nodes-browserless: ⭐12, updated 2 years ago ❌
   
4. Issue Task Order to Foreman:
   delegate_task(
     agent_id: "foreman",
     task_description: "Install n8n-nodes-puppeteer and take a screenshot of https://example.com",
     context: "Package verified: 142 stars, maintained, safe to install."
   )
   
5. Monitor Foreman progress via check_agent_status
</thinking>
```

### Safety Rules (Evaluated in `<thinking>`)

Before ordering any package installation, the Architect MUST evaluate:

| Check | Threshold | Action if Failed |
|-------|-----------|-----------------|
| GitHub stars | ≥ 10 | Warn user, suggest alternative |
| Last updated | < 1 year ago | Warn if stale, still allow |
| Known malware | Any match | Block, report to user |
| Scope overlap | > 5 tools overlapping | Suggest existing tool instead |

---

## Infrastructure Config

### Ollama Worker Setup

```bash
# One-time setup (automated by Phase 4.4)
ollama create worker-qwen -f resources/ollama/worker-qwen.Modelfile
```

### Model Routing Config

In OpenPawz Settings → Models:
```json
{
  "model_routing": {
    "boss_model": "claude-opus-4-20250514",
    "worker_model": "worker-qwen:latest",
    "auto_tier": true,
    "specialty_models": {
      "automation-executor": "worker-qwen:latest"
    }
  }
}
```

### n8n Engine Config

Env vars set automatically on n8n startup:
```env
N8N_COMMUNITY_PACKAGES_ENABLED=true          # ✅ Shipped
N8N_COMMUNITY_PACKAGES_ALLOW_TOOL_USAGE=true  # ✅ Shipped
N8N_MCP_SERVER_MODE=true                       # ✅ Shipped (via mcp_mode)
```

---

## File Map (New + Modified)

| File | Status | Description |
|------|--------|-------------|
| `src-tauri/src/engine/mcp/transport.rs` | ✅ Done | SSE transport + McpTransportHandle |
| `src-tauri/src/engine/mcp/client.rs` | ✅ Done | Transport-agnostic MCP client |
| `src-tauri/src/engine/mcp/registry.rs` | ✅ Done | n8n auto-registration + pascal_to_snake remapping |
| `src-tauri/src/commands/n8n.rs` | ✅ Done | Community API + ncnodes search |
| `src-tauri/src/engine/tools/n8n.rs` | ✅ Done | Agent tools for ncnodes search/install/refresh |
| `src-tauri/src/engine/tools/mod.rs` | ✅ Done | Wire new n8n tools |
| `src-tauri/src/engine/orchestrator/sub_agent.rs` | ✅ Done | automation-executor system prompt + tool wiring |
| `src-tauri/src/engine/orchestrator/tools.rs` | ✅ Done | automation-executor specialty |
| `resources/ollama/worker-qwen.Modelfile` | ✅ Done | Qwen silent executor profile |
| `src-tauri/src/commands/ollama.rs` | ✅ Done | Ollama model management (5 commands) |
| `src/views/settings-advanced/molecules.ts` | ✅ Done | Worker setup UI button |
| `src/views/integrations/community/atoms.ts` | ✅ Done | Community browser types + helpers |
| `src/views/integrations/community/molecules.ts` | ✅ Done | Community browser DOM + IPC |
| `src/views/integrations/community/index.ts` | ✅ Done | Community browser barrel |
| `src/styles/_community-browser.css` | ✅ Done | Community browser stylesheet |
| `src/views/integrations/community-browser.ts` | 🔲 Phase 6 | Package browser UI |
| `src-tauri/tests/n8n_mcp_e2e.rs` | 🔲 Phase 7 | Integration test |
| `src-tauri/src/lib.rs` | ✅ Done + Phase 3 | Register new commands |

---

## Priority Order

```
Phase 3 (NCNodes Search)     — HIGH   — Unlocks discovery
Phase 4 (Architect/Worker)   — HIGH   — Unlocks delegation
Phase 5 (Tool Remapping)     — MEDIUM — Polish / UX
Phase 6 (Browser UI)         — MEDIUM — User-facing convenience  
Phase 7 (E2E Verification)   — HIGH   — Proves it all works
```

Recommended execution: **3 → 7 → 4 → 5 → 6**
(Build search, prove it works, then wire the delegation, then polish.)

---

## The One-Liner

> *"Opus thinks, Qwen executes, n8n connects, MCP bridges — zero gaps."*
