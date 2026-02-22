# UX Overhaul Plan

> **Status**: Active  
> **Last updated**: 2026-02-22  
> **Owner**: @elisplash

Full audit of every frontend view — what's real, what's broken, what's missing.

---

## Current Sidebar (18 items)

| # | Nav Item | View ID | Verdict | Action |
|---|----------|---------|---------|--------|
| 1 | Dashboard | `dashboard` | **Keep** | First-run setup flow, feature cards |
| 2 | Today | `today` | **Fix** | Connect to real Kanban tasks (not localStorage) |
| 3 | Chat | `chat` | **Keep** | Core feature — works well |
| 4 | Agents | `agents` | **Keep** | Works — CRUD, avatars, mini-chat |
| 5 | Tasks | `tasks` | **Keep** | Works — Kanban board with cron |
| 6 | Orchestrator | `orchestrator` | **Merge** | Absorb into Tasks as a "Projects" tab |
| 7 | Projects | `code` | **Rename** | Rename to "Files" — it's just a file browser |
| 8 | Content | `content` | **Remove** | Too thin — textarea + AI improve can live in chat |
| 9 | Mail | `mail` | **Keep** | Works — email client |
| 10 | Automations | `automations` | **Merge** | Absorb into Tasks as a "Scheduled" tab |
| 11 | Channels | `channels` | **Keep** | Works — all 11 bridges |
| 12 | Research | `research` | **Keep** | Works — but needs error recovery |
| 13 | Nodes | `nodes` | **Rename** | Rename to "Engine" — it's a status page, not nodes |
| 14 | Trading | `trading` | **Keep** | Works — dashboard + policies |
| 15 | Memory | `memory` | **Fix** | Add per-agent scoping + delete |
| 16 | Skills | `skills` | **Remove** | Dead stub — real skills are in Settings > Skills |
| 17 | Foundry | `foundry` | **Keep** | Works — models + chat modes |
| 18 | Settings | `settings` | **Keep** | Works — all sub-tabs |

---

## Proposed New Sidebar (14 items + 2 new)

```
Today             ← fixed: uses real Kanban tasks
Chat
Agents
Squads            ← NEW: real-time squad chat + planning
Tasks             ← merged: Kanban + Scheduled tab + Projects tab
Files             ← renamed from "Projects"
Mail
Channels
Research
Trading
Memory            ← fixed: per-agent scoping + delete
Foundry
Engine            ← renamed from "Nodes"
Settings
```

Removed: Dashboard (merge into Today), Content (too thin), Skills (dead stub), Automations (merged), Orchestrator (merged).

---

## Phase 1 — Quick Wins (no new features, just cleanup)

### 1.1 Remove dead `skills.ts` stub
- Both exported functions are no-ops
- Real skills management is in `settings-skills/`
- Remove `skills-view` nav item from `index.html`
- Remove `skills.ts` view file

### 1.2 Rename "Nodes" → "Engine"
- Change nav label and icon in `index.html`
- Remove noop pairing functions from `nodes/atoms.ts`
- Remove pairing imports from `main.ts`
- Update view description text

### 1.3 Rename "Projects" → "Files"
- Change nav label in `index.html` from "Projects" to "Files"
- Change icon from `folder_open` to `description` or keep `folder_open`

### 1.4 Remove "Content" view
- Remove nav item from `index.html`
- Archive `content.ts` (or delete — it's 158 lines)
- The AI Improve feature can be replicated with a slash command

### 1.5 Remove "Dashboard" as separate view
- Merge first-run setup flow into Today view
- The feature cards from dashboard just link to other views
- Today already has weather, tasks, agent greeting, quick actions

---

## Phase 2 — Merge Overlap

### 2.1 Merge Automations into Tasks
Currently Automations is a filtered view of tasks with `cron_schedule`. Merge it:

- **Tasks view** gets 3 tabs: **Board** (Kanban) | **Scheduled** (cron tasks) | **Activity** (run history)
- Board tab = current tasks Kanban
- Scheduled tab = current automations view (active/paused columns + run history)
- Activity tab = existing activity feed (currently below the board)
- Remove `automations-view` nav item
- Remove `automations/` view directory

### 2.2 Merge Orchestrator into Tasks
Orchestrator manages multi-agent projects with boss/worker pattern. Merge as a 4th tab:

- **Tasks view** gets a 4th tab: **Projects** (multi-agent orchestration)
- Shows project cards, agent roster, live message bus — exact same UI, just housed in Tasks
- Remove `orchestrator-view` nav item
- Move `orchestrator/` rendering into `tasks/` as a tab

---

## Phase 3 — Fix Existing Views

### 3.1 Fix Today: Use Real Tasks
**Problem**: Today's checklist uses `localStorage`, completely separate from the Kanban task board.

**Fix**: Replace localStorage tasks with `pawEngine.tasksList()` filtered by agent or status:
- Show today's assigned/in-progress tasks from the real Kanban
- "Add task" creates a real task in the engine (status: inbox)
- Toggle marks task as done in the engine
- Remove `localStorage.getItem('paw-tasks')` entirely

### 3.2 Fix Memory: Per-Agent Scoping
**Problem**: All memories are global. No separation between system and per-agent memories.

**Fix**:
- Add agent filter dropdown to Memory Palace (default: "All Agents" / "System")
- Backend already stores `agent_id` on memories — just need to filter in UI
- Show agent avatar next to each memory card
- "Remember" form includes agent selector
- Memory stats show per-agent breakdown

### 3.3 Fix Memory: Add Delete
**Problem**: No way to delete individual memories from the UI.

**Fix**: Add delete button (trash icon) on each memory card with confirmation.

### 3.4 Fix Engine (Nodes): Clean Up
- Remove all noop pairing stubs
- Add fullscreen toggle for the status dashboard
- Show active task count, running channels, connected MCP servers
- Make it a proper system overview

---

## Phase 4 — New Feature: Squads View

### 4.1 Squads Workspace
A dedicated sidebar view for managing and observing agent squads in real-time.

**Layout**:
```
┌──────────────────────────────────────────────────┐
│ Squads                                     [+ New] │
├──────────┬───────────────────────────────────────┤
│ Squad    │ Alpha Team                            │
│ List     │ Goal: Research competitor landscape   │
│          │ Status: active  Members: 3            │
│ ┌──────┐ ├───────────────────────────────────────┤
│ │Alpha │ │ ┌─────────────────────────────────┐   │
│ │Team  │ │ │ Message Feed (real-time)        │   │
│ ├──────┤ │ │                                 │   │
│ │Beta  │ │ │ Alice (coordinator):            │   │
│ │Squad │ │ │   "Starting research on X..."   │   │
│ └──────┘ │ │                                 │   │
│          │ │ Bob (member):                    │   │
│          │ │   "Found 3 competitors..."      │   │
│          │ │                                 │   │
│          │ └─────────────────────────────────┘   │
│          │ ┌─────────────────────────────────┐   │
│          │ │ [Send message to squad...]      │   │
│          │ └─────────────────────────────────┘   │
│          ├───────────────────────────────────────┤
│          │ Members: Alice (coord) | Bob | Eve    │
│          │ [+ Add Member] [Edit Goal] [Disband]  │
└──────────┴───────────────────────────────────────┘
```

**Features**:
- List all squads in left sidebar
- Create squad modal (name, goal, pick agents)
- Real-time message feed showing inter-agent communication
- Poll `agent_messages` table filtered by squad channel
- Listen to `agent-message` Tauri event for live updates
- Send message to squad from UI (as the user, routed through active agent)
- Member management (add/remove agents, change roles)
- Squad goal editing and status updates

**Backend needed**: Add Tauri commands for:
- `engine_list_squads` → calls `store.list_squads()`
- `engine_create_squad` → calls `store.create_squad()`
- `engine_update_squad` → calls `store.update_squad()`
- `engine_delete_squad` → calls `store.delete_squad()`
- `engine_squad_members` → add/remove
- `engine_squad_messages` → calls `store.get_agent_messages()` filtered by squad channel

---

## Phase 5 — Missing Features

These are capabilities a platform like Pawz should have but currently doesn't:

### 5.1 Activity Timeline / Agent Feed
**What**: A global feed showing what ALL agents are doing across the system — tool calls, messages sent, tasks completed, research started, etc.

**Why**: Right now you can only see an agent's activity by opening its chat session. There's no bird's-eye view of system-wide agent activity.

**Where**: Could be a tab in Today view or a standalone "Activity" view.

### 5.2 Notifications Center
**What**: A notification bell/drawer showing unread items:
- Agent messages waiting for you
- Tasks completed or failed
- Webhook events received
- Channel messages requiring approval
- Tool calls awaiting HIL approval

**Why**: Events happen across many views — you can miss them. A unified notification center prevents that.

**Where**: Top bar notification icon with dropdown drawer.

### 5.3 Agent Handoff Protocol
**What**: A structured way for agents to hand work to each other:
- Agent A completes part of a task → hands remaining work to Agent B
- The handoff includes context, files, and state
- Agent B picks up with full awareness of what A did

**Why**: Currently squads can message each other, but there's no structured "handoff" pattern. This makes multi-agent workflows more reliable.

**Where**: Built into the agent_comms tool system.

### 5.4 Webhook Event Log
**What**: A view showing all received webhook events — timestamp, path, payload preview, which task was triggered.

**Why**: Webhooks exist but there's no visibility into what's arriving. Debugging webhook integrations is blind.

**Where**: Could be a tab in the Engine view or in Settings > Webhooks.

### 5.5 Quick Agent Switcher
**What**: A keyboard shortcut (Cmd+K or Cmd+J) that opens a quick-switch palette to jump between agents, views, or actions.

**Why**: With 14+ views and multiple agents, navigation is slow. Power users need fast switching.

**Where**: Global overlay, like VS Code's command palette.

---

## Implementation Priority

### Sprint 1: Cleanup (no new code, just removals + renames)
- [x] 1.1 Remove dead `skills.ts` stub
- [x] 1.2 Rename Nodes → Engine
- [x] 1.3 Rename Projects → Files
- [x] 1.4 Remove Content view
- [x] 1.5 Merge Dashboard into Today

### Sprint 2: Merge Overlap
- [x] 2.1 Merge Automations into Tasks (Scheduled tab)
- [x] 2.2 Merge Orchestrator into Tasks (Projects tab)

### Sprint 3: Fix Existing
- [x] 3.1 Today: use real Kanban tasks
- [x] 3.2 Memory: per-agent scoping
- [x] 3.3 Memory: add delete
- [x] 3.4 Engine: clean up and improve

### Sprint 4: New Features
- [x] 4.1 Squads view (real-time agent team workspace)
- [x] 5.1 Activity timeline
- [x] 5.2 Notifications center

### Sprint 5: Polish
- [ ] 5.3 Agent handoff protocol
- [ ] 5.4 Webhook event log
- [x] 5.5 Quick agent switcher

---

## Success Criteria

- [x] Sidebar has ≤15 items (down from 18)
- [x] No dead/stub views in navigation
- [x] No duplicate data stores (Today tasks = Kanban tasks)
- [x] Memory supports per-agent filtering
- [x] Squads have a real UI workspace
- [ ] Every backend feature has a corresponding frontend surface
