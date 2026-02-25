# Skills & PawzHub Workspace Overhaul — Plan

> Internal planning doc. Not shipped.

---

## The Problem (3 things)

### 1. Everything is dumped into one "Skills" page
Right now, clicking Skills shows you **everything** in a single scroll: a wizard, PawzHub browser, community browser, extensions, and 400+ built-in integration cards. There's no separation between:

- **Prompt-only skills** (`.md` — passive context injected into the agent)
- **Tool integrations** (TOML with credentials — active tools the agent calls)
- **MCP servers** (external tool servers — buried in Settings, not even in Skills)
- **Dashboard extensions** (TOML with `[widget]` — live data on the Today page)
- **Full extensions** (TOML with `[view]` — custom sidebar tabs)

A user can't tell what's what, what's active, what needs setup, or what each thing actually does for them.

### 2. Not modular — everything ships baked in
All 400+ built-ins are compiled into the binary. The user gets everything whether they want it or not. There's no concept of "my workspace" vs "available to add." It should feel like a phone's home screen vs app store.

### 3. No discovery moment for PawzHub
PawzHub exists as a registry and an iframe view, but there's no natural flow where a user discovers new capabilities. The iframe just loads the whole docs site — it's not a marketplace experience.

---

## The Fix — Three Spaces, Clear Lifecycle

### New Sidebar Structure

```
── Work ──────────────
   Today
   Chat
   Agents
   Tasks
   Squads
   Files

── Connect ───────────
   Mail
   Channels
   Research
   Trading

── Workspace ─────────     ← NEW section name
   My Skills                ← replaces "Skills"
   PawzHub                  ← marketplace/store
   Foundry

── System ────────────
   Memory
   Engine
   Settings
```

**Key changes:**
- "Skills" → **"My Skills"** (what you have, what's active)
- "PawzHub" stays but becomes a real **store/marketplace**, not an iframe to docs
- MCP servers move **out of Settings** and into My Skills as a first-class tab
- "Workspace" section name makes it clear: this is your setup

---

## My Skills — The Workspace View

The current single-scroll page becomes a **tabbed workspace** with clear status at a glance.

### Tab Layout

```
┌──────────────────────────────────────────────────────────────────┐
│  My Skills                                              [+ Add] │
│                                                                  │
│  ┌─────────┬──────────────┬─────────┬────────────┬─────────────┐ │
│  │ Active  │ Integrations │  Tools  │ Extensions │  Create     │ │
│  │  (12)   │     (5)      │  (3)    │    (1)     │             │ │
│  └─────────┴──────────────┴─────────┴────────────┴─────────────┘ │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────────┐ │
│  │  📊 Summary Bar                                             │ │
│  │  12 active · 5 need setup · 3 MCP servers · 2 widgets       │ │
│  └──────────────────────────────────────────────────────────────┘ │
│                                                                  │
│  ┌────────────────────┐  ┌────────────────────┐                  │
│  │ 📧 Email           │  │ 💬 Slack            │                 │
│  │ Integration        │  │ Integration         │                 │
│  │ ● Configured       │  │ ⚠ Needs API key     │                │
│  │ 2 tools · widget   │  │ 1 tool              │                │
│  │ [Configure] [···]  │  │ [Set up] [···]      │                │
│  └────────────────────┘  └────────────────────┘                  │
│                                                                  │
│  ┌────────────────────┐  ┌────────────────────┐                  │
│  │ 🔧 MCP: filesystem │  │ 🌤 Weather          │                │
│  │ MCP Server         │  │ Skill (prompt)      │                │
│  │ ● Connected (4)    │  │ ● Enabled           │                │
│  │ 4 tools available  │  │ No config needed    │                │
│  │ [Disconnect] [···] │  │ [Disable] [···]     │                │
│  └────────────────────┘  └────────────────────┘                  │
└──────────────────────────────────────────────────────────────────┘
```

### Tabs Explained

| Tab | What's in it | Source |
|-----|-------------|--------|
| **Active** | Everything currently enabled/connected — skills, integrations, MCP servers, extensions. The "home screen." | Union of all sources, filtered to enabled=true |
| **Integrations** | TOML skills with credentials (email, Slack, GitHub, etc.) | `builtins.rs` integrations + `~/.paw/skills/` TOML |
| **Tools** | MCP servers + prompt-only skills | MCP server list + `.md` skills |
| **Extensions** | Skills with `[view]` or `[widget]` sections | TOML skills with view/widget |
| **Create** | Skill creation wizard (already exists, just moved to a tab) | Current wizard.ts |

### Card Design (per item)

Each card should show at a glance:

```
┌──────────────────────────────┐
│ [icon]  Name                 │
│ Type badge    Status dot     │
│                              │
│ One-line description         │
│                              │
│ [tools: 3] [widget] [mcp]   │  ← capability badges
│                              │
│ [Primary Action]  [···]      │  ← Configure / Set up / Enable / Connect
└──────────────────────────────┘
```

**Status states:**
- 🟢 Active/Configured/Connected
- 🟡 Needs setup (missing credentials or binary)
- ⚪ Disabled (installed but turned off)
- 🔴 Error (MCP disconnected, binary missing)

**Type badges (colored):**
- 🔵 Skill (prompt-only, blue)
- 🟣 Integration (credentials + tools, purple)
- 🔴 MCP Server (external tools, red)
- 🟡 Extension (custom view, gold)

---

## PawzHub — The Marketplace View

**Kill the iframe.** The PawzHub sidebar view should be a **native in-app marketplace**, not a browser embed. It pulls from the same `registry.json` that already exists but presents it as a store experience.

### Layout

```
┌──────────────────────────────────────────────────────────────────┐
│  PawzHub                                          [Refresh]     │
│  Discover skills, integrations, and extensions for your agent   │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────────┐│
│  │ 🔍 Search skills...                                         ││
│  └──────────────────────────────────────────────────────────────┘│
│                                                                  │
│  [All] [Integrations] [Tools] [Smart Home] [Media] [Dev] [···]  │
│                                                                  │
│  ── Featured ────────────────────────────────────────────────── │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐           │
│  │ n8n      │ │ Discord  │ │ Hue      │ │ Spotify  │           │
│  │ 🟣 Integ │ │ 🟣 Integ │ │ 🟣 Smart │ │ 🟣 Media │           │
│  │ [Install]│ │ [Active] │ │ [Install]│ │ [Set up] │           │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘           │
│                                                                  │
│  ── All Skills (41) ─────────────────────────────────────────── │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐           │
│  │ ...      │ │ ...      │ │ ...      │ │ ...      │           │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘           │
└──────────────────────────────────────────────────────────────────┘
```

### Install Flow

```
Browse PawzHub → Click "Install" → Skill appears in My Skills →
  → If needs credentials: opens Configure modal
  → If prompt-only: immediately active
  → If MCP: starts server, shows connected status
```

### Card States in PawzHub

- **Not installed**: "Install" button
- **Installed, needs setup**: "Set up" button (yellow dot)
- **Installed & active**: "Active" badge (green dot), no action needed
- **Update available**: "Update" button (version comparison)

---

## What Moves Where

| Current Location | Thing | New Location |
|-----------------|-------|-------------|
| Skills page → built-in cards | 400+ built-in integrations | **My Skills → Active/Integrations tab** (only if enabled) |
| Skills page → PawzHub section | PawzHub browser | **PawzHub sidebar view** (native, not iframe) |
| Skills page → Community section | Community skills browser | **PawzHub sidebar view** (merged into one store) |
| Skills page → Wizard | Skill creation wizard | **My Skills → Create tab** |
| Skills page → Extensions | TOML extension viewer | **My Skills → Extensions tab** |
| Settings → MCP tab | MCP server management | **My Skills → Tools tab** (first-class) |
| PawzHub sidebar (iframe) | Docs website iframe | **Killed.** Replace with native marketplace |
| Sidebar → "Skills" nav | Nav item | **Sidebar → "My Skills"** under Workspace section |

---

## Implementation Phases

### Phase 1 — My Skills Tabs (restructure existing code)
**Goal:** Split the monolithic skills page into clear tabs.

- [ ] Rename sidebar nav "Skills" → "My Skills"
- [ ] Rename sidebar section "System" → "Workspace" (for Skills, PawzHub, Foundry)
- [ ] Add tab bar to skills-view: Active / Integrations / Tools / Extensions / Create
- [ ] **Active tab**: Filter existing skill cards to only show enabled items
- [ ] **Integrations tab**: Show TOML/built-in skills with credentials
- [ ] **Tools tab**: Move MCP servers from Settings into this tab, add prompt-only skills
- [ ] **Extensions tab**: Move extension viewer here
- [ ] **Create tab**: Move wizard here
- [ ] Add summary bar at top with counts

**Files to change:**
- `index.html` — nav item text, tab bar HTML
- `src/views/settings-skills/index.ts` — tab switching logic
- `src/views/settings-skills/molecules.ts` — card rendering, filtering
- `src/views/settings-mcp/` — extract components to reuse in Skills view
- `src/views/router.ts` — update if needed

### Phase 2 — Native PawzHub Marketplace (replace iframe)
**Goal:** PawzHub becomes a real in-app store, not an iframe.

- [ ] Replace pawzhub-view iframe with native HTML/TS marketplace
- [ ] Create `src/views/pawzhub/` directory (index.ts, molecules.ts, atoms.ts)
- [ ] Move PawzHub browsing code from `community.ts` into pawzhub view
- [ ] Remove community section from skills page
- [ ] Add Featured section (curated list from registry.json)
- [ ] Add install status awareness (show "Active" vs "Install" vs "Set up")
- [ ] One-click install → item appears in My Skills immediately
- [ ] Remove iframe, CSP frame-src rules, iframe-related button handlers

**Files to change:**
- `index.html` — replace pawzhub-view div content
- Create `src/views/pawzhub/index.ts`
- Create `src/views/pawzhub/molecules.ts`
- Create `src/views/pawzhub/atoms.ts`
- `src/views/settings-skills/community.ts` — extract PawzHub code, remove inline section
- `src/main.ts` — remove iframe button handlers
- `src-tauri/tauri.conf.json` — remove frame-src CSP

### Phase 3 — Card Redesign & Status System
**Goal:** Every item looks consistent and shows status clearly.

- [x] Unified card component used by both My Skills and PawzHub
- [x] Status dot system (green/yellow/grey/red)
- [x] Type badges (Skill/Integration/MCP/Extension with colors)
- [x] Capability badges (tools count, widget, mcp)
- [x] Expand/collapse for credentials + details
- [x] Primary action button adapts to state (Configure / Set up / Enable / Disable)

**Files changed:**
- Created `src/components/molecules/skill-card.ts` — shared card component with adapters
- `src/views/settings-skills/tab-active.ts` — refactored to use shared card
- `src/views/settings-skills/tab-tools.ts` — refactored to use shared card
- `src/views/settings-skills/tab-extensions.ts` — refactored to use shared card
- `src/views/pawzhub/molecules.ts` — refactored to use shared card
- `src/views/pawzhub/index.ts` — unified install button wiring
- `src/styles.css` — added `uc-*` unified card CSS classes

### Phase 4 — Modular Enable/Disable
**Goal:** Skills are opt-in, not all-on-by-default.

- [x] New installs start with a "setup wizard" that asks which categories the user cares about
- [x] Built-in skills ship "available" but not "enabled" by default (except essentials like weather)
- [x] My Skills → Active tab only shows what the user chose
- [x] PawzHub shows "Install" even for built-in skills that aren't enabled
- [x] Persist enabled state in DB (already exists in vault, extend to non-credential skills)

**Backend changes:**
- `src-tauri/src/engine/skills/types.rs` — added `default_enabled: bool` to SkillDefinition + SkillStatus
- `src-tauri/src/engine/skills/builtins.rs` — added `default_enabled` to all 40 definitions (weather/blogwatcher = true)
- `src-tauri/src/engine/skills/vault.rs` — added `get_skill_enabled_state()` (Option<bool>), `bulk_set_skills_enabled()`, `is_onboarding_complete()`, `set_onboarding_complete()`
- `src-tauri/src/engine/skills/status.rs` — uses `get_skill_enabled_state().unwrap_or(default_enabled)` fallback
- `src-tauri/src/engine/skills/prompt.rs` — uses same default_enabled fallback for prompt injection
- `src-tauri/src/engine/agent_loop/helpers.rs` — updated tool loading to use default_enabled
- `src-tauri/src/engine/swarm.rs` — updated tool loading to use default_enabled
- `src-tauri/src/engine/tools/mod.rs` — `get_skill_creds()` uses default_enabled fallback
- `src-tauri/src/engine/tools/agents.rs` — self-info uses default_enabled fallback
- `src-tauri/src/commands/skills.rs` — added `engine_skill_bulk_enable`, `engine_is_onboarding_complete`, `engine_set_onboarding_complete`
- `src-tauri/src/lib.rs` — registered new commands

**Frontend changes:**
- `src/engine/atoms/types.ts` — added `default_enabled?: boolean` to EngineSkillStatus
- `src/engine/molecules/ipc_client.ts` — added `skillBulkEnable()`, `isOnboardingComplete()`, `setOnboardingComplete()`
- Created `src/views/settings-skills/setup-wizard.ts` — category picker wizard (7 categories, skip option)
- `src/views/settings-skills/index.ts` — checks onboarding state, shows wizard on first launch
- `src/views/pawzhub/molecules.ts` — added `renderBuiltinSkillsSection()`, `wireBuiltinEnableButtons()` for disabled built-ins
- `src/views/pawzhub/index.ts` — fetches skill list, renders disabled built-ins as enableable
- `src/styles.css` — added `sw-*` setup wizard CSS (overlay, dialog, category cards, animations)

---

## Priority Order

1. **Phase 1** — biggest UX win, mostly frontend reshuffling
2. **Phase 2** — makes PawzHub actually useful as a discovery tool
3. **Phase 3** — visual polish, consistency
4. **Phase 4** — requires backend changes, can ship after the UI feels right

---

## Open Questions

- [ ] Should MCP servers also appear in PawzHub? (community MCP registry?)
- [ ] Do we want a "Recently used" or "Suggested" section on My Skills?
- [ ] Should the Today dashboard widgets auto-add when a skill with `[widget]` is enabled?
- [ ] Should we show an onboarding flow on first launch that walks through picking skills?
- [ ] Do we keep the Community open-source browser (skills.sh / GitHub) or merge everything into PawzHub?
