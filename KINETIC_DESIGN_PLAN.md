# Kinetic Intelligence Engine — Complete UI Transformation Plan

> **Codename:** v4.6 "Kinetic"
> **Philosophy:** An industrial-grade intelligence workspace that feels like a control room for AI — not a dashboard, not a chat app. Every surface earns its place by surfacing information and enabling action.
> **Priority #1:** User interaction and ability to get information. Aesthetics serve function, never replace it.

---

## Table of Contents

1. [Design System Foundation](#1-design-system-foundation)
2. [Global Frame & Chrome](#2-global-frame--chrome)
3. [Sidebar Navigation — "The Rail"](#3-sidebar-navigation--the-rail)
4. [Today View — "Mission Control"](#4-today-view--mission-control)
5. [Chat View — "The Terminal"](#5-chat-view--the-terminal)
6. [Agents View — "Fleet Command"](#6-agents-view--fleet-command)
7. [Tasks View — "Operations Board"](#7-tasks-view--operations-board)
8. [Integrations View — "The Switchboard"](#8-integrations-view--the-switchboard)
9. [Mail View — "Signal Inbox"](#9-mail-view--signal-inbox)
10. [Channels View — "Comms Array"](#10-channels-view--comms-array)
11. [Skills View — "The Armory"](#11-skills-view--the-armory)
12. [PawzHub — "The Depot"](#12-pawzhub--the-depot)
13. [Foundry — "The Forge"](#13-foundry--the-forge)
14. [Settings — "System Config"](#14-settings--system-config)
15. [Research View — "Deep Scan"](#15-research-view--deep-scan)
16. [Trading View — "Market Wire"](#16-trading-view--market-wire)
17. [Squads View — "Strike Teams"](#17-squads-view--strike-teams)
18. [Projects View — "Codebase"](#18-projects-view--codebase)
19. [Content Studio — "The Pressroom"](#19-content-studio--the-pressroom)
20. [Orchestrator — "Mission Planner"](#20-orchestrator--mission-planner)
21. [Memory Palace — "The Vault"](#21-memory-palace--the-vault)
22. [Nodes / Engine — "Engine Room"](#22-nodes--engine--engine-room)
23. [Modals & Overlays](#23-modals--overlays)
24. [Notification System — "Signal Feed"](#24-notification-system--signal-feed)
25. [Agent Dock & Mini-Chat — "The Hotline"](#25-agent-dock--mini-chat--the-hotline)
26. [Command Palette — "Quick Strike"](#26-command-palette--quick-strike)
27. [Onboarding / Setup Views](#27-onboarding--setup-views)
28. [Avatar System](#28-avatar-system)
29. [Textures & Visual FX](#29-textures--visual-fx)
30. [Rust Pulse Bridge](#30-rust-pulse-bridge)
31. [Implementation Sequence](#31-implementation-sequence)

---

## 1. Design System Foundation

### 1.1 Color Palette — Token Mapping

The entire app re-skins through CSS custom properties. The Kinetic palette replaces the current magenta/neon system with a monochrome + accent-red language derived from the mood board.

| Token (current) | Current Value (dark) | Kinetic Value (dark) | Notes |
|---|---|---|---|
| `--bg-primary` | `#1e1e1e` | `#050505` | Near-black canvas — "void" base |
| `--bg-secondary` | `#252526` | `#0a0a0a` | Card/panel backgrounds |
| `--bg-sidebar` | `#181818` | `#030303` | Rail background — near-invisible |
| `--bg-hover` | `#2a2d2e` | `#141414` | Hover state — subtle lift |
| `--bg-input` | `#313131` | `#0f0f0f` | Input fields — just perceptible |
| `--bg-overlay` | `rgba(0,0,0,0.7)` | `rgba(0,0,0,0.85)` | Heavier overlay for modals |
| `--bg-tertiary` | `#2d2d2d` | `#111111` | Third-level nesting |
| `--bg-code` | `#1e1e1e` | `#060606` | Code blocks |
| `--accent` | `#ff00ff` | `#FF4D4D` | **Kinetic Red** — primary action |
| `--accent-hover` | `#ff44ff` | `#FF6B6B` | Lighter red on hover |
| `--accent-subtle` | `rgba(255,0,255,0.12)` | `rgba(255,77,77,0.12)` | Subtle red wash |
| `--accent-muted` | `rgba(255,0,255,0.18)` | `rgba(255,77,77,0.18)` | Muted red |
| `--accent-lighter` | `rgba(255,0,255,0.08)` | `rgba(255,77,77,0.08)` | Barely-there red |
| `--accent-strong` | `rgba(255,0,255,0.55)` | `rgba(255,77,77,0.55)` | Strong red |
| `--accent-color` | `#a855f7` | `#FF4D4D` | Secondary accent = same red |
| `--text-primary` | `#cccccc` | `#E8E0D4` | **Kinetic Cream** — warm white |
| `--text-secondary` | `#969696` | `#8A8478` | Muted cream |
| `--text-muted` | `#5a5a5a` | `#4A4540` | Very dim |
| `--text-tertiary` | `#6e6e6e` | `#5A5550` | Between muted and secondary |
| `--border` | `#3c3c3c` | `#1a1a1a` | 1px sharp lines — the grid |
| `--border-subtle` | `#2a2a2a` | `#111111` | Barely visible separator |
| `--border-focus` | `#ff00ff` | `#FF4D4D` | Focus ring = accent red |

**New tokens to add:**

| Token | Value | Purpose |
|---|---|---|
| `--kinetic-sage` | `#8FB0A0` | Secondary accent — success, growth, health indicators |
| `--kinetic-cream` | `#E8E0D4` | Warm text / UI highlights |
| `--kinetic-white` | `#F5F0EB` | High-emphasis text on dark |
| `--kinetic-red` | `#FF4D4D` | Semantic alias for accent |
| `--kinetic-black` | `#050505` | Semantic alias for bg-primary |
| `--grain-opacity` | `0.04` | Film grain overlay intensity |
| `--halftone-opacity` | `0.03` | Halftone dot overlay intensity |
| `--pulse-glow` | `0 0 8px rgba(255,77,77,0.3)` | Rust pulse glow effect |

### 1.2 Light Theme — "Kinetic Light"

| Token | Kinetic Light Value | Notes |
|---|---|---|
| `--bg-primary` | `#F5F0EB` | Warm cream paper |
| `--bg-secondary` | `#EDE8E2` | Card backgrounds |
| `--bg-sidebar` | `#E8E3DD` | Rail |
| `--accent` | `#CC3333` | Darker red for light contrast |
| `--text-primary` | `#1A1815` | Near-black on cream |
| `--border` | `#D4CFC8` | Soft warm border |
| `--kinetic-sage` | `#6D9485` | Darker sage for contrast |

### 1.3 Typography

| Token | Current | Kinetic | Notes |
|---|---|---|---|
| `--font-sans` | System UI stack | Same — system fonts are industrial | No change needed |
| `--font-pixel` | `'Press Start 2P'` | `'Share Tech Mono'` **only** | Drop Press Start 2P. Share Tech Mono alone = clean industrial |
| `--font-mono` | Cascadia/Fira Code stack | Same | Monospace stays |
| `--type-hero` | `2rem` | `1.75rem` | Slightly tighter hero text |
| `--type-label` | `0.6875rem` | `0.625rem` | Smaller labels — dense information |
| `--type-micro` | `0.625rem` | `0.5625rem` | Micro text for meters/badges |

### 1.4 Radius — Going Sharp

| Token | Current | Kinetic | Notes |
|---|---|---|---|
| `--radius-sm` | `6px` | `2px` | Nearly sharp |
| `--radius-md` | `10px` | `3px` | Barely rounded |
| `--radius-lg` | `14px` | `4px` | Subtle softness |
| `--radius-xl` | `18px` | `6px` | For larger containers |
| `--radius-pill` | `999px` | `999px` | Keep pills round (badges, search) |

### 1.5 Shadows — Minimal

| Token | Kinetic Value | Notes |
|---|---|---|
| `--shadow-sm` | `0 1px 2px rgba(0,0,0,0.5)` | Heavier on near-black canvas |
| `--shadow-md` | `0 2px 8px rgba(0,0,0,0.6)` | Floating elements |
| `--shadow-lg` | `0 4px 16px rgba(0,0,0,0.7)` | Modals, drawers |
| `--glow-cyan` | `0 0 8px rgba(143,176,160,0.2)` | Sage glow (replaces cyan) |
| `--glow-magenta` | `0 0 8px rgba(255,77,77,0.25)` | Red glow (replaces magenta) |
| `--glow-green` | `0 0 8px rgba(143,176,160,0.2)` | Sage glow (replaces neon green) |

### 1.6 Semantic Color Consolidation

Reduce the rainbow. The kinetic palette is restrained:

| Semantic | Color | Usage |
|---|---|---|
| **Action / Danger / Active** | `#FF4D4D` (red) | Primary action, errors, alerts, active states |
| **Health / Success / Growth** | `#8FB0A0` (sage) | Success, healthy, connected, growth |
| **Warning** | `#D4A853` (muted gold) | Warnings, cautions |
| **Info / Neutral** | `#7A8B9A` (steel) | Informational, neutral badges |
| **Text** | `#E8E0D4` (cream) | All primary text |

Remove: neon green `#39ff14`, cyan `#00ffff`, hot pink `#ff69b4`, electric blue `#00ccff`. These are replaced by the 4-color semantic system above.

---

## 2. Global Frame & Chrome

### Current State
- Standard sidebar + main content layout
- Sidebar has gradient border (`--sidebar-border-image`)
- CRT scanlines overlay (`--scanline-opacity: 0.08`)
- Cards use `border-radius: 10px` with hover-lift shadows

### Kinetic Transformation

**2.1 Bento Grid Frame**
The entire viewport becomes a CSS Grid with 1px borders — the "bento" layout.

```
┌─────────────────────────────────────────────┐
│  Rail (48px)  │  Main Content Area          │
│               │                             │
│  ◉ Today      │  ┌─────────────────────┐   │
│  ◉ Chat       │  │  View Content       │   │
│  ◉ Agents     │  │  (bento grid of     │   │
│  ─ Work ──    │  │   cards within)     │   │
│  ◉ Tasks      │  │                     │   │
│  ─ Connect ─  │  └─────────────────────┘   │
│  ◉ Integ.     │                             │
│  ◉ Mail       │                             │
│  ◉ Channels   │                             │
│  ─ Space ──   │                             │
│  ◉ Skills     │                             │
│  ◉ PawzHub    │                             │
│  ◉ Foundry    │                             │
│  ─ System ──  │                             │
│  ◉ Settings   │                             │
└─────────────────────────────────────────────┘
```

- **1px `--border` lines** separate every panel, card, and section
- **No border-radius on the outer frame** — the app is a sharp rectangle
- **No gradient sidebar border** — replace with a single 1px `--border` right edge
- **Cards within views** use the bento-internal grid: 1px borders, no gaps, no shadows on most cards
- **Elevated elements** (modals, drawers, command palette) get `--shadow-lg` — the ONLY shadow users

**2.2 Grain Overlay**
An SVG `feTurbulence` filter applied as a `::after` pseudo-element on `#app`:

```css
#app::after {
  content: '';
  position: fixed;
  inset: 0;
  pointer-events: none;
  z-index: 9999;
  opacity: var(--grain-opacity);
  background: url("data:image/svg+xml,..."); /* feTurbulence inline SVG */
  mix-blend-mode: overlay;
}
```

- Replaces the CRT scanlines (`--scanline-opacity: 0` in Kinetic)
- Adds organic, film-like texture to the near-black canvas
- `pointer-events: none` ensures no interaction blocking
- Light theme: `--grain-opacity: 0.02` (barely perceptible)

**2.3 Halftone Pattern (Selective)**
Used sparingly on specific surfaces (Today view hero, Agent cards, section headers):

```css
.halftone-surface::before {
  content: '';
  position: absolute;
  inset: 0;
  background: radial-gradient(circle, var(--kinetic-red) 0.5px, transparent 0.5px);
  background-size: 6px 6px;
  opacity: var(--halftone-opacity);
  pointer-events: none;
}
```

**2.4 Remove**
- CRT scanlines overlay → replaced by grain
- Neon gradient sidebar border → 1px solid border
- `box-shadow` on cards → 1px border grid
- `border-radius: 10px+` → 2-4px max
- Hover-lift animations → border-color change on hover

---

## 3. Sidebar Navigation — "The Rail"

### Current State
- 11 nav items in 4 sections (top, Work, Connect, Workspace, System)
- Section labels as dividers
- Material Symbols icons + text labels
- Active state uses `--accent` highlight + background wash
- PawzHub has a `New` badge

### Kinetic Transformation

**Layout:** Icon-only collapsed rail (48px wide) with text tooltips on hover. Expand to 200px on hover or toggle.

**Collapsed State:**
```
┌──────┐
│  ◉   │ ← Active: 2px left border accent-red, icon filled
│  ○   │ ← Inactive: outline icon, cream color
│  ○   │
│ ──── │ ← Section divider: 1px horizontal rule
│  ○   │
│ ──── │
│  ○   │
│  ○   │
│  ○   │
│ ──── │
│  ○   │
│  ○   │
│  ○   │
│ ──── │
│  ○   │
│      │
│      │
│  ◉   │ ← Bottom: theme toggle (sun/moon)
└──────┘
```

**Active State Indicator:**
- 2px left border in `--kinetic-red`
- Icon color: `--kinetic-red`
- Background: `--accent-lighter` (barely-there red wash)

**Hover State:**
- Rail widens to show labels alongside icons
- Smooth transition: `width var(--transition-smooth)`
- Tooltip fallback if expand is disabled: CSS `::after` tooltip on each item

**Section Dividers:**
- Replace text labels ("Work", "Connect") with 1px horizontal rules
- Dividers are `--border-subtle` (very dim)
- The section grouping is structural, not labeled

**Badge:**
- PawzHub "New" badge: small red dot (6px circle) instead of text badge
- Notification count: red circle with number, top-right of bell icon

**Implementation Notes:**
- Current nav lives in `index.html` lines 38-89
- Active class toggling in `router.ts` (`navHighlightMap`)
- CSS changes: sidebar width, item padding, label visibility, left-border active indicator
- No structural HTML changes needed — hide labels with CSS, show on hover/expanded

---

## 4. Today View — "Mission Control"

### Current State (12 cards)
1. Agent Fleet — agent status grid
2. Usage Today — token count, cost, I/O stats
3. Active Skills — skills count + list
4. Weather — weather card
5. Integrations — health dashboard strip (Phase 6/7 work)
6. Tasks — pending tasks + add button
7. Activity — activity feed
8. 30-Day Activity — heatmap
9. Your Agent Can — capabilities grid
10. Quick Actions — morning briefing, summarize inbox, etc.
11. Unread Emails — email summary
12. Skill Widgets — dynamic skill outputs

### Kinetic Transformation

**Layout:** Full bento grid. 12-column base grid, cards snap to grid cells with 1px borders between them. No gaps, no card shadows, no rounded corners.

```
┌──────────────────────────────────────────────────┐
│  MISSION CONTROL                 Date  │ Weather │
│  Good [morning], [User]                │  18°C   │
├────────────────────┬─────────┬─────────┤─────────┤
│                    │ USAGE   │ ACTIVE  │ UNREAD  │
│  AGENT FLEET       │ 12.4k   │ SKILLS  │ MAIL    │
│  ◉ Aria  [active]  │ tokens  │ 7 of 12 │ 3 msgs  │
│  ○ Max   [idle]    │ $0.04   │         │         │
│  ○ Scout [idle]    │         │         │         │
├────────────────────┴─────────┴─────────┴─────────┤
│  INTEGRATIONS HEALTH                              │
│  ● Slack ● GitHub ● Gmail ○ Notion ○ n8n         │
├─────────────────────────────┬────────────────────┤
│  TASKS (4 pending)          │ QUICK ACTIONS      │
│  □ Review PR #482           │ ▸ Morning Briefing │
│  □ Draft blog post          │ ▸ Summarize Inbox  │
│  □ Update docs              │ ▸ What's on today? │
│  □ Fix auth bug             │                    │
├────────────────────┬────────┴────────────────────┤
│  ACTIVITY          │  30-DAY HEATMAP              │
│  10:32 Aria: ...   │  ▪▪▫▪▪▪▫▪▪▪▫▪▪▫▪▪▪▫▪▪▪...  │
│  10:28 Scout: ...  │                              │
├────────────────────┴─────────────────────────────┤
│  CAPABILITIES              │ SKILL WIDGETS       │
│  🔧 Code  📧 Email  🔍 Web │ [dynamic content]   │
└──────────────────────────────────────────────────┘
```

**Key Changes:**

| Element | Current | Kinetic |
|---|---|---|
| Greeting banner | Rounded card with shadow | Flush top-left cell, no border-radius |
| Card titles | Bold with icon | ALL-CAPS `--type-micro` monospace label, `letter-spacing: 0.1em` |
| Agent Fleet | Grid of avatar circles | Compact status rows: `◉ Name [status]` — one line per agent |
| Usage | Bar chart card | Numeric readout — big number, tiny label. No chart framing |
| Weather | Separate card | Merged into top-right corner of greeting row |
| Integrations strip | Horizontal scroll | Full-width status bar with dot indicators (`●` connected, `○` down) |
| Tasks | List with add modal | Inline editable list, `+` button at bottom, checkbox style: `□` / `■` |
| Activity heatmap | Canvas-based square grid | CSS grid of tiny squares — `4px` cells, sage = active, dim = inactive |
| Quick Actions | Button cards | Text-only links with `▸` prefix, hover: text turns red |
| Capabilities | Icon grid by category | Dense text list, grouped. Icons removed — text is the interface |

**Information Density:**
- Remove all card padding beyond 12px
- Remove all decorative icons from section headers (text labels only)
- Merge Weather into greeting row (no separate card)
- Agent Fleet becomes compact list, not avatar grid
- Every pixel saved is reclaimed for data

**Interaction:**
- Clicking an agent name in Fleet → opens mini-chat with that agent
- Clicking a task → inline edit (no modal)
- Clicking a Quick Action → executes immediately (existing behavior, no change)
- Clicking an integration dot → navigates to integrations view, opens that service's detail panel

---

## 5. Chat View — "The Terminal"

### Current State
- Header: agent avatar, name, model dropdown, session selector, action buttons
- Token meter with context breakdown popover
- Message list with bubbles/cards
- Input area: attach + textarea + mic + send
- Compaction warning banner
- Budget alert banner

### Kinetic Transformation

**The chat is the core. It must feel like a command terminal, not a messaging app.**

**5.1 Header Bar**

```
┌──────────────────────────────────────────────────┐
│ ◉ Aria  │ claude-sonnet │ ▓▓▓▓░░ 62%  │ Session │
│         │   ▾ change    │ 12.4k / 20k │  ▾ ◉ +  │
└──────────────────────────────────────────────────┘
```

- Agent avatar: 24px circle (current sprite), no text label overflow
- Model selector: text-only dropdown, `--type-label` size
- Token meter: inline horizontal bar (40px wide), percentage + raw numbers
- Session: dropdown + new chat button, compact right-aligned
- All in a single 40px-tall row with 1px bottom border
- Remove: session rename/delete/clear/compact as separate buttons → move into session dropdown menu

**5.2 Message Area**

| Element | Current | Kinetic |
|---|---|---|
| User messages | Right-aligned bubble with bg color | Left-aligned, full-width. Prefix: `YOU ›` in `--text-muted` monospace. No bubble. |
| Agent messages | Left-aligned bubble with avatar | Left-aligned, full-width. Prefix: `ARIA ›` in `--kinetic-red` monospace. No bubble. |
| System messages | Centered gray text | Full-width, `--text-muted`, thin 1px top/bottom border |
| Code blocks | Syntax-highlighted box | Same highlighting but `--bg-code` (#060606), 1px border, no radius |
| Tool calls | Expandable accordion | Compact status line: `▸ [tool_name] → result` in `--text-muted`. Expand on click. |
| Thinking/streaming | Pulsing dots or spinner | Red underline pulse animation on agent name prefix while streaming |
| Timestamps | Hidden or on hover | Always visible, right-aligned `--type-micro` monospace |

**5.3 Input Area**

```
┌──────────────────────────────────────────────────┐
│ ┃ Type a message...                    📎 🎤  ⏎ │
└──────────────────────────────────────────────────┘
```

- Single-line appearance, expands to multi-line as user types
- Left border: 2px `--kinetic-red` (the "cursor bar")
- Background: `--bg-secondary`
- Attachment button: subtle icon, no background
- Send button: `--kinetic-red` filled circle with arrow, or just `⏎` icon
- Remove: rounded corner pill shape → sharp rectangle, 1px top border

**5.4 Compaction & Budget Warnings**
- Slim bar (24px height) with 1px top border
- Red text for budget, gold text for compaction
- No rounded card wrapper — just a line of text with colored left-border accent

**5.5 Attachment Preview**
- Thumbnails in a horizontal strip below input
- 1px border, no radius, filename in `--type-micro`

---

## 6. Agents View — "Fleet Command"

### Current State
- Grid of agent cards with avatars, names, bios
- Create/edit modals
- Floating dock tray (bottom-right)
- Mini-chat popup windows

### Kinetic Transformation

**6.1 Fleet Grid**

Replace card grid with a **table-like roster view** as the default, with a grid toggle:

**Roster View (default):**
```
┌──────────────────────────────────────────────────┐
│ FLEET COMMAND                    [Grid] [Roster] │
├──────┬───────────┬──────────┬───────┬────────────┤
│ STAT │ AGENT     │ MODEL    │ TOOLS │ LAST ACTIVE│
├──────┼───────────┼──────────┼───────┼────────────┤
│  ◉   │ Aria      │ sonnet   │ 12    │ 2m ago     │
│  ○   │ Max       │ gpt-4o   │ 8     │ 1h ago     │
│  ○   │ Scout     │ llama3   │ 5     │ 3h ago     │
│  ○   │ Forge     │ deepseek │ 15    │ yesterday  │
├──────┴───────────┴──────────┴───────┴────────────┤
│  [+ New Agent]                                   │
└──────────────────────────────────────────────────┘
```

**Grid View (toggle):**
- Compact cards: 180px wide, avatar (48px), name, model line, status dot
- No bio text on grid cards — density over decoration
- 1px borders, no shadows, no radius
- Halftone overlay on agent avatar area (subtle, `--halftone-opacity`)

**6.2 Agent Editor Modal**
- Full-width slide-in panel from right (not centered modal)
- 4 tabs remain: Basics, Personality, Tools, Advanced
- Tab bar: underline style, `--kinetic-red` underline on active tab
- Avatar picker: sprite grid stays, wrapped in 1px border
- Sliders (Tone/Initiative/Detail): custom range inputs with `--kinetic-red` fill

**6.3 Agent Creator**
- Step-by-step wizard with numbered steps: `01 → 02 → 03 → 04`
- Progress shown as a horizontal line with dots, active dot is red
- Clean inputs, monospace labels

---

## 7. Tasks View — "Operations Board"

### Current State (4 tabs)
- **Board** — Kanban columns (Todo, In Progress, Done)
- **Scheduled** — Cron/automation cards
- **Squads** — Multi-agent teams
- **Projects** — File browser + git

### Kinetic Transformation

**7.1 Tab Bar**
- 4 tabs: Board | Scheduled | Squads | Projects
- Style: text-only tabs with 2px bottom border on active (`--kinetic-red`)
- No background fills, no pills — just text + underline
- Tab text: uppercase `--type-label`, `letter-spacing: 0.08em`

**7.2 Board Tab — Kanban**

```
┌──────────────┬──────────────┬──────────────┐
│ TODO (3)     │ IN PROGRESS  │ DONE (12)    │
│              │ (2)          │              │
├──────────────┼──────────────┼──────────────┤
│ □ Fix auth   │ ■ Review PR  │ ✓ Deploy v4  │
│   @Aria ·2h  │   @Max ·1h   │   @Scout     │
│              │              │              │
│ □ Blog post  │ ■ Docs       │ ✓ Migrate DB │
│   @Scout ·4h │   @Aria ·30m │   @Forge     │
│              │              │              │
│ □ Unit tests │              │ ...          │
│   unassigned │              │              │
└──────────────┴──────────────┴──────────────┘
```

- Columns separated by 1px vertical borders
- Task cards: no radius, no shadow, 1px bottom border between tasks
- Drag handles: thin left-border indicator (red when dragging)
- Agent assignment: `@Name` in `--text-muted`, not avatar circles
- Priority: P1/P2/P3 text prefix instead of color dots
- Add task: `+` at bottom of column, inline text input (no modal)

**7.3 Scheduled Tab**
- Table layout instead of cards
- Columns: Status | Name | Cron Expression | Next Run | Last Result
- Toggle switch for active/paused
- Cron expression in `--font-mono`

**7.4 Squads Tab**
- See [Section 17: Squads View](#17-squads-view--strike-teams)

**7.5 Projects Tab**
- See [Section 18: Projects View](#18-projects-view--codebase)

---

## 8. Integrations View — "The Switchboard"

### Current State (3 tabs)
- **Services** — catalog grid with search, filter pills, sort, detail panel
- **Automations** — automation templates/cards
- **Queries** — predefined queries per service

### Kinetic Transformation

This is the **Matrix Hub** — one of the signature Kinetic surfaces.

**8.1 Layout — "The Matrix"**

Replace the card grid with a **dense status matrix** as the default view:

```
┌──────────────────────────────────────────────────┐
│ THE SWITCHBOARD              [Matrix] [Catalog]  │
│ 🔍 Search services...    Filter: [All ▾]  Sort ▾ │
├──────┬───────┬────────┬───────┬──────┬───────────┤
│ SRVC │ STATE │ HEALTH │ CALLS │ COST │ ACTIONS   │
├──────┼───────┼────────┼───────┼──────┼───────────┤
│ Slack│  ● ON │ ████   │ 142   │$0.00 │ ··· ▸     │
│ GitHb│  ● ON │ ███░   │ 89    │$0.00 │ ··· ▸     │
│ Gmail│  ● ON │ ████   │ 54    │$0.00 │ ··· ▸     │
│ n8n  │  ○ OFF│ ░░░░   │ 0     │$0.00 │ Setup ▸   │
│ Jira │  ○ OFF│ ░░░░   │ 0     │$0.00 │ Setup ▸   │
├──────┴───────┴────────┴───────┴──────┴───────────┤
│ Showing 5 of 127 services                       │
└──────────────────────────────────────────────────┘
```

- **Connected services float to top** (pinned, as current)
- **Health bars:** 4-segment mini bar using CSS (sage = healthy, red = degraded, dim = unknown)
- **Call count:** last 24h API calls
- **Detail panel:** clicking `▸` opens right-side slide-in panel (existing behavior, restyled)

**Catalog View (toggle):**
- For users browsing unconnected services
- Larger cards with service icons, description, category tag
- Cards in bento grid (1px borders, no gaps, no radius)
- "Connect" button: outline style, turns filled red on hover

**8.2 Detail Panel**
- 400px right slide-in (existing)
- Sections: Connection Info, Health Card, Recent Actions, Setup Guide, Quick Actions
- All sections use 1px horizontal borders between them
- Setup guide: numbered steps with `--kinetic-red` step numbers

**8.3 Automations Tab**
- Table view: Status | Name | Trigger | Last Fired | Success Rate
- Active automations get sage `●`, paused get dim `○`
- Click to expand → show recent execution log inline

**8.4 Queries Tab**
- List of services with expandable query sets
- Each query: text label + `▸ Run` button
- Click → opens chat with query pre-filled (existing behavior)

---

## 9. Mail View — "Signal Inbox"

### Current State
- Account list sidebar
- Email list (sender, subject, date)
- Email detail pane
- Compose modal

### Kinetic Transformation

**Three-panel layout with 1px borders:**

```
┌────────────┬──────────────────┬──────────────────┐
│ ACCOUNTS   │ INBOX            │ MESSAGE           │
│            │                  │                   │
│ ● Work     │ John D. · 10:32 │ From: john@...    │
│ ○ Personal │ Re: Project upd  │ Date: Today 10:32 │
│            │                  │                   │
│            │ Sarah K. · 09:15 │ Hey, here's the   │
│            │ Sprint review    │ update on the...  │
│            │                  │                   │
│            │ Bot · 08:00      │                   │
│            │ Daily digest     │                   │
├────────────┴──────────────────┴──────────────────┤
│ [Compose]                                        │
└──────────────────────────────────────────────────┘
```

- Account list: 160px sidebar, account name + status dot + unread count
- Email list: sender name (bold) + time right-aligned, subject below, 1px bottom border between emails
- Unread emails: left 2px red border
- Selected email: `--bg-hover` background
- Detail pane: clean header (from, date, subject), body below
- Compose: full-width bottom panel slide-up (not modal)

---

## 10. Channels View — "Comms Array"

### Current State
- Channel platform cards (Discord, Slack, IRC, Matrix, etc.)
- Config forms per channel
- Status indicators

### Kinetic Transformation

**Status matrix layout:**

```
┌──────────────────────────────────────────────────┐
│ COMMS ARRAY                                      │
├──────────┬────────┬──────────┬───────────────────┤
│ CHANNEL  │ STATUS │ ACCOUNTS │ ACTIONS           │
├──────────┼────────┼──────────┼───────────────────┤
│ Discord  │  ● ON  │ 2 guilds │ Configure ▸       │
│ Slack    │  ○ OFF │ —        │ Connect ▸         │
│ Telegram │  ● ON  │ 1 bot    │ Configure ▸       │
│ Matrix   │  ○ OFF │ —        │ Connect ▸         │
│ IRC      │  ○ OFF │ —        │ Connect ▸         │
│ Twitch   │  ○ OFF │ —        │ Connect ▸         │
└──────────┴────────┴──────────┴───────────────────┘
```

- Click "Configure" → expands inline (accordion style) showing the channel config form
- Config forms: sharp inputs, monospace labels, `--kinetic-red` save button
- Inline expansion is better than navigating away — user stays in context
- Connected channels: sage status dot, config expandable
- Disconnected: dim dot, "Connect" text link

---

## 11. Skills View — "The Armory"

### Current State (5 tabs)
- **Active** — list of active skills
- **Integrations** — skills with credentials
- **Tools** — tool groups
- **Extensions** — MCP extensions
- **Create** — skill creation

### Kinetic Transformation

**5 tabs remain with underline active indicator.**

**11.1 Active Tab**

```
┌──────────────────────────────────────────────────┐
│ THE ARMORY                                       │
│ Active │ Integrations │ Tools │ Extensions │ + ─ │
├──────┬───────────┬──────────┬──────────┬─────────┤
│  ✓   │ Web Search│ Built-in │ All      │ ▸      │
│  ✓   │ Code Exec │ Built-in │ All      │ ▸      │
│  ✓   │ File I/O  │ Built-in │ Aria,Max │ ▸      │
│  ✓   │ Slack Bot │ Community│ Aria     │ ▸      │
│  ○   │ Trading   │ Custom   │ None     │ Enable │
└──────┴───────────┴──────────┴──────────┴─────────┘
```

- Table layout: Enabled toggle | Name | Source | Assigned Agents | Expand
- Expanding shows skill config, description, credential fields
- "Create" tab (last tab): renamed to `+` icon, opens skill creation form

**11.2 Integrations Tab**
- Shows skills requiring credentials (API keys)
- Each skill: name, credential status (set/missing), last validated
- Missing credentials: red indicator, "Set Key" action

**11.3 Tools Tab**
- Tool group toggles (bulk enable/disable)
- Compact checkbox list

**11.4 Extensions Tab**
- MCP server list
- Status: running/stopped/error
- One-click start/stop

---

## 12. PawzHub — "The Depot"

### Current State
- Marketplace grid of community skills
- Categories, featured/popular sections
- Install/uninstall buttons

### Kinetic Transformation

```
┌──────────────────────────────────────────────────┐
│ THE DEPOT                                        │
│ 🔍 Search skills...         [Featured] [All] ─── │
├──────────────────────────────────────────────────┤
│ FEATURED                                         │
├──────────┬──────────┬──────────┬─────────────────┤
│ Skill A  │ Skill B  │ Skill C  │ Skill D         │
│ ★ 4.8    │ ★ 4.6    │ ★ 4.5   │ ★ 4.3           │
│ @author  │ @author  │ @author │ @author          │
│ [Install]│[Installed]│[Install]│ [Install]        │
├──────────┴──────────┴──────────┴─────────────────┤
│ CATEGORIES                                       │
│ Communication · Developer · Productivity · AI ···│
├──────────────────────────────────────────────────┤
│ [Paginated skill list...]                        │
└──────────────────────────────────────────────────┘
```

- Bento grid for featured (4 across)
- Skill cards: name, rating, author, install button
- "Installed" state: sage outline button, checkmark
- "Install" state: red outline button
- Category filter: horizontal scroll pills (keep pills round — `--radius-pill`)
- Click skill → expand inline with full description, screenshots, reviews

---

## 13. Foundry — "The Forge"

### Current State (2 tabs)
- **Models** — model browser, installed/available
- **Chat Modes** — named agent configurations

### Kinetic Transformation

**13.1 Models Tab**

```
┌──────────────────────────────────────────────────┐
│ THE FORGE                                        │
│ Models │ Modes                                   │
├──────┬──────────────┬────────┬───────┬───────────┤
│ PROV │ MODEL        │ SIZE   │ STATE │ ACTION    │
├──────┼──────────────┼────────┼───────┼───────────┤
│ Anth │ claude-sonnet│ —      │ API   │ Default ▸ │
│ OAI  │ gpt-4o       │ —      │ API   │ Config ▸  │
│ Olla │ llama3:8b    │ 4.7GB  │ Local │ Running ▸ │
│ Olla │ codellama    │ 3.8GB  │ Local │ Pull ▸    │
│ Deep │ deepseek-v3  │ —      │ API   │ Config ▸  │
└──────┴──────────────┴────────┴───────┴───────────┘
```

- Table layout with provider prefix
- Local models show size and pull/run status
- API models show config access
- Default model: highlighted row with red left-border

**13.2 Modes Tab**
- List of chat mode configurations
- Each mode: name, model, system prompt preview, skill count
- Click to edit inline
- "Create Mode" button at top
- Active mode: indicated with `●` prefix

---

## 14. Settings — "System Config"

### Current State (15 tabs)
general, models, agent-defaults, sessions, voice, browser, tailscale, webhook, n8n, mcp, security, memory, engine, logs

### Kinetic Transformation

**14.1 Tab Navigation — Vertical Sidebar**

Replace horizontal tab bar with a vertical settings sidebar (200px left panel):

```
┌──────────────┬───────────────────────────────────┐
│ SYSTEM       │                                   │
│ CONFIG       │  GENERAL                          │
│              │                                   │
│ ▸ General    │  Theme         [Dark ▾]           │
│   Models     │  Language      [English ▾]        │
│   Agents     │  Startup View  [Today ▾]          │
│   Sessions   │  Updates       [Auto ▾]           │
│              │                                   │
│ ── Voice ──  │  APPEARANCE                       │
│   Voice      │  Grain Effect  [■ On]             │
│   Browser    │  Halftone      [□ Off]            │
│              │  Compact Mode  [□ Off]            │
│ ── Network ─ │                                   │
│   Tailscale  │                                   │
│   Webhooks   │                                   │
│   n8n        │                                   │
│   MCP        │                                   │
│              │                                   │
│ ── System ── │                                   │
│   Security   │                                   │
│   Memory     │                                   │
│   Engine     │                                   │
│   Logs       │                                   │
└──────────────┴───────────────────────────────────┘
```

- Group tabs into sections: Core, Voice, Network, System
- Active tab: red left-border + `--bg-hover` background
- Content area: form groups with 1px horizontal borders between sections
- Form inputs: sharp rectangles, 1px border, `--bg-input` background
- Toggles: custom toggle switches with red active state
- Dropdowns: custom select with no native chrome

**14.2 New Settings: Kinetic Appearance**

Add a section under General for Kinetic-specific appearance controls:
- **Grain overlay** toggle (on/off)
- **Halftone accents** toggle (on/off)
- **Compact mode** toggle (removes padding, tightens spacing)
- **Pulse animations** toggle (Rust pulse bridge effects on/off)
- **Rail mode**: Collapsed (icons) / Expanded (icons + text) / Auto (hover expand)

**14.3 Memory Tab**
- Embeds Memory Palace view (see [Section 21](#21-memory-palace--the-vault))

**14.4 Engine Tab**
- Embeds Nodes/Engine view (see [Section 22](#22-nodes--engine--engine-room))

**14.5 Logs Tab**
- Monospace log viewer, `--bg-code` background
- Level filters: ERROR (red), WARN (gold), INFO (sage), DEBUG (dim)
- Auto-scroll toggle
- Search/filter input at top

---

## 15. Research View — "Deep Scan"

### Current State
- AI research workspace
- Findings, sources, live stream
- Multiple research modes
- Currently routed to chat-view (shared)

### Kinetic Transformation

**Research is a specialized chat mode. It stays within the Chat view but with a distinctive header:**

```
┌──────────────────────────────────────────────────┐
│ DEEP SCAN MODE              │ Sources: 12       │
│ ◉ Aria · claude-sonnet      │ Findings: 3       │
├──────────────────────────────┴──────────────────┤
│                                                  │
│ ARIA › Analyzing source [3/12]...                │
│         ████████░░ 67%                           │
│                                                  │
│ ARIA › Finding: The React team announced...      │
│   SOURCE: https://react.dev/blog (★ high)        │
│                                                  │
│ YOU › Compare this with Vue's approach           │
│                                                  │
├──────────────────────────────────────────────────┤
│ ┃ Research query...                    📎  ⏎    │
└──────────────────────────────────────────────────┘
```

- Header shows source count + findings count as live counters
- Research findings: formatted as structured blocks with source citations
- Progress: inline progress bar during active research
- Sources panel: could be toggled as a right sidebar panel (future enhancement)

---

## 16. Trading View — "Market Wire"

### Current State
- Coinbase integration
- Trade records, positions, P&L
- Trading policies
- Routed to today-view (shared)

### Kinetic Transformation

**Dedicated ticker-tape aesthetic:**

```
┌──────────────────────────────────────────────────┐
│ MARKET WIRE                          P&L: +$142  │
├──────────────────────────────────────────────────┤
│ BTC $67,840 ▲2.1% │ ETH $3,920 ▼0.4% │ SOL ... │
├──────────┬────────┬────────┬─────────┬───────────┤
│ ASSET    │ QTY    │ VALUE  │ P&L     │ ACTION    │
├──────────┼────────┼────────┼─────────┼───────────┤
│ BTC      │ 0.015  │ $1,017 │ +$42    │ ▸         │
│ ETH      │ 2.5    │ $9,800 │ +$100   │ ▸         │
├──────────┴────────┴────────┴─────────┴───────────┤
│ RECENT TRADES                                    │
│ · Buy 0.01 BTC @ $67,200  · 2h ago             │
│ · Sell 0.5 ETH @ $3,940   · 5h ago             │
├──────────────────────────────────────────────────┤
│ POLICIES                                         │
│ Max position: $5,000  Daily limit: $1,000        │
└──────────────────────────────────────────────────┘
```

- Top ticker strip: scrolling or static price bar
- Positions table: standard table layout
- P&L uses salmon-red for gains (positive = accent color), sage for neutral
- Trade history: compact log format
- Policy section: key-value pairs, editable

---

## 17. Squads View — "Strike Teams"

### Current State
- Multi-agent team cards
- Detail view with handoff visualization
- Member management modals

### Kinetic Transformation

**Embedded within Tasks → Squads tab.**

```
┌──────────────────────────────────────────────────┐
│ STRIKE TEAMS                        [+ New Team] │
├──────────┬───────────┬──────────┬────────────────┤
│ TEAM     │ MEMBERS   │ STATUS   │ MISSION        │
├──────────┼───────────┼──────────┼────────────────┤
│ Alpha    │ Aria, Max │ Active   │ Code review    │
│ Bravo    │ Scout     │ Idle     │ Research       │
│ Delta    │ Forge,Aria│ Active   │ Documentation  │
├──────────┴───────────┴──────────┴────────────────┤
│ ▸ Click to expand team details & handoff log     │
└──────────────────────────────────────────────────┘
```

- Click team row → expands inline with:
  - Member list (name, role, current task)
  - Handoff log (who passed what to whom)
  - Mission description (editable)
  - Add/remove member buttons

---

## 18. Projects View — "Codebase"

### Current State
- File tree browser
- File viewer
- Git integration (branch info, status)

### Kinetic Transformation

**Embedded within Tasks → Projects tab.**

```
┌────────────────────┬─────────────────────────────┐
│ CODEBASE           │ src/main.ts                 │
│                    │                             │
│ ▾ src/             │ 1  import { invoke } from   │
│   ▾ components/    │ 2  import { initRouter }    │
│     atoms.ts       │ 3                           │
│     molecules.ts   │ 4  async function main() {  │
│   ▸ views/         │ 5    await invoke('init');   │
│   main.ts ←        │ 6  }                        │
│   styles.css       │                             │
├────────────────────│                             │
│ BRANCH: main       │                             │
│ STATUS: 3 modified │                             │
└────────────────────┴─────────────────────────────┘
```

- Two-panel: file tree (200px left) + file viewer (rest)
- File tree: monospace, tree lines (`├── ▾ ▸`), no icons
- Active file: red left-border indicator
- Git status: bottom bar in file tree panel
- Modified files: dot indicator next to filename
- File viewer: syntax highlighted, line numbers in `--text-muted`

---

## 19. Content Studio — "The Pressroom"

### Current State
- Document list sidebar
- Text editor with toolbar
- Word count

### Kinetic Transformation

```
┌────────────────────┬─────────────────────────────┐
│ DOCUMENTS          │ ┌─────────────────────────┐ │
│                    │ │ B I U S  │ H1 H2  │ ··· │ │
│ · Blog post draft  │ ├─────────────────────────┤ │
│ · Release notes    │ │                         │ │
│ · Meeting notes ←  │ │ The quick brown fox     │ │
│                    │ │ jumped over the lazy     │ │
│ [+ New]            │ │ dog. This is the body   │ │
│                    │ │ of the document...      │ │
│                    │ │                         │ │
│                    │ ├─────────────────────────┤ │
│                    │ │ 342 words · 5 min read  │ │
└────────────────────┴─┴─────────────────────────┴─┘
```

- Document list: left sidebar, selected doc has red left-border
- Toolbar: minimal, icon-only, 1px bottom border
- Editor area: clean `--bg-primary`, `--text-primary` (cream on black)
- Word count bar: bottom, `--type-micro`, `--text-muted`
- Writing surface feels like a dark-mode typewriter

---

## 20. Orchestrator — "Mission Planner"

### Current State
- Multi-agent project management
- Agent messages stream
- Task assignments per specialty

### Kinetic Transformation

```
┌──────────────────────────────────────────────────┐
│ MISSION PLANNER               Status: 2 Active  │
├─────────────────┬────────────────────────────────┤
│ AGENTS          │ MISSION FEED                   │
│                 │                                │
│ ◉ Aria          │ 10:32 Aria: Completed code     │
│   └─ Code review│         review for PR #482     │
│                 │                                │
│ ◉ Max           │ 10:28 Max: Starting API tests  │
│   └─ Testing    │                                │
│                 │ 10:25 Aria→Max: Handoff — code  │
│ ○ Scout         │         is ready for testing   │
│   └─ Standby    │                                │
│                 │ 10:20 [System] Mission started  │
├─────────────────┴────────────────────────────────┤
│ ┃ Direct agents...                          ⏎   │
└──────────────────────────────────────────────────┘
```

- Left panel: agent roster with current task
- Right panel: chronological mission feed
- Handoff events: `→` indicator between agent names
- System events: `[System]` prefix in `--text-muted`
- Input at bottom: direct instructions to the orchestrator

---

## 21. Memory Palace — "The Vault"

### Current State
- Vector memory CRUD
- Recall cards (closest matches)
- Provider config (OpenAI/Azure)
- Graph visualization (knowledge graph)

### Kinetic Transformation

**Embedded in Settings → Memory tab.**

```
┌──────────────────────────────────────────────────┐
│ THE VAULT                     Memories: 1,247    │
│ 🔍 Search memories...              Provider: OAI │
├──────────────────────────────────────────────────┤
│ RECENT                                           │
├──────┬──────────────────────────┬────────────────┤
│ DIST │ CONTENT                  │ DATE           │
├──────┼──────────────────────────┼────────────────┤
│ 0.92 │ User prefers TypeScript  │ 2h ago         │
│ 0.87 │ Project uses Tauri v2    │ 1d ago         │
│ 0.81 │ Atomic design pattern    │ 3d ago         │
├──────┴──────────────────────────┴────────────────┤
│ [+ Store Memory]  [Graph View]  [Configure]      │
└──────────────────────────────────────────────────┘
```

- Table layout for memory entries
- Distance score shown as numeric (similarity)
- Graph view: toggle to knowledge graph visualization
- Graph nodes: circles with 1px borders, edges as lines, red for recent, dim for old
- Configure: provider settings (API key, model, dimensions)

---

## 22. Nodes / Engine — "Engine Room"

### Current State
- Node status, config
- Skills list with credential info
- Engine health

### Kinetic Transformation

**Embedded in Settings → Engine tab.**

```
┌──────────────────────────────────────────────────┐
│ ENGINE ROOM                    Status: RUNNING   │
├──────────────────────────────────────────────────┤
│ NODE HEALTH                                      │
│ ● Engine    Online · v2.1.0 · PID 4821          │
│ ● Memory    Online · 1,247 vectors              │
│ ○ Tailscale Disconnected                         │
├──────────────────────────────────────────────────┤
│ CONFIGURATION                                    │
│ Engine Path    /usr/local/bin/engine              │
│ Port           3000                               │
│ Auto-start     [■ On]                            │
│ Debug Mode     [□ Off]                           │
├──────────────────────────────────────────────────┤
│ SKILLS WITH CREDENTIALS                          │
│ ✓ openai_key      Set · Valid                    │
│ ✓ github_token    Set · Valid                    │
│ ✗ slack_token     Missing                        │
└──────────────────────────────────────────────────┘
```

- Status rows: dot + name + status text, all inline
- Config: key-value pairs, editable
- Credentials: checkmark/cross + key name + status

---

## 23. Modals & Overlays

### Design Language

All modals in the Kinetic system follow the same pattern:

**Structure:**
```
┌──────────────────────────────────────────────────┐
│░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░┌────────────────────────────────────┐░░░░░░░░│
│░░░░│ MODAL TITLE                    ✕   │░░░░░░░░│
│░░░░├────────────────────────────────────┤░░░░░░░░│
│░░░░│                                    │░░░░░░░░│
│░░░░│  Content area                      │░░░░░░░░│
│░░░░│                                    │░░░░░░░░│
│░░░░├────────────────────────────────────┤░░░░░░░░│
│░░░░│       [Cancel]  [Confirm]          │░░░░░░░░│
│░░░░└────────────────────────────────────┘░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│
└──────────────────────────────────────────────────┘
```

**Rules:**
- Sharp corners (`--radius-sm: 2px` max)
- 1px `--border` outline
- `--shadow-lg` (the only place shadows are used liberally)
- Backdrop: `--bg-overlay` (`rgba(0,0,0,0.85)`)
- Title: uppercase `--type-label`, `letter-spacing: 0.08em`
- Close button: `✕` text, no circle background
- Footer: right-aligned buttons, 1px top border separator
- Cancel: ghost button (text only, `--text-secondary`)
- Confirm/Action: `--kinetic-red` filled button, minimal padding

**Per-Modal Specifics:**

| Modal | Changes |
|---|---|
| **Confirm Modal** | Destructive actions: red text warning. Normal: cream text. |
| **Credential Prompt** | Input fields with eye-toggle for visibility. "Test Connection" button: sage when successful. |
| **HIL (Human-in-Loop)** | Risk classification badge: HIGH = red bg, MEDIUM = gold bg, LOW = sage bg. Tool name in monospace. |
| **Agent Editor** | Slide-in panel from right (not centered modal). Full height. |
| **Agent Creator** | Centered modal with step indicator at top. |
| **Squad Modals** | Standard centered modal pattern. |
| **Task Add** | Inline form at bottom of column (not modal). |
| **Compose Email** | Full-width slide-up panel from bottom. |

---

## 24. Notification System — "Signal Feed"

### Current State
- Bell icon with count badge
- Slide-in drawer from right
- Notification cards: icon + title + message + time
- Mark read / mark all read / clear all

### Kinetic Transformation

**24.1 Bell Icon**
- Badge: 6px red circle with count (tiny `--type-micro` text)
- No badge background blob — just the number and a dot

**24.2 Notification Drawer**

```
┌──────────────────────────┐
│ SIGNALS           Clear  │
├──────────────────────────┤
│ ● 10:32                  │
│ Aria completed task       │
│ "Fix auth bug"           │
├──────────────────────────┤
│ ● 10:28                  │
│ New email from John D.   │
│ "Sprint review update"   │
├──────────────────────────┤
│ ○ 09:15 (read)           │
│ Scout: Research complete │
├──────────────────────────┤
│ Show all (12)            │
└──────────────────────────┘
```

- 320px right drawer
- 1px left border (no shadow on drawer — bento style)
- Each notification: 1px bottom border, no card wrapping
- Unread: `●` red dot + bold title
- Read: `○` dim dot + normal weight
- Timestamp: right-aligned `--type-micro`
- Click notification → navigates to relevant view
- Kind icon mapping: removed — text content is enough, dots indicate read state

**24.3 Toast Notifications**
- Slide in from top-right
- 1px border, `--bg-secondary` background
- Left 2px border indicates type: red (error), sage (success), gold (warning), steel (info)
- No icon — the border color conveys severity
- Auto-dismiss: 4 seconds for info/success, persistent for errors

---

## 25. Agent Dock & Mini-Chat — "The Hotline"

### Current State
- Floating dock tray: bottom-right avatar circles (FB Messenger style)
- Mini-chat popup windows per agent with streaming

### Kinetic Transformation

**25.1 Agent Dock**
- Keep bottom-right position
- Avatars: 32px circles (down from 40px if current)
- Active agent: red ring border
- Idle agent: dim border
- Hover: show agent name as tooltip
- Max visible: 5 (scroll/expand for more)
- 1px border circle outlines, no shadow

**25.2 Mini-Chat Popup**

```
┌────────────────────────────┐
│ ◉ Aria          ─  □  ✕   │
├────────────────────────────┤
│ ARIA › Hello! How can I    │
│ help?                      │
│                            │
│ YOU › Check my emails      │
│                            │
│ ARIA › Checking inbox...   │
│ ████░░░░ 45%               │
├────────────────────────────┤
│ ┃ Message...          ⏎   │
└────────────────────────────┘
```

- Same message format as main Terminal (no bubbles, prefix labels)
- Compact: narrower (300px), shorter max-height (400px)
- Window controls: minimize (─), maximize to main chat (□), close (✕)
- Title bar: agent avatar (16px) + name + controls
- 1px border, `--shadow-md` (floating panel)
- Minimized state: collapses to just the dock avatar with unread badge

---

## 26. Command Palette — "Quick Strike"

### Current State
- Fuzzy-search overlay (`Ctrl+K` style)
- Agent/skill/view items

### Kinetic Transformation

```
┌──────────────────────────────────────────────────┐
│░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░┌──────────────────────────────┐░░░░░░░░░░░░│
│░░░░░░│ 🔍 Quick Strike...           │░░░░░░░░░░░░│
│░░░░░░├──────────────────────────────┤░░░░░░░░░░░░│
│░░░░░░│ ▸ Switch to Today            │░░░░░░░░░░░░│
│░░░░░░│ ▸ Chat with Aria             │░░░░░░░░░░░░│
│░░░░░░│ ▸ Run Web Search skill       │░░░░░░░░░░░░│
│░░░░░░│ ▸ Open Settings              │░░░░░░░░░░░░│
│░░░░░░│ ▸ New Agent                  │░░░░░░░░░░░░│
│░░░░░░└──────────────────────────────┘░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│
└──────────────────────────────────────────────────┘
```

- Centered overlay, 600px wide
- Sharp rectangle, 1px border, `--shadow-lg`
- Input: monospace, no border on input itself (the container IS the input)
- Results: text-only list items, `▸` prefix, hover: red text
- Category headers: uppercase `--type-micro` dim labels (`VIEWS`, `AGENTS`, `SKILLS`)
- Keyboard navigation: up/down, enter to execute, esc to close
- Active result: `--bg-hover` background

---

## 27. Onboarding / Setup Views

### Current State
- `setup-view` — welcome screen
- `install-view` — installation wizard (Node.js, engine)
- `manual-setup-view` — manual engine configuration

### Kinetic Transformation

**27.1 Welcome Screen**
```
┌──────────────────────────────────────────────────┐
│                                                  │
│              ⚡🐾                                │
│                                                  │
│           OPEN PAWZ                              │
│     Kinetic Intelligence Engine                  │
│                                                  │
│         [Get Started →]                          │
│                                                  │
│                    v4.6.0                        │
└──────────────────────────────────────────────────┘
```

- Full-screen `--bg-primary` (void black)
- Lightning paw logo centered (the new primary mark)
- App name in `--font-mono` (Share Tech Mono), cream color
- Subtitle: "Kinetic Intelligence Engine" in `--text-muted`
- Single CTA button: `--kinetic-red` filled
- Grain overlay active on this screen
- Version number: bottom-center, `--type-micro`

**27.2 Installation Wizard**
- Step indicator: `01 ─── 02 ─── 03 ─── 04`
- Active step: red number, line fills red up to current step
- Content: clean form areas for each step
- Progress feels mechanical and precise

**27.3 Tour Overlay**
- Spotlight: dark overlay with circular cutout around target element
- Tooltip: sharp rectangle, 1px red left-border, step counter `1/5`
- Next/Skip buttons: ghost text buttons

---

## 28. Avatar System

### Current State
- Sprite-based avatars (grid selector in agent editor)
- `--avatar-path: '/src/assets/avatars/'`
- Fixed set of character sprites

### Kinetic Transformation

**New avatars are in progress (from user).** The system must support:

**28.1 Avatar Frame**
- Avatar circles: 1px border (cream `--kinetic-cream`)
- Active/speaking: 2px `--kinetic-red` border with subtle pulse animation
- Idle: 1px `--border` (dim)
- No shadow on avatars — flat circles

**28.2 Avatar Sizes**
| Context | Size | Notes |
|---|---|---|
| Agent card (grid view) | 48px | Roster/grid |
| Agent card (roster view) | 24px | Inline with text |
| Chat header | 24px | Next to agent name |
| Mini-chat title | 16px | Compact |
| Agent dock | 32px | Floating dock tray |
| Agent editor | 96px | Large preview |
| Today fleet | 32px | Fleet roster |
| Message prefix | none | Text prefix only (no avatar in messages) |

**28.3 Avatar Picker (Editor)**
- Grid of available sprites
- 1px border on each option
- Selected: red border
- Hover: cream border
- Future: accept custom image uploads (file input)

---

## 29. Textures & Visual FX

### 29.1 Grain Overlay
- SVG `feTurbulence` → inline data URI
- Applied to `#app::after`
- `opacity: var(--grain-opacity)` (default 0.04, light theme 0.02)
- `mix-blend-mode: overlay`
- `pointer-events: none`
- User toggle in Settings → General → Appearance

### 29.2 Halftone Pattern
- CSS `radial-gradient` dots
- Applied selectively via `.halftone-surface` class:
  - Today view greeting area
  - Agent card hover state (subtle background)
  - Section headers on specific views
- `opacity: var(--halftone-opacity)` (default 0.03)
- User toggle in Settings → General → Appearance

### 29.3 Pulse Animation
- Subtle red glow pulse on active elements:
  - Active agent avatars (dock, fleet)
  - Streaming indicator (agent name prefix in chat)
  - Active integration dots
- CSS: `@keyframes kinetic-pulse { 0%,100% { box-shadow: 0 0 0 rgba(255,77,77,0) } 50% { box-shadow: var(--pulse-glow) } }`
- Duration: `2s ease-in-out infinite`
- User toggle in Settings → General → Appearance

### 29.4 Removed Effects
- ❌ CRT scanlines — replaced by grain
- ❌ Neon glow shadows (`--glow-cyan`, `--glow-magenta` in neon form) — replaced by subtle sage/red
- ❌ Gradient sidebar border — replaced by 1px solid
- ❌ Hover-lift card animations — replaced by border-color changes
- ❌ Box shadows on cards — replaced by border grid

---

## 30. Rust Pulse Bridge

### Architecture

The existing `EngineEvent` system (Rust → frontend deltas, tool calls, completions) already provides a pipeline for real-time UI updates. The Kinetic "Rust Pulse Bridge" extends this for visual heartbeat effects.

**30.1 Events That Trigger Visual Pulses**

| Rust Event | Frontend Effect |
|---|---|
| `engine_started` | Engine status dot goes sage with 1-second pulse |
| `stream_delta` | Agent name prefix in chat gets red underline pulse |
| `tool_call_start` | Tool name in action receipt gets red text flash |
| `tool_call_complete` | Tool name goes sage (success) or red (error) |
| `memory_stored` | Memory count in vault gets brief sage flash |
| `integration_health_update` | Relevant service dot pulses |
| `task_completed` | Task checkbox gets sage checkmark animation |
| `error` | Global toast with red left-border |

**30.2 Implementation**

No new Rust code needed. The existing `EngineEvent` listener in `bridge.ts` already captures these events. The Kinetic work adds CSS classes that are toggled via JavaScript event handlers:

```typescript
// In bridge.ts — existing event handler
case 'stream_delta':
  // existing: append content to chat
  // kinetic: add pulse class to agent prefix
  agentPrefixEl.classList.add('kinetic-pulse-active');
  setTimeout(() => agentPrefixEl.classList.remove('kinetic-pulse-active'), 300);
  break;
```

```css
.kinetic-pulse-active {
  animation: kinetic-pulse 0.3s ease-out;
}

@keyframes kinetic-pulse {
  0% { text-shadow: 0 0 0 transparent; }
  50% { text-shadow: 0 0 6px var(--kinetic-red); }
  100% { text-shadow: 0 0 0 transparent; }
}
```

**30.3 Performance**
- CSS animations only (GPU-accelerated)
- No continuous timers — event-driven
- Pulse classes auto-remove after animation duration
- Disable-able via Settings toggle

---

## 31. Implementation Sequence

### Phase K-0: Design System Foundation (Token Swap)
**Files:** `src/styles.css`
**Scope:** Change all CSS custom property values in `:root` and `[data-theme='light']`
**Risk:** Low — pure CSS token swap, no structural changes
**Estimated lines:** ~200 changed
**Commit:** `feat: kinetic design tokens — palette, typography, radius, shadows`

### Phase K-1: Global Frame & Textures
**Files:** `src/styles.css`, `index.html` (minimal)
**Scope:**
- Grain overlay pseudo-element
- Remove CRT scanlines
- Remove gradient sidebar border → 1px solid
- Reduce border-radius globally
- Remove card box-shadows → border-based cards
**Risk:** Low-Medium — visual-only, but wide-reaching
**Commit:** `feat: kinetic frame — grain overlay, sharp borders, bento cards`

### Phase K-2: Sidebar Rail
**Files:** `src/styles.css`, `index.html` (nav structure), `src/views/router.ts` (active state)
**Scope:**
- Collapse sidebar to icon-only 48px rail
- Hover-expand behavior
- Active state: left-border accent
- Remove section text labels → horizontal rules
**Risk:** Medium — affects primary navigation UX
**Commit:** `feat: kinetic rail — collapsed sidebar, hover expand`

### Phase K-3: Chat Terminal
**Files:** `src/styles.css`, `src/views/chat/` (message rendering)
**Scope:**
- Remove message bubbles → prefix labels (YOU ›, AGENT ›)
- Input area: left red border bar
- Header: compact single row
- Tool calls: inline status lines
**Risk:** Medium — changes core chat interaction appearance
**Commit:** `feat: kinetic terminal — chat view transformation`

### Phase K-4: Today / Mission Control
**Files:** `src/styles.css`, `src/views/today/molecules.ts`
**Scope:**
- Bento grid layout for Today cards
- Compact agent fleet
- Merge weather into greeting
- Dense card styling
**Risk:** Medium — layout restructuring
**Commit:** `feat: kinetic mission control — today view bento grid`

### Phase K-5: Table Views (Agents, Tasks, Integrations, Skills, Channels)
**Files:** `src/styles.css`, view-specific CSS for each
**Scope:**
- Convert card grids → table/roster layouts
- Matrix switchboard for integrations
- Inline expand on click (replaces some modals)
**Risk:** Medium-High — multiple views, interaction pattern changes
**Commit (split into sub-phases):**
- `K-5a`: Agents → Fleet Command roster
- `K-5b`: Tasks → Operations Board table
- `K-5c`: Integrations → Switchboard matrix
- `K-5d`: Skills → Armory table
- `K-5e`: Channels → Comms Array table

### Phase K-6: Modal & Overlay System
**Files:** `src/styles.css`, modal components
**Scope:**
- Sharp modal styling
- Heavier backdrop
- Consistent button patterns
- Slide-in panel for agent editor
**Risk:** Low — mostly CSS
**Commit:** `feat: kinetic modals — sharp overlays, slide-in panels`

### Phase K-7: Settings Vertical Sidebar
**Files:** `src/styles.css`, `src/views/settings-tabs.ts`, `index.html`
**Scope:**
- Convert horizontal tabs → vertical sidebar
- Group settings into sections
- Add Kinetic appearance controls
**Risk:** Medium — layout restructuring
**Commit:** `feat: kinetic settings — vertical sidebar, appearance controls`

### Phase K-8: Notifications, Command Palette, Toast
**Files:** `src/styles.css`, notification/command-palette components
**Scope:**
- Notification drawer restyling
- Command palette sharp styling
- Toast left-border type indicators
**Risk:** Low — mostly CSS
**Commit:** `feat: kinetic signals — notifications, command palette, toasts`

### Phase K-9: Mini-Chat, Dock, Avatars
**Files:** `src/styles.css`, `src/views/agents/dock.ts`, `src/views/agents/mini-chat.ts`
**Scope:**
- Mini-chat terminal style
- Dock avatar sizing/borders
- Avatar frame system
**Risk:** Low — CSS + avatar sizing
**Commit:** `feat: kinetic hotline — mini-chat, dock, avatar frames`

### Phase K-10: Rust Pulse Bridge & Polish
**Files:** `src/engine/molecules/bridge.ts`, `src/styles.css`
**Scope:**
- Add pulse CSS classes to bridge event handlers
- Streaming pulse on agent prefix
- Tool call flash effects
- Settings toggle for pulse effects
**Risk:** Low — additive only
**Commit:** `feat: kinetic pulse bridge — event-driven visual heartbeat`

### Phase K-11: Specialized Views (Trading, Research, Content, Orchestrator, Memory, Nodes)
**Files:** Various view directories
**Scope:** Apply Kinetic patterns to less-trafficked views
**Risk:** Low-Medium — follows established patterns
**Commit (per view):**
- `K-11a`: Trading → Market Wire
- `K-11b`: Research → Deep Scan
- `K-11c`: Content → Pressroom
- `K-11d`: Orchestrator → Mission Planner
- `K-11e`: Memory Palace → Vault
- `K-11f`: Nodes → Engine Room

### Phase K-12: Onboarding & Tour
**Files:** `index.html`, `src/components/tour.ts`, `src/components/showcase.ts`
**Scope:**
- Welcome screen with new logo
- Installation wizard step indicator
- Tour overlay restyling
**Risk:** Low
**Commit:** `feat: kinetic onboarding — welcome screen, wizard, tour`

### Phase K-13: Light Theme Calibration
**Files:** `src/styles.css`
**Scope:** Fine-tune all light theme tokens for Kinetic palette
**Risk:** Low — token values only
**Commit:** `feat: kinetic light theme — warm cream palette`

---

## Appendix A: Files Changed Per Phase

| Phase | CSS | HTML | TypeScript | Rust |
|---|---|---|---|---|
| K-0 | ✓ | | | |
| K-1 | ✓ | ✓ (minor) | | |
| K-2 | ✓ | ✓ | ✓ (router) | |
| K-3 | ✓ | | ✓ (chat render) | |
| K-4 | ✓ | | ✓ (today molecules) | |
| K-5a-e | ✓ | | ✓ (multiple views) | |
| K-6 | ✓ | | | |
| K-7 | ✓ | ✓ | ✓ (settings-tabs) | |
| K-8 | ✓ | | ✓ (minor) | |
| K-9 | ✓ | | ✓ (dock, mini-chat) | |
| K-10 | ✓ | | ✓ (bridge) | |
| K-11a-f | ✓ | | ✓ (minor per view) | |
| K-12 | ✓ | ✓ | ✓ (tour, showcase) | |
| K-13 | ✓ | | | |

## Appendix B: What Does NOT Change

- **Atomic architecture** — atoms (pure) → molecules (DOM + IPC) → index (barrel) pattern is preserved
- **Rust backend** — no Rust changes in Kinetic (except K-10 pulse bridge which is frontend-only)
- **Data layer** — all Jotai atoms, IPC commands, data flow is untouched
- **Router logic** — `switchView()`, `viewMap`, `allViewIds` remain identical
- **Business logic** — integrations, health monitoring, guardrails, tool remapping all unchanged
- **Test infrastructure** — all existing tests remain valid (visual changes don't affect atom tests)

## Appendix C: Design Principles Checklist

For EVERY surface/view, verify:

- [ ] **Can I get the information I need in < 2 seconds?** (density check)
- [ ] **Is there a clear action path?** (every view has a primary action)
- [ ] **Does it use the token system?** (no hardcoded colors)
- [ ] **Is the border-radius ≤ 4px?** (except pills)
- [ ] **Are shadows used only on floating elements?** (modals, dropdowns, dock)
- [ ] **Is the text cream-on-black, not pure white?** (warm, not cold)
- [ ] **Does red mean action/active, sage mean healthy/success?**
- [ ] **Is there no decoration without information?** (every visual element earns its place)
