# Three-Tier Extensibility — Implementation Plan

> **Status**: Implemented (Phases 1–3 complete)  
> **Last updated**: 2025-07-17  
> **Owner**: @elisplash

Pawz has three levels of extensibility. Each tier adds more power and more integration surface.

---

## The Three Tiers

```
┌─────────────────────────────────────────────────────────────────┐
│  Tier 3 — EXTENSIONS                       🟡 Gold badge       │
│  Custom sidebar views · Dashboard widgets · Persistent data    │
│  pawz-skill.toml + [view] + [widget] + [storage]              │
├─────────────────────────────────────────────────────────────────┤
│  Tier 2 — INTEGRATIONS                     🟣 Purple badge     │
│  Credential vault · API access · CLI binaries                  │
│  pawz-skill.toml + [[credentials]] + [instructions]            │
├─────────────────────────────────────────────────────────────────┤
│  Tier 1 — SKILLS                           🔵 Blue badge       │
│  Prompt injection only · Zero config · SKILL.md format         │
│  skills.sh ecosystem · Agent-installable                       │
└─────────────────────────────────────────────────────────────────┘
```

---

## Tier 1 — Skills (DONE ✅)

**Format**: `SKILL.md` (Markdown with YAML frontmatter)  
**Badge**: 🔵 Skill  
**Icon**: `school`

### What's built
- [x] SKILL.md parser in Rust
- [x] skills.sh search API integration
- [x] Community skills hero section with keyword search + popular tags
- [x] GitHub repo browser (owner/repo → scan for SKILL.md files)
- [x] Install/remove/enable/disable per skill
- [x] Per-agent skill scoping (agent_ids column)
- [x] Agent tools: `skill_search`, `skill_install`, `skill_list`
- [x] Instructions injected into system prompt at runtime
- [x] Context truncation preserves last user message to prevent empty-content errors

### What remains
- [ ] Skill version tracking (detect when upstream SKILL.md changes)
- [ ] Bulk install/update from skills.sh collections
- [ ] skill_remove and skill_toggle agent tools
- [ ] "Update available" badge on installed skill cards

---

## Tier 2 — Integrations (DONE ✅)

**Format**: `pawz-skill.toml`  
**Badge**: 🟣 Integration  
**Icon**: `cable`

### What's built
- [x] 40 built-in skills with credential vault (AES-GCM + OS keychain)
- [x] Per-agent enable/disable toggles
- [x] Credential encryption architecture
- [x] Domain allowlist/blocklist enforcement on fetch
- [x] Docker sandbox routing on exec
- [x] **TOML loader**: Parse `pawz-skill.toml` from `~/.paw/skills/{id}/`
- [x] **Credential UI**: Dynamic form generation from `[[credentials]]` declarations
- [x] **Binary detection**: Check PATH for required binaries declared in manifest
- [x] **Install hint display**: Show `install_hint` with links when credentials are missing
- [x] **Hot reload**: Watch `~/.paw/skills/` for new/changed TOML files
- [x] **PawzHub browser**: Browse and install TOML-based skills from the registry
- [x] **Registry format**: `registry.json` schema for TOML skills
- [x] **Skill output tool**: `skill_output` for agents to persist structured JSON
- [x] **Widget data store**: `skill_outputs` table in SQLite for widget data persistence
- [x] **Tier badge UI**: Show purple "Integration" badge on TOML-based skills
- [x] **Agent scoping**: Per-agent install (same as Tier 1)

### Technical design

```
~/.paw/skills/
├── notion/
│   └── pawz-skill.toml      ← Tier 2 integration
├── linear/
│   └── pawz-skill.toml
└── stripe-dashboard/
    └── pawz-skill.toml      ← Tier 3 if it has [view]
```

**Loader flow**:
1. On startup, scan `~/.paw/skills/*/pawz-skill.toml`
2. Parse each TOML → `CommunitySkillDefinition` struct
3. Register in `community_skills` table (or new `integrations` table)
4. If `[[credentials]]` present → generate credential form in UI
5. If `[instructions]` present → inject into agent prompt (same as Tier 1)
6. If `[widget]` present → register widget renderer on Dashboard
7. If `[view]` present → register custom sidebar view (Tier 3)

---

## Tier 3 — Extensions (DONE ✅)

**Format**: `pawz-skill.toml` with `[view]` and/or `[storage]` sections  
**Badge**: 🟡 Extension  
**Icon**: `dashboard_customize`

### What's built
- [x] **`[view]` manifest section**: Declare custom sidebar views
  ```toml
  [view]
  id = "crm-pipeline"
  label = "CRM"
  icon = "contacts"
  ```
- [x] **View renderer**: Render custom HTML/Markdown views from skill output data
- [x] **`[storage]` manifest section**: Declare persistent key-value data
  ```toml
  [storage]
  namespace = "crm"
  tables = ["contacts", "deals", "activities"]
  ```
- [x] **Storage API**: `skill_store_get`, `skill_store_set`, `skill_store_query`, `skill_store_delete` tools
- [x] **Sidebar registration**: Dynamic sidebar items from installed extensions
- [x] **View data binding**: Connect widget.fields to stored data for live rendering
- [x] **Extension lifecycle**: Install → configure → render → update → uninstall
- [x] **Modular workspace integration**: Extensions add to the view toggle grid

### Example: CRM Extension

```toml
[skill]
id = "simple-crm"
name = "Simple CRM"
version = "1.0.0"
author = "community"
category = "productivity"
icon = "contacts"
description = "A simple CRM with contacts, deals, and activity tracking"

[[credentials]]
key = "CRM_WEBHOOK"
label = "Import Webhook"
required = false

[instructions]
text = """
You have a CRM system. Use skill_store_set to save contacts and deals.
Use skill_store_query to search and filter records.
"""

[widget]
type = "table"
title = "Active Deals"
refresh = "5m"

[[widget.fields]]
key = "name"
label = "Deal"
type = "text"

[[widget.fields]]
key = "value"
label = "Amount"
type = "currency"

[[widget.fields]]
key = "stage"
label = "Stage"
type = "badge"

[view]
id = "crm-pipeline"
label = "CRM"
icon = "contacts"

[storage]
namespace = "crm"
tables = ["contacts", "deals", "activities"]
```

---

## Badge System

Every skill/integration/extension shows a tier badge in the UI:

| Tier | Badge | Color | Material Icon | Requirements |
|------|-------|-------|---------------|-------------|
| 1 | Skill | Blue (#3B82F6) | `school` | SKILL.md only |
| 2 | Integration | Purple (#8B5CF6) | `cable` | pawz-skill.toml + credentials/instructions |
| 3 | Extension | Gold (#F59E0B) | `dashboard_customize` | pawz-skill.toml + view and/or storage |

Additional quality badges:

| Badge | Meaning |
|-------|---------|
| **Verified** ✓ | Tested in a real workspace, CI-validated |
| **Official** ★ | Published by `OpenPawz` |
| **Popular** | 50K+ installs on skills.sh or PawzHub |

---

## Implementation Priority

### Phase 1 — Tier 2 Foundation ✅
1. TOML file loader (`~/.paw/skills/*/pawz-skill.toml`)
2. Dynamic credential form generation
3. Binary detection and readiness checks
4. Skill output tool and widget data store
5. Dashboard widget renderer for community skills

### Phase 2 — PawzHub Registry ✅
6. Registry API design (`registry.json` format)
7. In-app PawzHub browser for TOML skills
8. One-click publish flow (GitHub PR)
9. CI validation pipeline for submissions

### Phase 3 — Tier 3 Extensions ✅
10. `[view]` manifest section and sidebar registration
11. `[storage]` manifest section and data API
12. View renderer for custom sidebar views
13. Extension lifecycle management
14. Modular workspace integration (extensions in view grid)

### Phase 4 — Ecosystem Polish
15. Skill version tracking and update notifications
16. Install count tracking and popularity badges
17. Skill review / rating system
18. AI-assisted skill creation (full TOML generation from API docs)
19. Bulk operations (install collection, update all)

---

## Migration Path

Existing built-in skills won't change. The three tiers apply to **community** content:

| Current State | Tier | Format |
|--------------|------|--------|
| 40 built-in skills | N/A (core) | Rust `SkillDefinition` structs |
| Community SKILL.md | Tier 1 — Skill | SKILL.md |
| Community pawz-skill.toml (no view) | Tier 2 — Integration | pawz-skill.toml |
| Community pawz-skill.toml + [view] | Tier 3 — Extension | pawz-skill.toml |

The TOML format is a **superset** of SKILL.md. A skill author can start at Tier 1 with just a SKILL.md, graduate to Tier 2 by creating a pawz-skill.toml with credentials and widgets, and reach Tier 3 by adding a custom view.

---

## Success Criteria

- [x] Community author can create a Tier 2 integration in < 30 minutes
- [x] Dashboard widgets render structured data from community skills
- [x] Per-agent scoping works across all three tiers
- [x] PawzHub browser shows tier badges with correct colors
- [x] No community skill can access credentials from other skills
- [x] Extension views appear in the modular workspace toggle grid
- [x] Hot reload picks up new/changed skills without app restart
