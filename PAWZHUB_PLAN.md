# PawzHub — Build Plan

> Community skill marketplace + modular workspace system for Pawz.

---

## Vision

Pawz becomes a fully modular AI workspace where:

1. **Skills are community-driven** — anyone can create, publish, and install skills from PawzHub
2. **Workspaces are customizable** — users configure which views/tabs appear based on their use case
3. **Output is visual** — skills can declare dashboard widgets so their data lives beyond chat
4. **Creation happens in-app** — a wizard in Pawz generates, tests, and publishes skills without touching code

---

## Architecture Overview

```
┌──────────────────────────────────────────────────────────┐
│                     Pawz App                             │
│                                                          │
│  ┌─────────┐  ┌─────────────┐  ┌──────────────────────┐ │
│  │ Modular  │  │   Skill     │  │   Widget Renderer    │ │
│  │ Sidebar  │  │   Engine    │  │  (dashboard cards)   │ │
│  │ (views)  │  │             │  │                      │ │
│  └────┬─────┘  └──────┬──────┘  └──────────┬───────────┘ │
│       │               │                    │             │
│       ▼               ▼                    ▼             │
│  ┌─────────┐  ┌─────────────┐  ┌──────────────────────┐ │
│  │Workspace│  │ Skill       │  │   Skill Output       │ │
│  │ Config  │  │ Loader      │  │   Store (DB)         │ │
│  │ (DB)    │  │             │  │                      │ │
│  └─────────┘  └──────┬──────┘  └──────────────────────┘ │
│                       │                                  │
│            ┌──────────┴──────────┐                       │
│            ▼                     ▼                       │
│     builtin_skills()    ~/.paw/skills/                   │
│     (30+ core)          (community TOML)                 │
│                                                          │
└──────────────────────────────────────────────────────────┘
                         │
                         ▼
              ┌──────────────────┐
              │    PawzHub       │
              │  (GitHub repo)   │
              │  registry.json   │
              │  skills/*/       │
              └──────────────────┘
```

---

## Current State

### What We Have

The skill system is already well-architected in `src-tauri/src/engine/skills.rs` (1,227 lines):

- **`SkillDefinition`** struct — id, name, category, credentials, tool names, agent instructions, install hints
- **`SkillStatus`** — enable/disable per agent, credential validation, binary/env checking
- **Vault encryption** — OS keychain + XOR for API keys, AES-GCM for DB fields
- **Custom instructions** — users can override any skill's agent instructions
- **Credential injection** — decrypted values injected into agent prompts at runtime, never in plain text

The engine tools that skills use are already production-quality:

- **`fetch`** — full HTTP client (GET/POST/PUT/PATCH/DELETE), custom headers, body, 30s timeout, domain allowlist/blocklist enforcement
- **`exec`** — shell execution with Docker sandbox routing, per-agent workspace directories, output truncation
- **`read_file` / `write_file`** — file I/O with path security (blocked from reading engine source)

### The Problems

1. **All 30+ skills are hardcoded** in `builtin_skills()` (~950 lines). Every skill tool is hardcoded in `execute_tool()` with a giant `match` statement. No way to load skills at runtime or let the community contribute.

2. **All views are hardcoded** in `index.html` and `router.ts`. Every user sees the same 17 navigation tabs regardless of their use case. A marketer sees Trading. A trader sees Content Studio.

3. **Skill output is chat-only.** Skills like server monitoring, analytics, or portfolio tracking produce data worth persisting — but it all disappears in the chat scroll.

### The Insight

- ~80% of existing skills are **instruction-only** — they inject text into the system prompt telling the agent how to use `exec`/`fetch` with a particular CLI or API. These are trivially pluggable.
- The ~20% that are **vault skills** (with dedicated Rust tool functions like `email_send`, `dex_swap`) stay as built-in core skills.
- The `fetch` tool already supports arbitrary HTTP with custom headers — any REST API is reachable. The `exec` tool already supports CLI access with sandbox routing. The credential vault already encrypts and injects API keys. **The platform is ready for plugins.**

---

## Competitive Analysis: OpenClaw's Ecosystem

OpenClaw (ClawHub) has 5,705 community skills. A third-party audit (VoltAgent) found:

| Filtered out | Count |
|--------------|-------|
| Spam / junk / test skills | 1,180 |
| Crypto / blockchain (blanket category ban) | 672 |
| Duplicates | 492 |
| **Malicious** (security audit flagged) | **396** |
| Non-English | 8 |
| **Total rejected** | **2,748 (48%)** |

Only 3,002 of 5,705 passed basic curation. Their format is a freeform `SKILL.md` — no structured credential fields, no typed categories, no widget spec, no version management. Users set credentials via environment variables manually.

### PawzHub Advantages Over ClawHub

| | ClawHub (OpenClaw) | PawzHub (Pawz) |
|--|---|---|
| **Format** | Freeform SKILL.md (prose) | Structured `pawz-skill.toml` (typed schema) |
| **Credentials** | Manual env vars | Typed `[[credentials]]` fields, vault-encrypted, auto-injected |
| **Output** | Chat text only | Dashboard widgets + chat |
| **Quality** | 48% junk/malicious | Created-in-app, tested in workspace, CI-validated |
| **Security** | VirusTotal scan after publish | Validated at submission, domain policy enforced at runtime |
| **Modularity** | Drop in folder | Per-workspace profiles, customizable sidebar |
| **Creation** | Write markdown by hand | In-app wizard + AI generation |
| **Versioning** | None | Semver in manifest, update detection |

---

## Phase 1 — Plugin Runtime (~1 week)

> Make skills loadable from disk instead of hardcoded.

### 1.1 Skill Manifest Format

Define `pawz-skill.toml` as the standard skill package format:

```toml
[skill]
id = "notion"
name = "Notion"
version = "1.0.0"
author = "community"
category = "productivity"
icon = "edit_note"
description = "Read and write Notion pages, databases, and blocks via the API"
install_hint = "Get your API key at https://www.notion.so/my-integrations"

[[credentials]]
key = "NOTION_API_KEY"
label = "Integration Token"
description = "Your Notion internal integration token"
required = true
placeholder = "secret_..."

[instructions]
text = """
You have access to the Notion API via the user's integration token.

## Reading pages
Use `fetch` with:
- URL: `https://api.notion.so/v1/pages/{page_id}`
- Headers: `Authorization: Bearer {NOTION_API_KEY}`, `Notion-Version: 2022-06-28`

## Searching
Use `fetch` with POST to `https://api.notion.so/v1/search`
Body: {"query": "search term"}
"""

[widget]
type = "table"
title = "Recent Pages"

[[widget.fields]]
key = "title"
label = "Page"
type = "text"

[[widget.fields]]
key = "updated"
label = "Last Updated"
type = "datetime"

[[widget.fields]]
key = "status"
label = "Status"
type = "badge"
```

**Implementation:**
- New file: `src-tauri/src/engine/skill_manifest.rs`
- Parse TOML into `SkillDefinition` using the `toml` crate (already a Cargo dependency)
- Schema validation: required fields, valid category enum, semver format, safe ID (alphanumeric + hyphens)
- Maps directly to existing `SkillDefinition` struct — no schema changes needed

### 1.2 Skill Loader

Load community skill manifests from `~/.paw/skills/` at startup, merge with built-in skills.

```
~/.paw/skills/
├── notion/
│   └── pawz-skill.toml
├── linear/
│   └── pawz-skill.toml
└── airtable/
    └── pawz-skill.toml
```

**Implementation:**
- New file: `src-tauri/src/engine/skill_loader.rs`
  - `load_community_skills()` — scan `~/.paw/skills/*/pawz-skill.toml`, parse each, return `Vec<SkillDefinition>`
  - `all_skills()` — concatenate `builtin_skills()` + `load_community_skills()`, deduplicate by ID (builtin wins on collision)
- Modify `skills.rs`:
  - `get_all_skill_status()` calls `all_skills()` instead of `builtin_skills()`
  - `get_enabled_skill_instructions()` calls `all_skills()` instead of `builtin_skills()`
- Community skills flagged with `source: "community"` vs `source: "builtin"` for UI badges

### 1.3 Install / Uninstall Commands

**Implementation:**
- New Tauri command: `install_skill(url: String)` — download TOML (or zip), write to `~/.paw/skills/{id}/pawz-skill.toml`, validate manifest
- New Tauri command: `uninstall_skill(skill_id: String)` — delete `~/.paw/skills/{id}/` folder, clean up DB records (enabled state, credentials)
- Manifest validation on install:
  - Required fields present
  - ID is safe (no path traversal, no slashes, alphanumeric + hyphens only)
  - Category is a valid enum value
  - No executable code in instructions (instruction-only enforcement)

### 1.4 UI — Skill Management

**Implementation in `src/views/settings-skills.ts`:**
- Badge on each skill card: "Core" (grey) or "Community" (magenta)
- Uninstall button for community skills (hidden for core skills)
- Import from file: drag-and-drop a `.toml` or `.zip` to install a skill manually
- Skill detail panel shows source, version, author

---

## Phase 2 — Skill Output Widgets (~1 week)

> Skills that produce data get persistent visual output on the dashboard.

### 2.1 Widget Spec in Manifest

The `[widget]` section of `pawz-skill.toml` declares a small structured display:

**5 widget types:**

| Type | Description | Display |
|------|-------------|---------|
| `status` | Single status indicator | Badge + label + timestamp |
| `metric` | Single number with trend | Large number + delta arrow |
| `table` | Rows of structured data | Column headers + data rows |
| `log` | Chronological event feed | Timestamped entries, newest first |
| `kv` | Key-value pairs | Label: value list |

**6 field types:**

| Type | Rendering |
|------|-----------|
| `text` | Plain text |
| `number` | Formatted with locale separators |
| `badge` | Colored pill (green/yellow/red based on value) |
| `datetime` | Relative time ("2 hours ago") |
| `percentage` | Bar + number |
| `currency` | Dollar/euro symbol + formatted number |

### 2.2 Skill Output Store

When the agent uses a skill and produces structured data, it writes to the skill's output store:

**Implementation:**
- New DB table: `skill_outputs` (skill_id, agent_id, data JSON, updated_at)
- New Tauri command: `write_skill_output(skill_id, data)` — agent calls this via a new `skill_output` tool
- New Tauri command: `read_skill_output(skill_id)` — dashboard reads this to render widgets
- Output data is a JSON object where keys match the `widget.fields[].key` values

### 2.3 Dashboard Widget Renderer

The Today/Dashboard view renders active skill widgets:

**Implementation in `src/views/today.ts`:**
- After loading tasks and weather, query `read_skill_output` for all enabled skills that have `[widget]` defined
- Render each widget using one of the 5 built-in renderers (status card, metric, table, log, kv)
- Widgets are styled consistently using the Pawz brand palette
- Click on a widget opens the skill detail panel or starts a chat about that skill

### 2.4 Widget Refresh

Skills that need periodic updates:
- `refresh = "5m"` in the widget spec — agent re-runs the skill's data fetch on an interval
- Uses existing automation/cron infrastructure
- Manual refresh button on each widget

---

## Phase 3 — PawzHub Registry (~1 week)

> The community marketplace. GitHub-hosted, no server needed.

### 3.1 Registry Repository

Create `github.com/elisplash/pawzhub`:

```
pawzhub/
├── registry.json              # Master index of all skills
├── skills/
│   ├── notion/
│   │   └── pawz-skill.toml
│   ├── linear/
│   │   └── pawz-skill.toml
│   └── ...
├── CONTRIBUTING.md            # How to submit a skill
├── SKILL_TEMPLATE.toml        # Template for new skills
└── .github/
    └── workflows/
        └── validate.yml       # CI: validate manifests on PR
```

**`registry.json` format:**

```json
{
  "version": 1,
  "updated": "2026-02-20T00:00:00Z",
  "skills": [
    {
      "id": "notion",
      "name": "Notion",
      "version": "1.0.0",
      "author": "elisplash",
      "category": "productivity",
      "description": "Read and write Notion pages via the API",
      "icon": "edit_note",
      "has_widget": true,
      "download_url": "https://raw.githubusercontent.com/elisplash/pawzhub/main/skills/notion/pawz-skill.toml"
    }
  ]
}
```

### 3.2 Submission Flow

Contributors submit skills via GitHub PR:

1. Fork `elisplash/pawzhub`
2. Create `skills/{skill-id}/pawz-skill.toml` (use SKILL_TEMPLATE.toml as base)
3. Open PR — CI validates the manifest automatically
4. Maintainer reviews and merges
5. CI auto-rebuilds `registry.json`

**CI validation checks (`validate.yml`):**
- Valid TOML syntax, all required fields present
- Unique skill ID (no collision with built-in or existing community skills)
- Category is one of the defined enums
- No executable code (instruction-only for Phase 1)
- `id` is alphanumeric + hyphens, no path traversal
- Version is valid semver
- Description length (10-500 chars)
- Widget fields reference valid types if `[widget]` is present

**Recommended (not required) for higher quality:**
- Skill tested in a Pawz workspace (screenshot/demo in PR description)
- Skills created via the in-app wizard get a "Created in Pawz" badge

### 3.3 In-App PawzHub Browser

New "Browse PawzHub" tab in the Skills view:

**UI Flow:**
1. Fetches `registry.json` from GitHub raw (cached locally, refreshed on open)
2. Grid of available skills — icon, name, author, category, description, `has_widget` badge
3. Filter by category, search by name
4. "Install" button downloads `pawz-skill.toml` to `~/.paw/skills/{id}/`
5. "Installed" badge + "Uninstall" for already-installed skills
6. "Update Available" indicator when registry version > installed version

**Implementation:**
- New section in `src/views/settings-skills.ts` or separate `src/views/pawzhub.ts`
- New Tauri commands: `fetch_pawzhub_registry()`, `install_pawzhub_skill(id: String)`
- Category filter matches existing `SkillCategory` enum

### 3.4 Version Management

- Semver in every manifest (`version = "1.0.0"`)
- App compares installed version vs registry version on PawzHub tab open
- "Update" button when newer version available
- Changelog field in manifest (optional `[changelog]` section)

---

## Phase 4 — Modular Workspace (~1 week)

> Users customize which views appear in their sidebar based on their use case.

### 4.1 The Problem

Currently, all 17 navigation tabs are hardcoded in `index.html`:

| View | Icon | Every user sees it |
|------|------|--------------------|
| Dashboard | `dashboard` | Yes |
| Today | `today` | Yes |
| Chat | `chat` | Yes |
| Agents | `smart_toy` | Yes |
| Tasks | `task_alt` | Yes |
| Orchestrator | `account_tree` | Yes |
| Projects | `code` | Yes |
| Content Studio | `edit_note` | Yes |
| Mail | `mail` | Yes |
| Automations | `schedule` | Yes |
| Channels | `forum` | Yes |
| Research | `science` | Yes |
| Nodes | `hub` | Yes |
| Trading | `candlestick_chart` | Yes |
| Memory Palace | `psychology` | Yes |
| Skills | `extension` | Yes |
| Foundry | `tune` | Yes |

A user who only uses Pawz for marketing has a Trading tab they never click. A trader has Content Studio they never open. A developer has neither but sees both.

### 4.2 Workspace Profiles

Users choose a workspace profile (or custom) that configures which views are visible:

**Preset profiles:**

| Profile | Visible Views | Hidden Views |
|---------|---------------|--------------|
| **All** (default) | Everything | Nothing |
| **Developer** | Chat, Agents, Tasks, Projects, Research, Orchestrator, Nodes, Memory, Skills, Settings | Trading, Content Studio, Mail |
| **Marketer** | Chat, Agents, Tasks, Content Studio, Mail, Research, Automations, Skills, Settings | Trading, Projects, Nodes, Orchestrator |
| **Trader** | Chat, Agents, Tasks, Trading, Research, Automations, Memory, Skills, Settings | Content Studio, Projects, Nodes, Mail |
| **Minimal** | Chat, Agents, Tasks, Skills, Settings | Everything else |
| **Custom** | User picks exactly which views to show | User picks |

### 4.3 Implementation

**Database:**
- New config key: `workspace_profile` (string: "all" / "developer" / "marketer" / "trader" / "minimal" / "custom")
- New config key: `workspace_visible_views` (JSON array of view IDs, used when profile is "custom")

**Sidebar rendering (`index.html` + `router.ts`):**
- On app load, read `workspace_profile` and `workspace_visible_views` from config
- Show/hide nav items based on profile
- Core views that are always visible regardless of profile: Dashboard, Chat, Agents, Skills, Settings
- Everything else is toggleable

**Settings UI (new section in Settings or dedicated "Workspace" tab):**
- Profile selector dropdown (All, Developer, Marketer, Trader, Minimal, Custom)
- When "Custom" selected, show checkbox grid of all available views
- Changes apply immediately (sidebar updates live)
- "Reset to Default" button

**Router changes (`src/views/router.ts`):**
- `allViewIds` becomes dynamic based on profile
- `switchView()` respects visibility — trying to navigate to a hidden view redirects to Dashboard
- Nav items not in the profile are `display: none`

### 4.4 Skill-Driven Views (Future)

Eventually, community skills could register their own custom views/tabs in the sidebar. For now, the modular workspace only controls visibility of the existing 17 built-in views. Custom skill views are a Phase 6 feature.

---

## Phase 5 — In-App Skill Creator (~3-5 days)

> Create, test, and publish skills without leaving Pawz.

### 5.1 Create Skill Wizard

New panel in the Skills view: "Create New Skill"

**Step 1: Basic Info**
- Name, ID (auto-generated from name), description
- Category selector (dropdown of existing categories)
- Icon picker (Material Symbols grid)
- Author name (pre-filled from settings)

**Step 2: Credentials**
- Add credential fields: key, label, description, required, placeholder
- Dynamic form — add/remove credential rows
- Preview of what the user will see when configuring the skill

**Step 3: Instructions**
- Large text editor for agent instructions
- Template starters: "REST API", "CLI Tool", "Web Scraper"
- Markdown preview
- Hint: "Tell the agent which URLs to fetch, which commands to exec, and how to parse the response"

**Step 4: Widget (Optional)**
- Widget type selector (status, metric, table, log, kv)
- Add widget fields: key, label, type
- Live preview of what the widget will look like

**Step 5: Test**
- Enable the skill for the current agent
- Start a chat — verify the agent can use the skill correctly
- Widget output appears on dashboard if configured

**Step 6: Export / Publish**
- "Save Locally" — writes `pawz-skill.toml` to `~/.paw/skills/{id}/`
- "Export TOML" — downloads the file
- "Publish to PawzHub" — opens a pre-filled GitHub PR on `elisplash/pawzhub` (uses `gh` CLI or GitHub API)

### 5.2 AI-Assisted Skill Creation

The agent itself can generate skills:

- User says: "Create a skill for the Notion API"
- Agent fetches Notion API docs, generates a complete `pawz-skill.toml`
- Wizard pre-fills with the generated content
- User reviews, tests, publishes

This is essentially free — it's a prompt template that uses the agent's existing `fetch` + text generation capabilities.

---

## Phase 6 — Advanced (Later)

| Feature | Description | Effort |
|---------|-------------|--------|
| **WASM plugin tools** | Skills that provide custom tool functions via WASM (not just instructions) | 2-4 weeks |
| **Skill ratings & reviews** | Star ratings, install counts, community feedback | ~2 days |
| **Skill dependencies** | Skill A requires Skill B to be installed | ~1 day |
| **Auto-update** | Background check for skill updates, one-click update all | ~1 day |
| **Skill analytics** | Install counts, usage frequency (opt-in) | ~2 days |
| **Skill bundles** | Curated packs ("Developer Kit", "Trader Pack", "Smart Home Bundle") | ~1 day |
| **Private registries** | Corporate/team skill registries behind auth | ~2 days |
| **Custom skill views** | Skills that register their own navigation tab/view | ~1 week |
| **Skill revenue sharing** | Paid premium skills with creator payouts | Complex |

---

## Security Model

### Instruction-Only Enforcement (Phase 1-3)

Community skills are instruction-only. They cannot:
- Execute arbitrary Rust code (no compiled tool functions)
- Bypass the engine's security policies (domain allowlist/blocklist enforced by `execute_fetch`)
- Access the OS keychain directly (credential injection is handled by the engine)
- Read engine source code (blocked by `execute_read_file`)
- Install blocked packages (enforced by `execute_exec`)

They can only:
- Inject text into the agent's system prompt (instructions)
- Declare credential fields that the engine encrypts and injects
- Declare widget schemas that the app renders

### Security Checks on Install

- Manifest TOML is parsed with strict validation (no arbitrary keys)
- Skill ID must be alphanumeric + hyphens (no path traversal)
- Instructions are text-only (scanned for suspicious patterns)
- Credential keys are validated (no collision with engine internals)
- Widget specs are validated against the 5 allowed types

### Runtime Security

- All credentials vault-encrypted (AES-GCM + OS keychain key)
- Network requests go through `execute_fetch` which enforces domain policy
- Shell commands go through `execute_exec` which routes through Docker sandbox when enabled
- Agent workspace isolation (each agent has its own working directory)

---

## Implementation Timeline

```
Week 1:  Phase 1 — Manifest format, skill loader, install/uninstall, UI badges
Week 2:  Phase 2 — Widget spec, output store, dashboard renderers, refresh
Week 3:  Phase 3 — PawzHub registry, CI, in-app browser, version management
Week 4:  Phase 4 — Modular workspace profiles, sidebar customization
Week 5:  Phase 5 — Create Skill wizard, AI generation, publish flow
Week 6:  Seed 30-50 community skills, polish, documentation, launch
```

---

## Starter Skills to Seed PawzHub

High-value skills the community would want immediately (all instruction-based):

| Skill | Category | API | Widget Type |
|-------|----------|-----|-------------|
| Notion | Productivity | notion.so/v1 | Table (recent pages) |
| Linear | Productivity | api.linear.app | Table (active issues) |
| Airtable | Productivity | airtable.com/v0 | Table (records) |
| Jira | Productivity | atlassian.net REST | Table (sprint issues) |
| Stripe | API | api.stripe.com | Metric (MRR) |
| Twilio | Communication | api.twilio.com | Log (recent messages) |
| SendGrid | Communication | api.sendgrid.com | Metric (delivery rate) |
| OpenWeatherMap | API | openweathermap.org | Status (conditions) |
| Supabase | Development | supabase.co | KV (project stats) |
| Vercel | Development | api.vercel.com | Table (deployments) |
| Cloudflare | Infrastructure | api.cloudflare.com | Table (zone status) |
| PagerDuty | DevOps | api.pagerduty.com | Log (incidents) |
| Shopify | E-commerce | shopify.dev | Metric (revenue) |
| YouTube Data | Media | googleapis.com | Table (video stats) |
| X / Twitter | Social | api.x.com | Log (mentions) |
| Todoist | Productivity | api.todoist.com | Table (todos) |
| Calendly | Productivity | api.calendly.com | Table (upcoming) |
| Figma | Design | api.figma.com | Table (files) |
| Reddit | Social | oauth.reddit.com | Log (mentions) |
| Pinecone | AI/ML | pinecone.io | KV (index stats) |

---

## Key Design Decisions

1. **Instruction-only first** — Community skills use the agent's existing `exec`/`fetch` tools. No custom Rust code. Safe, fast to build, covers 80% of use cases. WASM plugins come later.

2. **GitHub-based registry** — No server to maintain. PRs for submissions. CI for validation. Free hosting via raw.githubusercontent.com. The `elisplash/pawzhub` repo is the single source of truth.

3. **Local-first** — Skills are downloaded and stored locally in `~/.paw/skills/`. Works offline after install. No phone-home required.

4. **Core skills stay built-in** — The 30+ existing skills with dedicated Rust tool functions (email, trading, telegram, etc.) remain compiled into the binary. PawzHub extends, it doesn't replace.

5. **Quality over quantity** — OpenClaw's ClawHub has 5,705 skills and 48% are junk or malicious. PawzHub launches with 30-50 tested, high-quality skills with proper credential setup and widget output. In-app creation wizard + CI validation keeps quality high as the ecosystem grows.

6. **Dashboard widgets differentiate** — No other AI agent platform has structured visual output from community skills. A Stripe skill that shows MRR on a dashboard card is fundamentally different from one that dumps JSON in chat.

7. **Modular workspace is user empowerment** — The same app serves a trader, a developer, and a marketer by hiding irrelevant views. Workspace profiles make Pawz feel tailored without requiring different builds.

8. **Backward compatible** — Existing users see no breaking changes. Community skills are additive. The `builtin_skills()` function continues to work unchanged. Modular workspace defaults to "All" (current behavior).
