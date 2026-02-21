# Views Refactor — Sonnet Execution Prompts

> Copy-paste each phase prompt to Sonnet. Wait for commit before moving to next phase.  
> Authoritative plan: `VIEWS_REFACTOR.md`

---

## Phase 0: Deduplicate Shared Utilities

```
You are refactoring the src/views/ directory in /workspaces/paw/. Read VIEWS_REFACTOR.md at the project root — it is the authoritative plan. You are executing Phase 0 only (the prerequisite deduplication phase). Do NOT proceed to Phase 1+ yet.

Context: 25 view files in src/views/ redeclare utility functions that already exist in shared modules. Your job is to delete every local duplicate and replace it with an import from the shared module. Zero logic changes — import swaps only.

Shared modules that already export these utilities:
- src/components/helpers.ts exports: $, escHtml, escAttr, formatMarkdown, formatBytes, icon, populateModelSelect, promptModal, providerIcon. escHtml is the canonical name (some files use escapeHtml — rename those call sites)
- src/components/toast.ts exports: showToast(message, type, durationMs) with type 'info' | 'success' | 'error' | 'warning' defaulting to 'info'

Phase 0A — For every file in src/views/, do this:
1. If the file has const $ = (id: string) => document.getElementById(id); — delete that line and add $ to the import from ../components/helpers
2. If the file has a local function escHtml( — delete the function and add escHtml to the import from ../components/helpers
3. If the file has a local function escAttr( — delete the function and add escAttr to the import from ../components/helpers
4. If the file uses escapeHtml( or escapeAttr( (different name) — delete the local function, import escHtml/escAttr from helpers, and rename all call sites from escapeHtml( → escHtml( and escapeAttr( → escAttr(
5. If the file has a local function formatMarkdown( — delete it and import from ../components/helpers
6. Special: channels.ts already imports escHtml/escAttr from ../components/molecules/markdown — switch that import to ../components/helpers instead

Phase 0B — Toast consolidation:
For files with local showToast / showVaultToast / showSettingsToast / showSkillsToast:
1. Delete the local function
2. Add import { showToast } from '../components/toast'
3. Rename call sites if the local name was different (e.g., showVaultToast( → showToast(, showSettingsToast( → showToast()
4. If local had a 2-param signature (message, type) where type was 'success' | 'error' — call sites are compatible since shared version defaults to 'info'
5. Special: today.ts has a custom DOM-based toast — replace it with the shared import and ensure the page uses global-toast element

Files with local toast to fix: agents.ts, mail.ts, memory-palace.ts, research.ts, settings.ts (showSettingsToast), settings-skills.ts (showVaultToast), today.ts

Phase 0C — Add formatTimeAgo to src/components/helpers.ts:
Add this function to the end of helpers.ts:

export function formatTimeAgo(date: string | Date): string {
  const seconds = Math.floor((Date.now() - new Date(date).getTime()) / 1000);
  if (seconds < 60) return 'just now';
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
  if (seconds < 2592000) return `${Math.floor(seconds / 86400)}d ago`;
  return new Date(date).toLocaleDateString();
}

Then replace local formatTimeAgo() in tasks.ts, local timeAgo() in orchestrator.ts, and any similar local time formatters with imports from helpers.

Phase 0D — Centralize wsConnected state:
Create src/state/connection.ts:

let _wsConnected = false;
export function isConnected(): boolean { return _wsConnected; }
export function setConnected(v: boolean) { _wsConnected = v; }

Replace local wsConnected + setWsConnected() in: mail.ts, memory-palace.ts, research.ts, settings.ts. Update main.ts to call setConnected(true) from this module instead of per-view.

Rules:
- One file at a time. After editing each file, verify no type errors
- Do NOT change any logic, DOM structure, or behavior — only imports
- After all changes, run: grep -rn 'function escHtml\|function escapeHtml\|function escAttr\|function escapeAttr\|const \$ = .*getElementById' src/views/ — must return zero results
- Run: grep -rn 'function showToast\|function showVaultToast\|function showSettingsToast' src/views/ — must return zero results
- Final check: npx tsc --noEmit — zero errors
- Commit as: refactor: deduplicate shared utilities across all views (Phase 0)
```

---

## Phase 1: Decompose `agents.ts` (1,586 → 5 files)

```
You are refactoring src/views/ in /workspaces/paw/. Read VIEWS_REFACTOR.md — it is the authoritative plan. You are executing Phase 1 only. Phase 0 (dedup) is already committed.

Decompose src/views/agents.ts (1,586 lines) into the atomic design pattern. Create this directory structure:

src/views/agents/
├── atoms.ts          # ~200 lines — types, constants, pure helpers
├── molecules.ts      # ~250 lines — agent cards, dock rendering
├── editor.ts         # ~300 lines — agent create/edit modal
├── mini-chat.ts      # ~280 lines — floating mini-chat widget
└── index.ts          # ~150 lines — state, loadAgents, configure, initAgents, exports

atoms.ts — Move these pure functions and constants (zero DOM, zero IPC):
- Agent interface/type
- AGENT_TEMPLATES constant (~80 lines)
- TOOL_GROUPS constant
- AVATAR_COLORS / SPRITE_AVATARS constants
- spriteAvatar() — pure SVG builder
- isAvatar() — pure predicate
- seedSoulFiles() — agent personality file generator (pure data, no DOM)

molecules.ts — Move these DOM/render functions:
- renderAgents() — card grid rendering
- renderAgentDock() — floating dock bar
- onProfileUpdated() — event handler for real-time agent updates
- Agent delete confirmation dialog

editor.ts — Move the agent editor system:
- openAgentCreator() — creation modal with template tabs
- openAgentEditor() — edit modal with personality/skills/advanced tabs
- Template selection flow
- Agent file management (soul files, custom instructions)

mini-chat.ts — Move the mini-chat widget system:
- openMiniChat() — floating chat widget
- Mini-chat message rendering
- Mini-chat send/receive/stream handling
- _miniChats: Map state for tracking open chats

index.ts — Keep the orchestration layer:
- Module state: _agents, _selectedAgent, _availableModels, callbacks
- configure() — callback setup
- loadAgents() — data fetch + delegate to molecules
- getAgents() / getCurrentAgent() / setSelectedAgent() — state accessors
- initAgents() — event wiring, delegates to sub-modules
- Re-exports all public API that main.ts needs

Rules:
- All shared utils ($, escHtml, escAttr, showToast) come from ../components/helpers and ../components/toast — never local
- atoms.ts must have ZERO imports from Tauri, ZERO document.* calls
- Molecules import from ./atoms. Index imports from ./molecules and ./atoms.
- TypeScript resolves import './views/agents' → './views/agents/index.ts' automatically, so main.ts imports should not need path changes
- Delete the old agents.ts after creating the directory
- npx tsc --noEmit — zero errors
- Commit as: refactor: decompose agents.ts into atomic modules (Phase 1)
```

---

## Phase 2: Decompose `channels.ts` (1,166 → 4 files)

```
You are refactoring src/views/ in /workspaces/paw/. Read VIEWS_REFACTOR.md — it is the authoritative plan. You are executing Phase 2 only. Phases 0-1 are already committed.

Decompose src/views/channels.ts (1,166 lines) into atomic design pattern:

src/views/channels/
├── atoms.ts          # ~260 lines — CHANNEL_SETUPS config, types, predicates
├── molecules.ts      # ~250 lines — channel cards, status badges, user approval
├── setup.ts          # ~300 lines — setup modal, per-channel save logic
└── index.ts          # ~100 lines — loadChannels, initChannels, exports

atoms.ts — Move pure data (zero DOM, zero IPC):
- CHANNEL_SETUPS config array (~240 lines) — static definition of all 10 channel types with form fields
- CHANNEL_CLASSES mapping
- Channel type/field interfaces
- isChannelConfigured() predicate
- emptyChannelConfig() factory

molecules.ts — Move DOM rendering:
- Channel card grid rendering
- Pending user approval list rendering
- Channel status badge rendering
- Auto-start logic

setup.ts — Move the setup modal system:
- openChannelSetup() — modal with dynamic form fields populated from CHANNEL_SETUPS
- saveChannelSetup() — per-channel branching save logic (the largest function)
- closeChannelSetup()
- Form validation

index.ts — Keep orchestration:
- Module state: _channelSetupType
- loadChannels() — data fetch + delegate
- initChannels() — event wiring
- autoStartConfiguredChannels()
- loadMemory(), openMemoryFile(), loadDashboardCron(), loadSpaceCron()
- Re-exports

Rules:
- All shared utils from ../components/helpers and ../components/toast
- atoms.ts is pure — no DOM, no IPC
- Delete old channels.ts after creating directory
- npx tsc --noEmit — zero errors
- Commit as: refactor: decompose channels.ts into atomic modules (Phase 2)
```

---

## Phase 3: Decompose `mail.ts` (985 → 4 files)

```
You are refactoring src/views/ in /workspaces/paw/. Read VIEWS_REFACTOR.md — it is the authoritative plan. You are executing Phase 3 only. Phases 0-2 are already committed.

Decompose src/views/mail.ts (985 lines) into atomic design pattern. This file also needs IPC migration from raw invoke() to pawEngine.

src/views/mail/
├── atoms.ts          # ~120 lines — types, EMAIL_PROVIDERS, date formatters, content extraction
├── molecules.ts      # ~300 lines — account list, email list, email detail, compose modal
├── setup.ts          # ~200 lines — account setup modal, IMAP/SMTP configuration
└── index.ts          # ~100 lines — state, loadMail, initMailEvents, exports

atoms.ts — Pure logic:
- MailPermissions interface
- EMAIL_PROVIDERS config
- formatMailDate() — date formatter
- getAvatarClass() / getInitials() — string helpers
- _extractContent() — HTML content extraction
- loadMailPermissions() / saveMailPermissions() — localStorage helpers

molecules.ts — DOM + IPC:
- renderMailAccounts() — account list with status badges
- renderCredentialActivityLog() — table rendering
- renderMailList() — email list with folder tabs
- openMailMessage() — full email display
- openComposeModal() — compose form
- showMailEmpty() — empty state

setup.ts — Mail account configuration:
- openMailAccountSetup() — setup modal
- showMailAccountForm() — IMAP/SMTP form
- saveMailImapSetup() — save config via IPC

index.ts — Orchestration:
- Module state
- loadMail() — data fetch + delegate
- initMailEvents() — event wiring
- Re-exports

IPC Migration: If mail.ts uses raw invoke() calls instead of pawEngine, check if pawEngine has equivalent methods. If not, add thin wrappers to engine-bridge.ts. All IPC should go through pawEngine for consistency.

Rules:
- All shared utils from ../components/helpers and ../components/toast
- Use isConnected() from ../state/connection instead of local wsConnected
- atoms.ts is pure — no DOM, no IPC
- Delete old mail.ts after creating directory
- npx tsc --noEmit — zero errors
- Commit as: refactor: decompose mail.ts into atomic modules (Phase 3)
```

---

## Phase 4: Decompose `projects.ts` (956 → 4 files)

```
You are refactoring src/views/ in /workspaces/paw/. Read VIEWS_REFACTOR.md — it is the authoritative plan. You are executing Phase 4 only. Phases 0-3 are already committed.

Decompose src/views/projects.ts (956 lines) into atomic design pattern:

src/views/projects/
├── atoms.ts          # ~150 lines — types, security predicates, file icons, path helpers
├── molecules.ts      # ~300 lines — project sidebar, file tree, file viewer, git banner
├── git.ts            # ~150 lines — git status, git actions (pull, push, commit)
└── index.ts          # ~120 lines — state, loadProjects, bindEvents, exports

atoms.ts — Pure logic (security-critical — be careful):
- Interfaces: GitInfo, FileEntry, ProjectFolder
- SENSITIVE_PATH_PATTERNS + isSensitivePath() — security predicate
- isOutOfProjectScope() — scope guard
- getFileIcon() / getLanguageClass() — file type mapping
- shortenPath() / shortenRemote() — path formatters
- getDepth() / getProjectRoot() — path logic
- loadSavedProjects() / savePersistProjects() — localStorage

molecules.ts — DOM rendering:
- renderProjectsSidebar() — project list + git status
- selectProject() — folder selection + tree loading
- renderTreeEntries() — recursive file tree
- bindTreeEvents() — click/expand/collapse
- openFile() — file content display
- showProjectsEmpty() — empty state

git.ts — Git integration:
- renderGitBanner() — git status bar
- bindGitActions() — git button handlers (pull, push, commit, status)
- Git shell command wrappers

index.ts — Orchestration:
- Module state: _projects[], _selectedFile, _fileTreeCache, _expandedPaths, etc.
- loadProjects() — data fetch + delegate
- addProjectFolder() / removeProject() / promptAddFolder()
- bindEvents() — event wiring
- Re-exports

Rules:
- All shared utils from ../components/helpers and ../components/toast
- atoms.ts is pure — no DOM, no IPC, no Tauri plugin imports
- Security predicates (isSensitivePath, isOutOfProjectScope) must remain intact — do not modify their logic
- Delete old projects.ts after creating directory
- npx tsc --noEmit — zero errors
- Commit as: refactor: decompose projects.ts into atomic modules (Phase 4)
```

---

## Phase 5: Decompose `memory-palace.ts` (864 → 4 files)

```
You are refactoring src/views/ in /workspaces/paw/. Read VIEWS_REFACTOR.md — it is the authoritative plan. You are executing Phase 5 only. Phases 0-4 are already committed.

Decompose src/views/memory-palace.ts (864 lines) into atomic design pattern:

src/views/memory-palace/
├── atoms.ts          # ~80 lines — types, state helpers
├── molecules.ts      # ~250 lines — stats, sidebar, recall card, remember form
├── graph.ts          # ~250 lines — canvas knowledge graph visualization
└── index.ts          # ~100 lines — state, loadMemoryPalace, initPalaceEvents, exports

atoms.ts — Pure logic:
- Memory data interfaces/types
- State helper predicates (isPalaceAvailable, etc.)
- readMemoryForm() — form data extraction

molecules.ts — DOM + IPC:
- renderEmbeddingStatus() — embedding model status panel
- loadPalaceStats() — stats card rendering
- loadPalaceSidebar() — sidebar with search/recall
- palaceRecallById() — single memory card display
- renderRecallCard() — memory card template
- Tab switching (recall/remember/graph)
- Recall search with semantic query
- Remember form with store/backfill

graph.ts — Canvas knowledge graph (self-contained):
- renderPalaceGraph() — canvas-based knowledge graph visualization
- Graph layout algorithm
- Canvas event handlers (pan, zoom, click)
- Node/edge rendering

index.ts — Orchestration:
- Module state: _palaceInitialized, _palaceAvailable, _palaceSkipped
- loadMemoryPalace() — data fetch + delegate
- initPalaceEvents() — event wiring
- setCurrentSessionKey() — state setter
- resetPalaceState() — state reset
- Re-exports

IPC Migration: If using raw invoke() calls, migrate to pawEngine equivalents.

Rules:
- All shared utils from ../components/helpers and ../components/toast
- Use isConnected() from ../state/connection instead of local wsConnected
- atoms.ts is pure — no DOM, no IPC, no canvas
- Delete old memory-palace.ts after creating directory
- npx tsc --noEmit — zero errors
- Commit as: refactor: decompose memory-palace.ts into atomic modules (Phase 5)
```

---

## Phase 6: Decompose `settings-skills.ts` (807 → 4 files)

```
You are refactoring src/views/ in /workspaces/paw/. Read VIEWS_REFACTOR.md — it is the authoritative plan. You are executing Phase 6 only. Phases 0-5 are already committed.

Decompose src/views/settings-skills.ts (807 lines) into atomic design pattern:

src/views/settings-skills/
├── atoms.ts          # ~100 lines — CATEGORY_META, SKILL_ICON_MAP, POPULAR_REPOS/TAGS, formatInstalls
├── molecules.ts      # ~250 lines — skill card, credential fields, binary status, filter
├── community.ts      # ~200 lines — community section, search, browse repo, install
└── index.ts          # ~100 lines — loadSkillsSettings, bindEvents, exports

atoms.ts — Pure data:
- CATEGORY_META — skill category definitions with icons/labels
- SKILL_ICON_MAP — skill name to icon mapping
- msIcon() / skillIcon() — pure icon string builders
- POPULAR_REPOS / POPULAR_TAGS — static catalogs
- formatInstalls() — number formatter

molecules.ts — DOM rendering:
- renderSkillsPage() — main page layout
- renderSkillCard() — skill card with toggle/credentials
- renderBinaryStatus() / renderEnvVarStatus() — dependency badges
- renderCredentialFields() — credential input fields
- renderAdvancedSection() — collapsible instructions editor
- bindFilterEvents() / bindSkillEvents() — event handlers

community.ts — Community skills browser:
- renderCommunitySection() — community skills hero section
- renderCommunityCard() / renderDiscoveredCard() — community skill cards
- browseRepo() / searchSkills() — search and browse actions
- wireInstallButtons() — install button handlers
- bindCommunityEvents() — community event wiring

index.ts — Orchestration:
- Module state: _currentFilter
- loadSkillsSettings() — data fetch + delegate
- Re-exports

Rules:
- All shared utils from ../components/helpers and ../components/toast
- atoms.ts is pure — no DOM, no IPC
- Delete old settings-skills.ts after creating directory
- npx tsc --noEmit — zero errors
- Commit as: refactor: decompose settings-skills.ts into atomic modules (Phase 6)
```

---

## Phase 7: Decompose `research.ts` (710 → 3 files)

```
You are refactoring src/views/ in /workspaces/paw/. Read VIEWS_REFACTOR.md — it is the authoritative plan. You are executing Phase 7 only. Phases 0-6 are already committed.

Decompose src/views/research.ts (710 lines) into atomic design pattern:

src/views/research/
├── atoms.ts          # ~80 lines — types, extractDomain, parseProgressStep, stream protocol
├── molecules.ts      # ~300 lines — project list, findings, sources, live feed, progress
└── index.ts          # ~120 lines — state, loadProjects, initResearchEvents, exports

atoms.ts — Pure logic:
- extractDomain() — URL domain extraction
- parseProgressStep() — streaming progress parser
- Stream protocol functions: appendDelta, resolveStream, getContent, setContent (if pure state)
- Research-specific types/interfaces

molecules.ts — DOM + IPC:
- renderLiveSourceFeed() — live source cards during research
- renderProgressSteps() — step-by-step progress display
- loadProjects() / openProject() — project list + detail rendering
- renderFindings() — findings cards with markdown
- renderSourcesPanel() — sources sidebar
- showFindingDetail() — detail modal

index.ts — Orchestration:
- Module state: wsConnected, _activeProject, _findings[], _isResearching, _researchMode, _runId, _streamContent, _streamResolve, _liveSources[], _liveSteps[], promptModalFn
- configure() — callback setup
- loadProjects() — data fetch + delegate
- initResearchEvents() — event wiring
- Stream state management
- Re-exports

Rules:
- All shared utils from ../components/helpers and ../components/toast
- Use isConnected() from ../state/connection instead of local wsConnected
- atoms.ts is pure — no DOM, no IPC
- Delete old research.ts after creating directory
- npx tsc --noEmit — zero errors
- Commit as: refactor: decompose research.ts into atomic modules (Phase 7)
```

---

## Phase 8: Decompose `settings.ts` (647 → 3 files)

```
You are refactoring src/views/ in /workspaces/paw/. Read VIEWS_REFACTOR.md — it is the authoritative plan. You are executing Phase 8 only. Phases 0-7 are already committed.

Decompose src/views/settings.ts (647 lines) into atomic design pattern:

src/views/settings-main/
├── atoms.ts          # ~60 lines — types, budget helpers, ToolRule interface
├── molecules.ts      # ~300 lines — status, usage, audit log, security policies, approvals
└── index.ts          # ~100 lines — state, loadSettings, initSettings, exports

Note: Directory is settings-main/ (not settings/) so it doesn't collide with the many settings-*.ts files that remain flat.

atoms.ts — Pure logic:
- ToolRule interface
- getBudgetLimit() / setBudgetLimit() — localStorage helpers
- downloadFile() — generic file download util
- BUDGET_KEY constant
- Export format builders (exportAuditJSON, exportAuditCSV) if they produce data without DOM

molecules.ts — DOM + IPC:
- loadSettingsStatus() — status panel rendering
- loadSettingsLogs() — log viewer
- loadSettingsUsage() — usage stats with auto-refresh
- initBudgetSettings() — budget form
- loadSettingsPresence() — presence info
- loadSettingsNodes() — node status
- loadSettingsDevices() — device list
- loadSettingsWizard() — setup wizard
- loadSettingsApprovals() — approval queue
- loadSecurityAudit() — audit log table
- loadSecurityPolicies() — policy form with per-agent overrides
- renderToolRules() / addToolRule() — dynamic rule table
- updateSessionOverrideBanner() — status banner
- updateEncryptionStatus() — encryption status check

index.ts — Orchestration:
- Module state: _usageRefreshInterval, _toolRules[], _overrideBannerInterval
- startUsageAutoRefresh() / stopUsageAutoRefresh() — timer management
- checkTokenAutoRotation() — rotation logic
- loadSettings() / initSettings() — init
- Re-exports

Rules:
- All shared utils from ../components/helpers and ../components/toast
- Use isConnected() from ../state/connection instead of local wsConnected
- atoms.ts is pure — no DOM, no IPC
- Delete old settings.ts after creating directory
- Update any imports in other files that reference ./settings or ../views/settings
- npx tsc --noEmit — zero errors
- Commit as: refactor: decompose settings.ts into atomic modules (Phase 8)
```

---

## Phase 9: Decompose `tasks.ts` (582 → 3 files)

```
You are refactoring src/views/ in /workspaces/paw/. Read VIEWS_REFACTOR.md — it is the authoritative plan. You are executing Phase 9 only. Phases 0-8 are already committed.

Decompose src/views/tasks.ts (582 lines) into atomic design pattern:

src/views/tasks/
├── atoms.ts          # ~60 lines — COLUMNS, types
├── molecules.ts      # ~280 lines — board, cards, feed, stats, task modal, agent picker, drag-and-drop
└── index.ts          # ~100 lines — state, loadTasks, bindTaskEvents, cron timer, exports

atoms.ts — Pure data:
- COLUMNS constant — task status columns definition
- Task/Activity types/interfaces
- Any pure helper functions

molecules.ts — DOM + IPC:
- renderBoard() — kanban board columns
- createTaskCard() — task card with drag events
- renderFeed() — activity feed
- renderStats() — stats counters
- openTaskModal() — task create/edit modal
- renderAgentPicker() — multi-agent tag picker
- saveTask() / deleteTask() / runTask() — CRUD actions
- setupDragAndDrop() — drag & drop handlers

index.ts — Orchestration:
- Module state: _tasks[], _activity[], _editingTask, _feedFilter, _agents[], _modalSelectedAgents[], _cronInterval
- loadTasks() — data fetch + delegate
- setAgents() — state setter
- onTaskUpdated() — event handler
- startCronTimer() / stopCronTimer() — cron management
- bindTaskEvents() — event wiring
- Re-exports

Rules:
- All shared utils from ../components/helpers and ../components/toast
- Use formatTimeAgo from ../components/helpers (added in Phase 0C)
- atoms.ts is pure — no DOM, no IPC
- Delete old tasks.ts after creating directory
- npx tsc --noEmit — zero errors
- Commit as: refactor: decompose tasks.ts into atomic modules (Phase 9)
```

---

## Phase 10: Decompose 4 remaining files

```
You are refactoring src/views/ in /workspaces/paw/. Read VIEWS_REFACTOR.md — it is the authoritative plan. You are executing Phase 10 only. Phases 0-9 are already committed.

Decompose these 4 files into atomic design pattern. Each gets 3 files (atoms/molecules/index):

1. src/views/settings-browser.ts (563 lines) → src/views/settings-browser/
   atoms.ts — types only (~40 lines)
   molecules.ts — profiles, options, screenshots, workspaces, network policy (~280 lines)
   index.ts — loadBrowserSettings, exports (~80 lines)

2. src/views/today.ts (523 lines) → src/views/today/
   atoms.ts — getWeatherIcon, getGreeting, getPawzMessage, Task type (~60 lines)
   molecules.ts — weather, emails, tasks, quick actions, dashboard grid (~250 lines)
   index.ts — state, loadToday, initToday, exports (~80 lines)
   IPC Migration: raw invoke() → pawEngine equivalents

3. src/views/orchestrator.ts (509 lines) → src/views/orchestrator/
   atoms.ts — specialtyIcon, messageKindLabel, types (~60 lines)
   molecules.ts — project list, detail, agent roster, message bus, modals (~250 lines)
   index.ts — state, loadProjects, initOrchestrator, exports (~80 lines)

4. src/views/settings-voice.ts (505 lines) → src/views/settings-voice/
   atoms.ts — voice catalogs (GOOGLE/OPENAI/ELEVENLABS_VOICES), LANGUAGES (~100 lines)
   molecules.ts — settings form, sliders, TTS test, talk mode (~250 lines)
   index.ts — loadVoiceSettings, initVoiceSettings, exports (~60 lines)

Rules:
- All shared utils from ../components/helpers and ../components/toast
- Use formatTimeAgo from ../components/helpers where needed
- atoms.ts files are pure — no DOM, no IPC
- Delete old flat files after creating directories
- npx tsc --noEmit — zero errors
- Commit as: refactor: decompose remaining oversized views into atomic modules (Phase 10)
```

---

## Phase 11: Final Import Cleanup

```
You are refactoring src/views/ in /workspaces/paw/. Read VIEWS_REFACTOR.md — it is the authoritative plan. You are executing Phase 11 (final phase). Phases 0-10 are already committed.

Final cleanup:
1. Check main.ts — all view imports should resolve correctly (TypeScript resolves ./views/agents → ./views/agents/index.ts automatically)
2. Check any cross-view imports (views importing from other views)
3. Run: find src/views -maxdepth 1 -name '*.ts' | sort — only flat files under 400 lines should remain (settings-advanced, settings-agent-defaults, automations, trading, settings-config, settings-engine, foundry, nodes, settings-tailscale, settings-sessions, settings-env, content, router, settings-tabs, skills)
4. Run: wc -l $(find src/views -name '*.ts') | sort -rn — no single file should exceed ~300 lines
5. Run: grep -rn 'function escHtml\|function escapeHtml\|function escAttr\|const \$ = .*getElementById' src/views/ — must return zero
6. Run: npx tsc --noEmit — zero errors
7. Update VIEWS_REFACTOR.md — mark all phases as completed

Commit as: refactor: final views cleanup and verification (Phase 11)
```

---

## Quick Reference — Phase Status

| Phase | Target | Status |
|-------|--------|--------|
| 0 | Deduplicate shared utilities | ✅ be54dac |
| 1 | agents.ts (1,586 → 5 files) | ⬜ |
| 2 | channels.ts (1,166 → 4 files) | ⬜ |
| 3 | mail.ts (985 → 4 files) | ⬜ |
| 4 | projects.ts (956 → 4 files) | ⬜ |
| 5 | memory-palace.ts (864 → 4 files) | ⬜ |
| 6 | settings-skills.ts (807 → 4 files) | ⬜ |
| 7 | research.ts (710 → 3 files) | ⬜ |
| 8 | settings.ts (647 → 3 files) | ⬜ |
| 9 | tasks.ts (582 → 3 files) | ⬜ |
| 10 | 4 remaining (browser/today/orch/voice) | ⬜ |
| 11 | Final import cleanup | ⬜ |
