# n8n Deep Integration Plan

> **Goal**: Turn n8n from a prompt-only TOML skill into a first-class integration
> that gives every OpenPawz agent instant access to 400+ services through n8n's
> node ecosystem — with zero custom tool code per service.
>
> **Architecture**: `Agent → MCP Client → n8n MCP Server → 400+ Nodes`

---

## Current State

| Component | Status | Limitation |
|-----------|--------|------------|
| n8n TOML skill (`resources/n8n/`) | Shipping | Prompt-only — agent uses raw `fetch` calls guided by instructions |
| MCP client system | Fully operational | stdio + SSE transports, auto-connect, tool namespacing |
| Inbound webhook server | Operational | n8n → OpenPawz triggering works but is manual HTTP setup |
| Nodes view | Read-only dashboard | Shows engine/provider/skill status, no n8n awareness |
| n8n guide (`guides/n8n.mdx`) | Documented | Covers REST API approach only, not MCP bridge |

**Key insight**: n8n v2+ ships an **MCP Server Trigger** node (`n8n-nodes-langchain.mcptrigger`)
that exposes any workflow as MCP tools. This means n8n can act as a universal MCP
server — every workflow becomes a callable tool, and OpenPawz already speaks MCP
natively.

---

## Phase 1: n8n Connection Manager (Settings UI)

**Goal**: Dedicated n8n configuration panel that replaces the generic TOML credential
entry with a guided connection experience.

### Tasks

1. **New settings tab**: `src/views/settings-n8n/` (atoms / molecules / index)
   - URL input with validation (ping `{url}/api/v1/workflows` on save)
   - API key input (masked, stored in encrypted vault)
   - Connection status indicator (connected / disconnected / error)
   - "Test Connection" button — hits `/api/v1/workflows?limit=1` to verify
   - Version display — fetch n8n version from API response headers

2. **Engine command**: `engine_n8n_test_connection` (Rust)
   - Takes `url` + `api_key`, attempts a GET to `/api/v1/workflows?limit=1`
   - Returns `{ connected: bool, version: string, workflow_count: number, error?: string }`
   - Validates URL format, handles timeouts, SSL errors

3. **Config persistence**: Store n8n config in channel config store (key `"n8n_config"`)
   ```typescript
   interface N8nConfig {
     url: string;
     api_key: string;       // encrypted in vault
     enabled: boolean;
     auto_discover: boolean; // Phase 2
     mcp_mode: boolean;      // Phase 3
   }
   ```

4. **Sidebar entry**: Add "n8n" item under Settings section with `account_tree` icon

5. **Upgrade existing TOML skill**: When n8n config is set via the new panel,
   auto-populate the TOML skill's `N8N_BASE_URL` and `N8N_API_KEY` credentials
   so the instruction-based approach still works as a fallback

### Acceptance Criteria

- [ ] User can enter n8n URL + API key in a dedicated settings panel
- [ ] "Test Connection" validates the connection and shows workflow count
- [ ] Config is encrypted and persisted across restarts
- [ ] Existing TOML skill credentials are auto-synced

---

## Phase 2: Workflow Discovery & Skill Mapping

**Goal**: Auto-discover n8n workflows and present them as "virtual skills" in the
Skills view and Capabilities card. Users can selectively enable workflows as
agent tools.

### Tasks

1. **Workflow discovery engine** (Rust):
   - `engine_n8n_list_workflows` — fetches all workflows from n8n API
   - `engine_n8n_get_workflow(id)` — fetches single workflow with full node graph
   - Returns `N8nWorkflow[]`:
     ```typescript
     interface N8nWorkflow {
       id: string;
       name: string;
       active: boolean;
       tags: string[];
       nodes: string[];      // node type names (e.g. "Slack", "Gmail", "GitHub")
       triggerType: 'webhook' | 'schedule' | 'manual' | 'event';
       createdAt: string;
       updatedAt: string;
     }
     ```

2. **Workflow browser UI** (`src/views/settings-n8n/workflows.ts`):
   - Grid/list of discovered workflows with name, status, node icons
   - Toggle to enable/disable each workflow as an agent tool
   - "Refresh" button to re-scan
   - Tag-based filtering
   - Node icon mapping (n8n node type → Material icon)

3. **Workflow → Skill mapping**:
   - Each enabled workflow registers as a virtual skill with:
     - ID: `n8n-wf-{workflow_id}`
     - Name: workflow name
     - Category: inferred from nodes (e.g. Slack nodes → "communication")
     - Description: auto-generated from node graph ("Sends Slack messages and creates GitHub issues")
   - Virtual skills appear in:
     - Skills settings (with n8n badge)
     - Capabilities card on Today view
     - Agent skill assignment

4. **Workflow-to-category inference** (atoms helper):
   - Map n8n node names to OpenPawz categories:
     ```
     Slack/Discord/Telegram/Teams → communication
     Gmail/Outlook/SendGrid      → communication
     GitHub/GitLab/Jira           → development
     Google Sheets/Notion/Airtable → productivity
     Shopify/Stripe/PayPal        → trading
     S3/Drive/Dropbox             → storage
     HTTP Request/Webhook         → web
     ...
     ```

5. **Periodic sync**: Optional background refresh every 10 minutes (configurable)
   to detect new/removed/changed workflows

### Acceptance Criteria

- [ ] All n8n workflows appear in a dedicated browser within settings
- [ ] Users can toggle individual workflows as agent-available skills
- [ ] Enabled workflows appear in the Capabilities card with correct categories
- [ ] Category inference works for the top 50 most common n8n nodes

---

## Phase 3: MCP Bridge (The Game Changer)

**Goal**: Connect to n8n's MCP Server Trigger so that n8n workflows become
native MCP tools — callable with structured input/output, not raw HTTP.

### Architecture

```
┌─────────────────┐     MCP (stdio/SSE)     ┌──────────────────────┐
│   OpenPawz      │ ◄──────────────────────► │   n8n Instance       │
│   MCP Client    │   tools/list, call_tool  │   MCP Server Trigger │
│   (registry.rs) │                          │   (per workflow)     │
└─────────────────┘                          └──────────────────────┘
         │                                            │
         ▼                                            ▼
   Agent sees tools:                          Workflow executes:
   - n8n_send_slack_message                   Slack → Gmail → Sheets
   - n8n_create_github_issue                  GitHub → Jira → Notify
   - n8n_run_daily_report                     Fetch → Transform → DB
```

### Tasks

1. **MCP Server auto-registration**:
   - When n8n config has `mcp_mode: true`, auto-create an `McpServerConfig`:
     ```rust
     McpServerConfig {
       id: "n8n-mcp",
       name: "n8n Workflows",
       transport: McpTransport::Sse,
       url: "{n8n_url}/mcp",      // n8n's MCP endpoint
       command: "",
       args: vec![],
       env: HashMap::new(),        // API key passed via SSE auth header
       enabled: true,
     }
     ```
   - Register in MCP registry alongside other MCP servers
   - Auto-connect on startup via `mcpConnectAll()`

2. **SSE transport auth extension** (Rust, `transport.rs`):
   - n8n's MCP Server requires authentication
   - Extend `McpServerConfig` with optional `auth_header: Option<String>`
   - SSE transport sends `Authorization: Bearer {api_key}` or
     `X-N8N-API-KEY: {api_key}` header on the EventSource connection
   - Fallback: env var injection if n8n accepts it

3. **Tool discovery enrichment**:
   - After MCP `tools/list` returns n8n tools, enrich with metadata:
     - Map tool names to friendly labels
     - Add n8n workflow ID cross-reference
     - Tag with source `"n8n"` for UI filtering
   - Merge with Phase 2 workflow metadata for richer display

4. **n8n MCP setup wizard** (UI):
   - Guide users through enabling MCP Server Trigger in n8n:
     1. "Add an MCP Server Trigger node to your workflow"
     2. "Define the tool name and input schema"
     3. "Activate the workflow"
     4. "Click 'Connect MCP' below"
   - Auto-detect if n8n instance has MCP endpoint available
   - One-click "Enable MCP Mode" toggle

5. **Hybrid fallback**:
   - If MCP connection fails, fall back to REST API + prompt injection (Phase 1 approach)
   - Surface connection mode in UI: "MCP (native)" vs "API (fallback)"
   - Log which path was used for debugging

### Acceptance Criteria

- [ ] n8n MCP Server auto-registers in MCP registry when enabled
- [ ] Agent sees n8n workflow tools alongside builtin + other MCP tools
- [ ] Tool calls route through MCP → n8n → node execution → response
- [ ] Fallback to REST API mode when MCP is unavailable
- [ ] Setup wizard guides users through n8n MCP configuration

---

## Phase 4: Native n8n Tools (Rust Engine)

**Goal**: Replace the prompt-only `fetch` approach with dedicated Rust tool
functions for core n8n operations — faster, more reliable, typed responses.

### Tasks

1. **New Rust tool module**: `src-tauri/src/engine/tools/n8n.rs`
   - `n8n_list_workflows` — returns formatted workflow list
   - `n8n_execute_workflow` — triggers a workflow by ID/name with optional input
   - `n8n_workflow_status` — checks if a workflow is active + last execution result
   - `n8n_activate_workflow` / `n8n_deactivate_workflow`
   - `n8n_list_executions` — recent execution history with status
   - `n8n_get_execution` — detailed execution result for debugging

2. **Register in `skill_tools()`**:
   ```rust
   "n8n" => tools.extend(n8n::definitions()),
   ```

3. **HTTP client** (`src-tauri/src/engine/tools/n8n.rs`):
   - Reuse Tauri's `reqwest` client
   - Read `n8n_config` from channel config store for URL + API key
   - Structured error responses (not raw HTTP errors)
   - Timeout: 30s for list/status, 120s for execution

4. **Smart workflow resolution**:
   - Accept workflow by name OR ID: `n8n_execute_workflow("Daily Report")`
   - Fuzzy name matching when exact match fails
   - Cache workflow list for 60s to avoid repeated API calls

5. **Execution polling**:
   - For long-running workflows, return execution ID immediately
   - Provide `n8n_poll_execution(id)` to check completion
   - Auto-summarize execution output (extract key data, skip node metadata)

### Acceptance Criteria

- [ ] 6 native n8n tools registered in the engine
- [ ] Agents can list, execute, monitor workflows without raw `fetch`
- [ ] Workflow name resolution works (fuzzy match)
- [ ] Long-running executions are handled gracefully
- [ ] All tools have typed input/output schemas

---

## Phase 5: Nodes View Enhancement

**Goal**: Transform the Nodes view from a generic status dashboard into a
visual integration hub showing all connected services — n8n nodes, MCP servers,
builtin skills — in a unified graph.

### Tasks

1. **Integration inventory card**:
   - Show total connected integrations count
   - Breakdown: `{X} via n8n · {Y} MCP servers · {Z} builtin skills`
   - Connection health indicators

2. **n8n workflow cards**:
   - Each enabled n8n workflow gets a card showing:
     - Workflow name + active/inactive status
     - Node icons (the services it connects to)
     - Last execution: timestamp + success/fail
     - "Run Now" action button
     - "Open in n8n" link

3. **Service dependency graph** (stretch goal):
   - Visual graph showing which services are connected through which pathways
   - e.g. "Gmail → via n8n workflow 'Email Digest' → Google Sheets"
   - Powered by workflow node data from Phase 2

4. **MCP server cards**:
   - Current MCP status cards (already exist) get enhanced with:
     - Tool count per server
     - Last tool call timestamp
     - Latency indicator

5. **Unified search**: Search across all integration types by name or service

### Acceptance Criteria

- [ ] Nodes view shows n8n workflows alongside MCP servers and skills
- [ ] Workflow cards display real execution status
- [ ] "Run Now" triggers workflow execution from the dashboard
- [ ] Integration count reflects all connection types

---

## Phase 6: Bidirectional Flow & Event System

**Goal**: Complete the loop — n8n can trigger OpenPawz agents via events,
and agents can subscribe to n8n workflow outcomes.

### Tasks

1. **n8n → OpenPawz webhook templates**:
   - Pre-built n8n workflow templates that call the Pawz webhook
   - One-click import into n8n via the API
   - Templates: "Alert agent on failure", "Daily summary to agent",
     "Forward webhook to agent"

2. **Execution event streaming**:
   - Poll n8n execution API for new completions (30s interval)
   - Emit Tauri events: `n8n:execution:complete`, `n8n:execution:failed`
   - Today view shows n8n execution feed alongside agent activity

3. **Agent-triggered workflow chains**:
   - Agent executes workflow A → result feeds into workflow B
   - Support for passing output data between sequential workflow calls
   - Error handling: if workflow A fails, don't trigger B

4. **Notification integration**:
   - Tray notifications for n8n workflow failures
   - Toast notifications for completed executions
   - Configurable: per-workflow notification preferences

### Acceptance Criteria

- [ ] n8n workflow templates are importable with one click
- [ ] Execution events appear in the Today view activity feed
- [ ] Sequential workflow chaining works with data passing
- [ ] Failure notifications surface in the system tray

---

## Implementation Priority

| Phase | Effort | Impact | Dependencies |
|-------|--------|--------|-------------|
| **Phase 1**: Connection Manager | 2-3 days | Foundation | None |
| **Phase 2**: Workflow Discovery | 2-3 days | High visibility | Phase 1 |
| **Phase 3**: MCP Bridge | 3-4 days | **Game changer** | Phase 1, MCP SSE auth |
| **Phase 4**: Native Tools | 2-3 days | Reliability | Phase 1 |
| **Phase 5**: Nodes View | 2-3 days | Polish | Phase 2 |
| **Phase 6**: Bidirectional | 2-3 days | Ecosystem | Phase 1, Phase 4 |

**Recommended order**: Phase 1 → 4 → 2 → 3 → 5 → 6

Phase 4 (native tools) before Phase 2 (discovery) because native tools give
immediate agent capability improvement, while discovery is a UI enhancement.
Phase 3 (MCP bridge) can be developed in parallel once Phase 1 lands.

---

## Technical Notes

### n8n MCP Server Trigger

n8n's MCP Server Trigger node (`n8n-nodes-langchain.mcptrigger`) exposes
workflows as MCP tools. Each workflow defines:
- **Tool name** — how agents invoke it
- **Input schema** — JSON Schema for parameters
- **Output** — workflow execution result

The MCP Server runs on the n8n instance at `{base_url}/mcp` (SSE transport).
This is the same protocol OpenPawz already speaks via `McpRegistry`.

### Auth Flow

```
Phase 1-2 (REST): Agent → fetch(url, {headers: {X-N8N-API-KEY: key}})
Phase 3   (MCP):  McpClient → SSE → n8n MCP endpoint (auth via header)
Phase 4   (Rust): reqwest::get(url).header("X-N8N-API-KEY", key)
```

All paths use the same API key. Phase 3 (MCP) may require n8n's internal
auth token for MCP endpoints specifically — this needs testing.

### File Inventory (Estimated)

```
src/views/settings-n8n/          # Phase 1-2
  atoms.ts                       # Types, helpers, node→category map
  atoms.test.ts                  # Unit tests
  molecules.ts                   # Connection panel UI
  workflows.ts                   # Workflow browser UI (Phase 2)
  index.ts                       # Orchestration + exports

src-tauri/src/engine/tools/n8n.rs  # Phase 4 — native tool module
src-tauri/src/commands/n8n.rs      # Phase 1 — test_connection command

src/views/nodes/molecules.ts       # Phase 5 — enhanced with n8n cards
src/views/today/molecules.ts       # Phase 6 — execution feed
```

### Compatibility

- **n8n v1.x**: REST API fully supported (Phases 1, 2, 4)
- **n8n v2.x**: REST API + MCP Server Trigger (all phases)
- **Self-hosted**: Full support (recommended)
- **n8n Cloud**: Full support (requires paid plan for API access)

---

## Future: Zapier Integration

This same architecture pattern applies to Zapier:

| n8n Concept | Zapier Equivalent |
|-------------|-------------------|
| Workflows | Zaps |
| MCP Server Trigger | Zapier NLA (Natural Language Actions) API |
| REST API | Zapier API (limited) |
| Self-hosted | Cloud-only |
| 400+ nodes | 7,000+ apps |

Phase 1-2 structure (settings panel + discovery) can be reused for Zapier
with a `settings-zapier/` view. Phase 3 (MCP) doesn't apply to Zapier —
instead, Zapier NLA provides a REST-based tool calling interface that would
need a dedicated Rust tool module similar to Phase 4.

A `ZAPIER_INTEGRATION_PLAN.md` will follow once n8n phases are stable.
