# Views Refactor Plan

> **Goal**: Decompose 14 monolithic view files (14,605 LOC) into the atomic design pattern already established in `src/features/` and `src/engine/`.  
> **Pattern**: `atoms/` (pure functions, constants, types) → `molecules/` (DOM + IPC compositions) → `index.ts` (barrel exports + event wiring)  
> **Rule**: No single file exceeds ~300 lines. No duplicated utilities.

## ✅ Refactor Complete

All phases executed and committed. Final verification passed:
- `npx tsc --noEmit` — zero errors
- `grep` for local `escHtml`/`escapeHtml`/`escAttr`/`$` duplicates — zero matches
- All view imports resolve correctly (TypeScript auto-resolves directory imports)

| Phase | Target | Status | Commit |
|-------|--------|--------|--------|
| 0 | Deduplicate shared utilities | ✅ | `be54dac` |
| 0D+1 | Connection + line budget fixes | ✅ | `f3cf02b` |
| 1 | agents.ts (1,586 → 5 files) | ✅ | `f4562d0` |
| 2 | channels.ts (1,166 → 4 files) | ✅ | `5ea5fc2` |
| 3 | mail.ts (985 → 4 files + IPC) | ✅ | `a7b62a3` |
| 4 | projects.ts (941 → 4 files) | ✅ | `0f9de39` |
| 5 | memory-palace.ts (839 → 4 files + IPC) | ✅ | `05441c7` |
| 6 | settings-skills.ts (792 → 4 files) | ✅ | `f05021a` |
| 7 | research.ts (677 → 3 files) | ✅ | `df883f5` |
| 8 | settings.ts (626 → 3 files) | ✅ | `c29966a` |
| 9 | tasks.ts (560 → 3 files) | ✅ | `567e775` |
| 10 | 4 remaining (browser/today/orch/voice) | ✅ | `ad01682` |
| 11 | Final import cleanup & verification | ✅ | — |

### Known Remaining Items

- `settings-models.ts` (860 lines) — intentionally left flat; best-structured of the large files, uses shared `settings-config` module. Can be decomposed in a future pass if needed.
- Several `molecules.ts` files exceed ~300 lines (largest: `settings-browser/molecules.ts` at 545). These contain cohesive DOM rendering logic that would fragment if split further.
- `automations.ts` (418) and `trading.ts` (434) are flat borderline files — acceptable as-is.

---

## Original State (Pre-Refactor)

### File Sizes (sorted by severity)

### File Sizes (sorted by severity)

| # | File | Lines | Violations |
|---|------|------:|------------|
| 1 | `agents.ts` | 1,586 | 5× over limit, local `escHtml`/`escAttr`/`showToast`/`$` |
| 2 | `channels.ts` | 1,166 | 4× over limit, 240-line config constant |
| 3 | `mail.ts` | 985 | 3× over, raw `invoke()` not `pawEngine`, local `escHtml`/`showToast`/`$`/`formatMarkdown` |
| 4 | `projects.ts` | 956 | 3× over, local `escapeHtml`/`escapeAttr`/`$`, security predicates |
| 5 | `memory-palace.ts` | 864 | 3× over, raw `invoke()`, local `escHtml`/`showToast`/`$` |
| 6 | `settings-models.ts` | 859 | 3× over (but best-structured — uses shared `settings-config`) |
| 7 | `settings-skills.ts` | 807 | 3× over, local `escHtml`/`showVaultToast`/`$` |
| 8 | `research.ts` | 710 | 2× over, local `escHtml`/`showToast`/`$`/`formatMarkdown` |
| 9 | `settings.ts` | 647 | 2× over, local `escHtml`/`showSettingsToast`/`$` |
| 10 | `tasks.ts` | 582 | 2× over, local `escHtml`/`$` |
| 11 | `settings-browser.ts` | 563 | ~2× over, local `escHtml`/`$` |
| 12 | `today.ts` | 523 | ~2× over, raw `invoke()`, own `showToast` impl, local `escHtml`/`$` |
| 13 | `orchestrator.ts` | 509 | ~2× over, local `escapeHtml`/`$`, own `timeAgo` |
| 14 | `settings-voice.ts` | 505 | ~2× over, local `$` |

### Duplication Epidemic

| Utility | Duplicated In | Already Exists In |
|---------|--------------|-------------------|
| `$()` getElementById | **25 files** | `components/helpers.ts` (exported, barely imported) |
| `escHtml()` / `escapeHtml()` | **15 files** | `components/helpers.ts` (exported, barely imported) |
| `escAttr()` / `escapeAttr()` | **6 files** | `components/helpers.ts` (exported, barely imported) |
| `showToast()` variants | **7 files** (3 different signatures) | `components/toast.ts` (exported, used by ~5 files) |
| `formatMarkdown()` | **2 files** | `components/helpers.ts` (exported, not imported by views) |
| `formatTimeAgo()` / `timeAgo()` | **3 files** | Not centralized |
| `wsConnected` + `setWsConnected()` | **4 files** | Not centralized |

**Root cause**: `components/helpers.ts` exports `$`, `escHtml`, `escAttr`, `formatMarkdown`, `formatBytes` — but only 3 of 29 view files import from it. Every other file re-declares these locally.

---

## Phased Execution Plan

### Phase 0: Deduplicate Shared Utilities (prerequisite — must complete first)

> Eliminate all duplicated utility functions by making every view import from shared modules.  
> **Zero logic changes. Import swap only.**

#### Phase 0A: Consolidate `$`, `escHtml`, `escAttr`, `formatMarkdown`, `formatBytes`

**Target**: `src/components/helpers.ts` (already exports all of these)

For each of the following files, **delete** the local function definition and **add an import** from `../components/helpers`:

| File | Delete Local | Add Import |
|------|-------------|------------|
| `agents.ts` | `$` (L15), `escHtml` (L1126), `escAttr` (L1130) | `{ $, escHtml, escAttr }` |
| `automations.ts` | `$` (L8), `escHtml` (L21), `escAttr` (L26) | `{ $, escHtml, escAttr }` |
| `channels.ts` | `$` (L9) | `{ $ }` (already imports `escHtml`/`escAttr` from molecules/markdown — switch to helpers) |
| `content.ts` | `$` (L8) | `{ $ }` |
| `foundry.ts` | `$` (L9), `escHtml` (L22) | `{ $, escHtml }` |
| `mail.ts` | `$` (L7), `escHtml` (L56), `escAttr` (L61), `formatMarkdown` (local) | `{ $, escHtml, escAttr, formatMarkdown }` |
| `memory-palace.ts` | `$` (L6), `escHtml` (L41) | `{ $, escHtml }` |
| `nodes.ts` | `$` (L8) | `{ $ }` |
| `projects.ts` | `$` (L7), `escapeHtml` (L942), `escapeAttr` (L950) | `{ $ , escHtml as escapeHtml, escAttr as escapeAttr }` — OR rename call sites |
| `research.ts` | `$` (L8), `escHtml` (L86), `formatMarkdown` (local) | `{ $, escHtml, formatMarkdown }` |
| `settings.ts` | `$` (L8), `escHtml` (L17) | `{ $, escHtml }` |
| `settings-advanced.ts` | `$` (L11) | `{ $ }` |
| `settings-agent-defaults.ts` | `$` (L12) | `{ $ }` |
| `settings-browser.ts` | `$` (L8), `escHtml` (L561) | `{ $, escHtml }` |
| `settings-engine.ts` | `$` (L8), `escHtml` (L10) | `{ $, escHtml }` |
| `settings-env.ts` | `$` (L12) | `{ $ }` |
| `settings-models.ts` | `$` (L12) | `{ $ }` |
| `settings-sessions.ts` | `$` (L12) | `{ $ }` |
| `settings-skills.ts` | `$` (L7), `escHtml` (L9) | `{ $, escHtml }` |
| `settings-tabs.ts` | `$` (L12) | `{ $ }` |
| `settings-tailscale.ts` | `$` (L8) | `{ $ }` |
| `settings-voice.ts` | `$` (L6) | `{ $ }` |
| `tasks.ts` | `$` (L11), `escHtml` (L13) | `{ $, escHtml }` |
| `today.ts` | `$` (L7), `escHtml` (L517) | `{ $, escHtml }` |
| `trading.ts` | `$` (L6), `escHtml` (L22) | `{ $, escHtml }` |
| `orchestrator.ts` | `escapeHtml` (L459) | `{ escHtml }` — rename call sites from `escapeHtml()` to `escHtml()` |

**Special case — `projects.ts`**: Uses `escapeHtml`/`escapeAttr` (different name). Either:
- (a) Rename all call sites to `escHtml`/`escAttr`, OR
- (b) Import as `{ escHtml as escapeHtml, escAttr as escapeAttr }`
- Prefer (a) for consistency.

**Special case — `channels.ts`**: Currently imports from `../components/molecules/markdown`. That module should re-export from `helpers.ts` or channels should switch to importing from `helpers.ts` directly.

**Verification**: After each file, run `npx tsc --noEmit` to confirm no type errors.

#### Phase 0B: Consolidate `showToast`

**Target**: `src/components/toast.ts` (already exports a working `showToast`)

Delete local toast implementations and import from `../components/toast`:

| File | Delete Local | Notes |
|------|-------------|-------|
| `agents.ts` | `showToast` (L1114-1124) | Different signature — uses `'success' \| 'error'` default `'success'`. Convert call sites to pass type explicitly |
| `mail.ts` | `showToast` (L65-75) | Same signature as shared — direct swap |
| `memory-palace.ts` | `showToast` (L46-56) | Same signature as shared — direct swap |
| `research.ts` | `showToast` (L99-109) | 3-type variant — matches shared |
| `settings.ts` | `showSettingsToast` (L578-590) | Rename all call sites from `showSettingsToast()` to `showToast()` |
| `settings-skills.ts` | `showVaultToast` (L14-27) | Rename all call sites from `showVaultToast()` to `showToast()` |
| `today.ts` | `showToast` (L505-516) | **Custom DOM impl** — different toast element ID. Must either switch to `global-toast` or add param to shared `showToast` |

**Special case — `today.ts`**: Its `showToast` creates/uses a different DOM element than `global-toast`. Options:
- (a) Make `today.ts` use the global toast element → simplest
- (b) Add optional `elementId` param to shared `showToast` → more flexible
- Prefer (a).

#### Phase 0C: Add `formatTimeAgo` to shared helpers

**Add to `src/components/helpers.ts`**:
```typescript
export function formatTimeAgo(date: string | Date): string {
  const seconds = Math.floor((Date.now() - new Date(date).getTime()) / 1000);
  if (seconds < 60) return 'just now';
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
  if (seconds < 2592000) return `${Math.floor(seconds / 86400)}d ago`;
  return new Date(date).toLocaleDateString();
}
```

Then replace local versions in:
- `tasks.ts` — `formatTimeAgo()` 
- `orchestrator.ts` — `timeAgo()`
- `settings-browser.ts` — already imports from features (redirect to helpers)

#### Phase 0D: Centralize `wsConnected` state

Create `src/state/connection.ts`:
```typescript
let _wsConnected = false;
export function isConnected(): boolean { return _wsConnected; }
export function setConnected(v: boolean) { _wsConnected = v; }
```

Replace local `wsConnected` + `setWsConnected()` in:
- `mail.ts`
- `memory-palace.ts`
- `research.ts`
- `settings.ts`

**Phase 0 completion check**: `grep -rn 'function escHtml\|function escapeHtml\|function escAttr\|function escapeAttr\|function showToast\|function showVaultToast\|function showSettingsToast' src/views/` returns **zero results**.

---

### Phase 1: Decompose `agents.ts` (1,586 → ~5 files)

The largest file. Contains agent CRUD, the agent editor modal (with personality/skills/advanced tabs), mini-chat system, agent dock, and avatar generation.

#### Target Structure
```
src/views/agents/
├── atoms.ts          # ~200 lines — types, constants, pure helpers
├── molecules.ts      # ~250 lines — agent cards, dock rendering
├── editor.ts         # ~300 lines — agent create/edit modal
├── mini-chat.ts      # ~280 lines — floating mini-chat widget
└── index.ts          # ~150 lines — state, loadAgents, configure, initAgents, exports
```

#### Atom Extraction (`atoms.ts`)
Move these pure functions and constants:
- `Agent` interface
- `AGENT_TEMPLATES` constant (~80 lines)
- `TOOL_GROUPS` constant
- `AVATAR_COLORS` / `SPRITE_AVATARS` constants
- `spriteAvatar()` — pure SVG builder
- `isAvatar()` — pure predicate
- `seedSoulFiles()` — agent personality file generator

#### Molecule Extraction (`molecules.ts`)
- `renderAgents()` — card grid rendering
- `renderAgentDock()` — floating dock bar
- `onProfileUpdated()` — event handler
- Agent delete confirmation

#### Editor Extraction (`editor.ts`)
- `openAgentCreator()` — creation modal with tabs
- `openAgentEditor()` — edit modal with personality/skills/advanced tabs
- Template selection flow
- Agent file management (soul files, custom instructions)

#### Mini-Chat Extraction (`mini-chat.ts`)
- `openMiniChat()` — floating chat widget
- Mini-chat message rendering
- Mini-chat send/receive/stream handling
- `_miniChats: Map` state

#### Index (`index.ts`)
- Module state: `_agents`, `_selectedAgent`, `_availableModels`, callbacks
- `configure()` — callback setup
- `loadAgents()` — data fetch + delegate to molecules
- `getAgents()` / `getCurrentAgent()` / `setSelectedAgent()` — state accessors
- `initAgents()` — event wiring, delegates to sub-modules
- Re-exports all public API

---

### Phase 2: Decompose `channels.ts` (1,166 → ~4 files)

Contains channel config definitions, setup modal, channel cards, and per-channel save/start/stop logic.

#### Target Structure
```
src/views/channels/
├── atoms.ts          # ~260 lines — CHANNEL_SETUPS config, types, predicates
├── molecules.ts      # ~250 lines — channel cards, status badges, user approval
├── setup.ts          # ~300 lines — setup modal, per-channel save logic
└── index.ts          # ~100 lines — loadChannels, initChannels, exports
```

#### Atom Extraction (`atoms.ts`)
- `CHANNEL_SETUPS` config array (~240 lines) — static definition of all 10 channel types
- `CHANNEL_CLASSES` mapping
- Channel type/field interfaces
- `isChannelConfigured()` predicate
- `emptyChannelConfig()` factory

#### Molecule Extraction (`molecules.ts`)
- Channel card grid rendering
- Pending user approval list
- Channel status badge rendering
- Auto-start logic

#### Setup Extraction (`setup.ts`)
- `openChannelSetup()` — modal with dynamic form fields
- `saveChannelSetup()` — per-channel branching save (largest function)
- `closeChannelSetup()`
- Form validation

---

### Phase 3: Decompose `mail.ts` (985 → ~4 files)

Email client view. Also needs IPC migration from raw `invoke()` to `pawEngine`.

#### Target Structure
```
src/views/mail/
├── atoms.ts          # ~120 lines — types, EMAIL_PROVIDERS, date formatters, content extraction
├── molecules.ts      # ~300 lines — account list, email list, email detail, compose modal
├── setup.ts          # ~200 lines — account setup modal, IMAP/SMTP configuration
└── index.ts          # ~100 lines — state, loadMail, initMailEvents, exports
```

#### Atom Extraction (`atoms.ts`)
- `MailPermissions` interface
- `EMAIL_PROVIDERS` config
- `formatMailDate()` — date formatter
- `getAvatarClass()` / `getInitials()` — string helpers
- `_extractContent()` — HTML extraction
- `loadMailPermissions()` / `saveMailPermissions()` — localStorage

#### IPC Migration
All raw `invoke()` calls → `pawEngine.*` equivalents. If `pawEngine` doesn't have them yet, add them to `engine-bridge.ts`.

---

### Phase 4: Decompose `projects.ts` (956 → ~4 files)

File browser with git integration. Heavy Tauri plugin usage.

#### Target Structure
```
src/views/projects/
├── atoms.ts          # ~150 lines — types, security predicates, file icons, path helpers
├── molecules.ts      # ~300 lines — project sidebar, file tree, file viewer, git banner
├── git.ts            # ~150 lines — git status, git actions (pull, push, commit)
└── index.ts          # ~120 lines — state, loadProjects, bindEvents, exports
```

#### Atom Extraction (`atoms.ts`)
- Interfaces: `GitInfo`, `FileEntry`, `ProjectFolder`
- `SENSITIVE_PATH_PATTERNS` + `isSensitivePath()` — security
- `isOutOfProjectScope()` — scope guard
- `getFileIcon()` / `getLanguageClass()` — file type mapping
- `shortenPath()` / `shortenRemote()` — path formatters
- `getDepth()` / `getProjectRoot()` — path logic
- `loadSavedProjects()` / `savePersistProjects()` — localStorage

---

### Phase 5: Decompose `memory-palace.ts` (864 → ~4 files)

Memory visualization with canvas graph, search, and store/backfill.

#### Target Structure
```
src/views/memory-palace/
├── atoms.ts          # ~80 lines — types, state helpers
├── molecules.ts      # ~250 lines — stats, sidebar, recall card, remember form
├── graph.ts          # ~250 lines — canvas knowledge graph visualization
└── index.ts          # ~100 lines — state, loadMemoryPalace, initPalaceEvents, exports
```

#### IPC Migration
Raw `invoke()` calls → `pawEngine.*` equivalents (embedding config, memory plugin).

---

### Phase 6: Decompose `settings-skills.ts` (807 → ~4 files)

Skill vault management with built-in + community skills.

#### Target Structure
```
src/views/settings-skills/
├── atoms.ts          # ~100 lines — CATEGORY_META, SKILL_ICON_MAP, POPULAR_REPOS/TAGS, formatInstalls
├── molecules.ts      # ~250 lines — skill card, credential fields, binary status, filter
├── community.ts      # ~200 lines — community section, search, browse repo, install
└── index.ts          # ~100 lines — loadSkillsSettings, bindEvents, exports
```

---

### Phase 7: Decompose `research.ts` (710 → ~3 files)

Research workflow with live streaming progress.

#### Target Structure
```
src/views/research/
├── atoms.ts          # ~80 lines — types, extractDomain, parseProgressStep, stream protocol
├── molecules.ts      # ~300 lines — project list, findings, sources, live feed, progress
└── index.ts          # ~120 lines — state, loadProjects, initResearchEvents, exports
```

---

### Phase 8: Decompose `settings.ts` (647 → ~3 files)

Settings hub — status, usage, security audit, policies.

#### Target Structure
```
src/views/settings-main/
├── atoms.ts          # ~60 lines — types, budget helpers, ToolRule interface
├── molecules.ts      # ~300 lines — status, usage, audit log, security policies, approvals
└── index.ts          # ~100 lines — state, loadSettings, initSettings, exports
```

---

### Phase 9: Decompose `tasks.ts` (582 → ~3 files)

Kanban board with drag-and-drop.

#### Target Structure
```
src/views/tasks/
├── atoms.ts          # ~60 lines — COLUMNS, types
├── molecules.ts      # ~280 lines — board, cards, feed, stats, task modal, agent picker, drag-and-drop
└── index.ts          # ~100 lines — state, loadTasks, bindTaskEvents, cron timer, exports
```

---

### Phase 10: Decompose remaining files (4 files, ~2,100 lines total)

These are close to the limit but still violate it. Apply the same pattern:

#### `settings-browser.ts` (563 → ~3 files)
```
src/views/settings-browser/
├── atoms.ts          # ~40 lines — types
├── molecules.ts      # ~280 lines — profiles, options, screenshots, workspaces, network policy
└── index.ts          # ~80 lines — loadBrowserSettings, exports
```

#### `today.ts` (523 → ~3 files)
```
src/views/today/
├── atoms.ts          # ~60 lines — getWeatherIcon, getGreeting, getPawzMessage, Task type
├── molecules.ts      # ~250 lines — weather, emails, tasks, quick actions, dashboard grid
└── index.ts          # ~80 lines — state, loadToday, initToday, exports
```

**IPC Migration**: Raw `invoke()` → `pawEngine.*`.

#### `orchestrator.ts` (509 → ~3 files)
```
src/views/orchestrator/
├── atoms.ts          # ~60 lines — specialtyIcon, messageKindLabel, types
├── molecules.ts      # ~250 lines — project list, detail, agent roster, message bus, modals
└── index.ts          # ~80 lines — state, loadProjects, initOrchestrator, exports
```

#### `settings-voice.ts` (505 → ~3 files)
```
src/views/settings-voice/
├── atoms.ts          # ~100 lines — voice catalogs (GOOGLE/OPENAI/ELEVENLABS_VOICES), LANGUAGES
├── molecules.ts      # ~250 lines — settings form, sliders, TTS test, talk mode
└── index.ts          # ~60 lines — loadVoiceSettings, initVoiceSettings, exports
```

---

### Phase 11: Update `main.ts` imports

After all phases, update `main.ts` to import from the new `views/{name}/index.ts` barrel files instead of the old flat files. Since TypeScript resolves `./views/agents` → `./views/agents/index.ts` automatically, most imports won't need path changes — only if `main.ts` uses file-specific imports.

Verify: Run `npx tsc --noEmit` to confirm all imports resolve.

---

## Files That Stay Flat

These files are already within the ~300 line budget and don't need decomposition:

| File | Lines | Status |
|------|------:|--------|
| `settings-advanced.ts` | 390 | Borderline — acceptable |
| `settings-agent-defaults.ts` | 339 | ✓ OK |
| `automations.ts` | 432 | Borderline — acceptable after Phase 0 dedup |
| `trading.ts` | 443 | Borderline — acceptable after Phase 0 dedup |
| `settings-config.ts` | 238 | ✓ OK |
| `settings-engine.ts` | 252 | ✓ OK |
| `foundry.ts` | 231 | ✓ OK |
| `nodes.ts` | 190 | ✓ OK |
| `settings-tailscale.ts` | 188 | ✓ OK |
| `settings-sessions.ts` | 186 | ✓ OK |
| `settings-env.ts` | 145 | ✓ OK |
| `content.ts` | 129 | ✓ OK |
| `router.ts` | 112 | ✓ OK |
| `settings-tabs.ts` | 47 | ✓ OK |
| `skills.ts` | 21 | ✓ OK (stub) |

---

## Execution Rules

1. **Phase 0 is mandatory before any other phase.** Deduplication must complete first — otherwise every subsequent phase would need to handle both local and shared utils.

2. **One phase = one commit.** Each phase must leave the app in a compiling, working state.

3. **Verification per phase:**
   - `npx tsc --noEmit` — zero type errors
   - `grep` for leftover local duplicates — zero matches
   - Visual spot-check of the affected view in the app

4. **No logic changes during decomposition.** Move code, update imports, nothing else. Bug fixes or improvements are separate commits.

5. **Atoms are pure.** No `document.*`, no `invoke()`, no `import { invoke }`, no `listen()`, no side effects. If it touches the DOM or calls IPC, it's a molecule.

6. **Index files are thin.** Module state + init wiring + re-exports. No rendering, no heavy logic.

7. **Import path convention:** `import { escHtml, $ } from '../../components/helpers'` for shared utils. `import { renderAgentCards } from './molecules'` for intra-module.

8. **Naming:** Directories use kebab-case matching the old filename: `agents.ts` → `agents/`, `memory-palace.ts` → `memory-palace/`, `settings-skills.ts` → `settings-skills/`.

---

## Priority Order

| Priority | Phase | Impact | Est. Files Changed | Status |
|----------|-------|--------|-------------------|--------|
| **P0** | Phase 0 (dedup) | Blocks everything, eliminates ~400 lines of duplication | 25 files | ✅ |
| **P1** | Phase 1 (agents) | Largest file, most complex | 2→5 files | ✅ |
| **P1** | Phase 2 (channels) | Second largest, config-heavy | 2→4 files | ✅ |
| **P2** | Phase 3 (mail) | IPC migration needed | 2→4 files | ✅ |
| **P2** | Phase 4 (projects) | Security-critical code | 2→4 files | ✅ |
| **P2** | Phase 5 (memory-palace) | Canvas graph is complex | 2→4 files | ✅ |
| **P3** | Phase 6 (settings-skills) | Community skills browser | 2→4 files | ✅ |
| **P3** | Phase 7 (research) | Streaming complexity | 2→3 files | ✅ |
| **P3** | Phase 8 (settings) | Security audit | 2→3 files | ✅ |
| **P3** | Phase 9 (tasks) | Drag-and-drop | 2→3 files | ✅ |
| **P4** | Phase 10 (4 remaining) | Lower priority | 4→12 files | ✅ |
| **P4** | Phase 11 (main.ts) | Final import cleanup | 1 file | ✅ |

**Result**: 13 monolithic files → ~46 focused files (atoms + molecules + index per view), with all shared utilities imported from `components/helpers.ts` and `components/toast.ts`. `settings-models.ts` intentionally remains flat.
