# Dashboard Overhaul Plan — "What Is OpenPawz?"

> Internal strategy doc. Not shipped. For eyes only.

---

## The UI Problem (This Is The Real Issue)

The tab count is a symptom. The real problem is **our UI doesn't look or feel like a product.** It looks like a settings panel with pixel font branding.

### What the OpenClaw dashboards get right visually

Every single one of those 6 repos — even the simplest ones — has a visual language that says "I am showing you important data." They do this with:

1. **Data density on the main screen.** You land on their dashboards and see numbers, charts, status dots, sparklines. They feel alive. Our Today page has a greeting, a weather icon, and a flat task list. It feels empty.

2. **Information hierarchy through size.** Mission Control's task count is 3rem bold. Their agent status uses colored dots. Clawd Control's CPU percentage is a big number with a sparkline. We use 13px for everything — card titles, body text, stats — it's all the same visual weight.

3. **Color carries meaning.** OpenClaw Dashboard uses green for healthy, amber for warning, red for over-limit — on progress bars, dots, backgrounds. Their usage card glows amber when you're at 80% rate limit. Our color system exists (we have `--status-error`, `--status-success` etc.) but we barely use it in the Today view. Everything is `--text-primary` gray.

4. **Cards feel like containers of value.** VidClaw's cards have subtle gradient borders, inner glow on hover, a header with an icon + title + data badge. Our cards are `background: var(--bg-primary); border: 1px solid var(--border)` — flat boxes with no visual distinction between a weather card and a task card.

5. **Motion and feedback.** Clawtrol has smooth transitions between views. LobsterBoard has animated count-up numbers. Mission Control's live feed scrolls in real time. Our transitions are `steps(3)` (literally pixelated jumps leftover from the retro CRT theme that we already removed the scanlines from).

### What our CSS actually says right now

```css
/* Our design tokens tell the story: */
--radius-sm: 2px;          /* Almost square corners — retro leftover */
--radius-lg: 6px;          /* "Large" radius is smaller than most apps' small */
--transition-fast: 100ms steps(3);   /* Steps = pixelated animation. WHY? */
--font-pixel: 'Press Start 2P';     /* Pixel font for headings — fun but unreadable */

/* Our cards: */
.today-card {
  background: var(--bg-primary);     /* Same as the page background */
  border: 1px solid var(--border);   /* Thin gray line. That's the entire visual treatment */
  border-radius: var(--radius-lg);   /* 6px — barely visible */
}

/* Our greeting: */
.today-greeting {
  font-size: 18px;                          /* Small for a hero heading */
  font-family: var(--font-pixel);           /* 8-bit font for the main greeting */
  text-shadow: 0 0 12px rgba(255,0,255,0.4); /* Magenta glow = retro CRT vibe */
}
```

**The retro pixel theme was the original brand identity — but we killed the CRT scanlines, removed the pixel art approach, and switched to Material Symbols.** What's left is an awkward hybrid: modern UI with 2px corners, `steps()` transitions, and pixel font headings dropped on top. It needs to be one thing or the other.

### The visual gap vs. those dashboards

| Aspect | OpenClaw Dashboards | OpenPawz Now |
|--------|-------------------|--------------|
| **Card radius** | 12-16px (modern, rounded) | 2-6px (sharp, retro) |
| **Card bg** | Glass/elevated or distinct bg | Same as page bg — cards don't pop |
| **Typography scale** | Stat numbers 2-3rem, labels 0.75rem, body 0.875rem | Everything 13-14px, pixel font for headings |
| **Data viz** | Sparklines, progress bars, heatmaps, ring gauges | None — text only |
| **Color system** | Semantic (green/amber/red for states) | Exists but unused in dashboard |
| **Transitions** | Smooth ease curves | `steps(3)` pixelated jumps |
| **Hover states** | Glow, lift, border color, smooth | `translateY(-1px)` + barely visible glow |
| **Empty states** | Illustrated, actionable | "No items" text |
| **Information density** | 6-8 data points visible on landing | 3 (weather, tasks, activity) |
| **Motion** | Live counters, streaming feeds, pulsing dots | Static renders, manual refresh |

---

## The Honest Assessment (Structure)

OpenPawz has **17 sidebar tabs** and **10 settings sub-tabs**. That's 27 separate screens in a desktop app that most users will open for the first time and think: "What am I looking at?"

The OpenClaw dashboard repos all have something OpenPawz doesn't: **a clear answer to "what does this app DO?"** within 5 seconds of opening it. Their dashboards show: agents working, money being spent, system health, live activity. Ours shows a greeting, a weather card, and a lot of empty states.

### Current Sidebar (17 items)

```
── (Top) ──────────────
   Today            ← Greeting + weather + tasks stub
   Chat             ← Works well, core feature
   Agents           ← Works, solid

── Work ───────────────
   Tasks            ← Kanban, works, good
   Squads           ← Multi-agent groups, works
   Files            ← ⚠️ Broken — needs Tauri file access, no files without manual setup
                       User opens it → empty state. Why is this here?

── Connect ────────────
   Mail             ← Works if Himalaya configured. Otherwise → empty
   Channels         ← Works if channel configured. Otherwise → empty
   Research         ← ⚠️ Saves to ~/Documents/Paw/Research. Weird standalone UI.
                       Essentially just "ask the agent to research something" with extra steps.
                       Could be a slash command or chat mode instead.
   Trading          ← Works if API keys set up. Niche — 95% of users don't trade crypto.

── Workspace ──────────
   My Skills        ← Our Phase 1-4 work. Solid now.
   PawzHub          ← Our Phase 2 work. Solid now.
   Foundry          ← Model listing + Chat Modes. Works, useful.

── System ─────────────
   Memory           ← Memory Palace. Works but confusing UX — install banner, tabs,
                       graph visualization. Most users don't understand what "Memory" means.
   Engine           ← Shows engine status. One button, one screen. Almost empty.
   Settings         ← 10 sub-tabs. Functional but overwhelming.
```

### The Problem Summed Up

1. **Half the tabs are empty on first launch.** Files, Mail, Channels, Research, Trading, Memory — all show empty states or error screens until the user configures external services. A new user sees a sidebar full of dead ends.

2. **No visual identity.** Every OpenClaw dashboard has a distinct visual language — glassmorphic cards, heatmaps, sparklines, live data. OpenPawz has flat gray cards with text. It works but it doesn't impress.

3. **The Today page doesn't tell a story.** It should answer: "What are your agents doing? How much have you spent? What's the system status?" Instead it says "Good morning" and shows the weather.

4. **Files and Research are vestigial.** Files requires manual Tauri file system setup and shows a tree view — but the agent already has file tools. Research is a glorified "ask the agent to research and save to disk" — the Chat view does this already with better UX.

5. **Too many top-level tabs for niche features.** Trading is a whole sidebar tab used by <5% of users. Memory is a whole tab that most users don't understand. Engine shows almost nothing.

6. **The app doesn't show what makes it special.** OpenPawz has 400+ integrations, multi-agent orchestration, tool approval, container sandboxes, 11 channel bridges. None of that is visible from the dashboard. You have to dig into Settings to discover it.

---

## What OpenPawz Actually Is

Let's define it clearly:

> **OpenPawz is a desktop AI command center.** You configure AI agents, give them skills (tools + knowledge), point them at your services (email, chat, code), and watch them work — all from one app, with no cloud dependency.

The dashboard should make that identity obvious in 5 seconds.

---

## The Fix — Phased Plan

### Phase 1: Today Page Transformation (The "Command Center")

**Goal:** When you open OpenPawz, you immediately see what matters.

**Current state:** Greeting → weather → task list → empty email → empty skill widgets → quick actions → activity feed

**New state — a real command center:**

```
┌──────────────────────────────────────────────────────────────┐
│  Good morning, User                          Feb 25, 2026    │
│  ────────────────────────────────────────────────────────────│
│                                                              │
│  ┌─────────────────────────┐  ┌─────────────────────────┐   │
│  │ 🤖 Agent Status         │  │ 📊 Usage Today           │   │
│  │  main ● idle            │  │  Tokens: 14.2k          │   │
│  │  researcher ● working   │  │  Cost: $0.42            │   │
│  │  coder ● idle           │  │  ▁▂▃▅▇▅▃▂ (24h)        │   │
│  └─────────────────────────┘  └─────────────────────────┘   │
│                                                              │
│  ┌─────────────────────────┐  ┌─────────────────────────┐   │
│  │ ⚡ Active Skills (12)    │  │ 💻 System Health         │   │
│  │  Weather ✓ Trading ✓    │  │  CPU ▂▃▅▃▂  RAM ▅▅▆▅▅  │   │
│  │  GitHub ✓  Email ✓      │  │  Disk: 67% used         │   │
│  │  + 8 more               │  │  Uptime: 3d 14h         │   │
│  └─────────────────────────┘  └─────────────────────────┘   │
│                                                              │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │ 📋 Tasks                                           +Add │ │
│  │  ☐ Review PR #142                         → coder       │ │
│  │  ☐ Write blog post draft                  → main        │ │
│  │  ✓ Deploy staging build                   ✓ done 2h ago │ │
│  └─────────────────────────────────────────────────────────┘ │
│                                                              │
│  ┌──────────────────────────┐  ┌────────────────────────┐   │
│  │ 🔥 Activity (live)       │  │ 🗓 30-Day Activity     │   │
│  │  agent used github_pr    │  │  ░░▓▓█▓▓░░▓▓█▓▓░░▓▓█  │   │
│  │  agent wrote 3 files     │  │  Mon—Sun heatmap       │   │
│  │  task completed: deploy  │  └────────────────────────┘   │
│  │  agent read 12 emails    │                               │
│  └──────────────────────────┘                               │
└──────────────────────────────────────────────────────────────┘
```

**What this adds:**
- Agent fleet status card (data already exists via `agentsList()`)
- Usage/cost tracking card (token data exists in session transcripts)
- System health sparklines (Tauri has sysinfo access)
- Active skills summary card (data from skills list)
- 30-day activity heatmap (data from task activity)
- Live-streaming activity feed (upgrade existing static feed)

**What we take from the repos:**
- Activity heatmap concept (from OpenClaw Dashboard)
- Usage/sparkline card style (from OpenClaw Dashboard + Clawd Control)
- Fleet overview layout (from Clawd Control)
- Live feed streaming (from Mission Control)

**CSS namespace:** `cmd-*` (command center)

---

### Phase 2: Sidebar Consolidation (Cut the Dead Weight)

**Goal:** Go from 17 tabs to ~10. Every remaining tab should work on first launch or clearly explain what it needs.

**Proposed new sidebar:**

```
── (Core) ─────────────
   Today              ← Command center (Phase 1)
   Chat               ← Keep as-is
   Agents             ← Keep as-is

── Work ───────────────
   Tasks              ← Keep — absorb Squads as a tab within Tasks
   Projects           ← Rename "Files" → "Projects" but only if it works.
                         Otherwise: REMOVE entirely. Agent has file tools.

── Connect ────────────
   Mail               ← Keep, but improve empty state
   Channels           ← Keep, but improve empty state

── Workspace ──────────
   My Skills          ← Keep (our Phase 1-4 work)
   PawzHub            ← Keep (our Phase 2 work)
   Foundry            ← Keep

── System ─────────────
   Settings           ← Keep — absorb Memory + Engine as settings sub-tabs
```

**What gets removed/merged:**
| Tab | Action | Reason |
|-----|--------|--------|
| **Files** | Remove OR rename + fix | Doesn't work without manual Tauri setup. Agent already has file tools. If we keep it, it needs to actually work out of the box. |
| **Research** | Remove — becomes a Chat Mode | It's just "ask the agent to research." Make it a Foundry Chat Mode called "Research" with the deep-research system prompt baked in. User picks Research mode in Chat, types query. Same result, no dead tab. |
| **Trading** | Remove from sidebar — becomes a Skill Widget | Trading is a niche feature. It should be a skill that, when enabled, adds a widget to the Today page. Not a whole sidebar tab. |
| **Squads** | Merge into Tasks | Squads are team-tasks. Put them as a tab within the Tasks view (like we did with Automations → Scheduled tab). |
| **Memory** | Move to Settings sub-tab | Memory Palace is a power-user feature. It's not something most users interact with daily. Put it under Settings → Memory. |
| **Engine** | Move to Settings sub-tab | "Engine" shows one screen of status info. It's a settings diagnostic, not a daily workspace tab. Merge into Settings → Engine. |

**Net effect:** 17 → 11 sidebar items. Every remaining tab has a purpose and works on first launch.

---

### Phase 3: Visual Design System Overhaul

**Goal:** Make OpenPawz look like a modern command center, not a leftover retro settings panel.

This is the phase that actually changes how the app FEELS. Phase 1 adds content to Today. Phase 2 cleans up the sidebar. Phase 3 changes every pixel the user sees.

#### 3A. Design Token Reset

Kill the retro-without-commitment look. Pick a direction: modern dashboard.

```css
/* ── BEFORE (current) ─────────────────────── */
--radius-sm: 2px;                    /* Barely rounded */
--radius-lg: 6px;                    /* "Large" is tiny */
--transition-fast: 100ms steps(3);   /* Pixel-stepped transitions (!) */
--transition-normal: 200ms steps(4);
--font-pixel: 'Press Start 2P';     /* Hard to read, retro holdover */

/* ── AFTER ────────────────────────────────── */
--radius-sm: 6px;                    /* Softer, modern */
--radius-md: 10px;
--radius-lg: 14px;                   /* Visually distinct cards */
--radius-xl: 18px;
--transition-fast: 120ms ease-out;   /* Smooth, not stepped */
--transition-normal: 200ms ease-out;
--transition-smooth: 350ms cubic-bezier(0.4, 0, 0.2, 1);

/* Keep --font-pixel for the brand wordmark "OpenPawz" in the sidebar ONLY.
   Everything else uses --font-sans. No more pixel headings on views. */
```

**What this alone does:** Every card, every button, every hover instantly feels smoother and more modern. The stepped pixel transitions are the single biggest "this feels amateur" signal.

#### 3B. Card Elevation System

Our cards are invisible — same background as the page, thin gray border. We need visual layers.

```css
/* Card tiers — not just one flat treatment for everything */

.card-surface {
  /* Subtle lift — for content containers */
  background: var(--bg-secondary);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-lg);
  box-shadow: 0 1px 3px rgba(0,0,0,0.2), 0 1px 2px rgba(0,0,0,0.15);
}

.card-elevated {
  /* Prominent — for stats, agent fleet, usage */
  background: var(--bg-secondary);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  box-shadow: 0 4px 12px rgba(0,0,0,0.25), 0 2px 4px rgba(0,0,0,0.15);
}

.card-elevated:hover {
  border-color: var(--accent-muted);
  box-shadow: 0 6px 20px rgba(0,0,0,0.3), 0 0 0 1px var(--accent-lighter);
  transform: translateY(-2px);
  transition: all var(--transition-smooth);
}

.card-glass {
  /* Hero cards — agent summary, top-level stats */
  background: linear-gradient(
    135deg,
    rgba(255,255,255,0.04) 0%,
    rgba(255,255,255,0.01) 100%
  );
  border: 1px solid rgba(255,255,255,0.08);
  border-radius: var(--radius-xl);
  backdrop-filter: blur(12px);
  box-shadow: 0 8px 32px rgba(0,0,0,0.3);
}
```

**Current vs. After:**
- Today page agent card: flat gray box → glass card with backdrop blur
- Task cards: thin border box → elevated card with shadow depth
- Stats cards: invisible → surface cards with warm highlight borders

#### 3C. Typography Scale

Right now we have: 13px body, 14px card titles, 18px greeting, pixel font headings. That's not a type system. Every element has the same visual weight.

```css
/* Type scale — dashboard needs BIG numbers and small labels */
--type-hero: 2rem;       /* 32px — main stat numbers (usage cost, token count) */
--type-title: 1.25rem;   /* 20px — page headings, card hero text */
--type-heading: 0.9375rem; /* 15px — card section titles */
--type-body: 0.8125rem;  /* 13px — content text (unchanged) */
--type-label: 0.6875rem; /* 11px — labels, metadata, timestamps */
--type-micro: 0.625rem;  /* 10px — badges, counts */

/* Stat number treatment */
.stat-value {
  font-size: var(--type-hero);
  font-weight: 700;
  font-family: var(--font-mono);
  letter-spacing: -0.02em;
  line-height: 1;
  color: var(--text-primary);
}

.stat-label {
  font-size: var(--type-label);
  font-weight: 500;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--text-muted);
  margin-top: 4px;
}

/* View titles — ditch pixel font, use clean sans */
.view-title {
  font-size: var(--type-title);
  font-weight: 700;
  font-family: var(--font-sans);  /* was: var(--font-pixel), 14px */
  color: var(--text-primary);     /* was: var(--cyan) with text-shadow glow */
  text-shadow: none;
}
```

**Impact:** When a stat card shows "$4.28" in 32px mono font with "today's cost" in 11px uppercase underneath — that instantly communicates hierarchy. Right now everything is 13-14px gray text with no distinction.

#### 3D. Data Visualization Atoms

These repos all have sparklines, progress bars, heatmaps. We have zero data viz. We need a small set of reusable SVG/CSS atoms:

```
New file: src/components/molecules/data-viz.ts

Exports:
  sparkline(data: number[], color: string, width?: number, height?: number): string
    → Returns inline SVG <svg>. Used for CPU, RAM, usage-over-time.
    → Inspired by: OpenClaw Dashboard's 24h CPU/RAM sparklines

  heatmapStrip(days: { date: string; count: number }[], color: string): string
    → Returns 30-day GitHub-style grid of small squares.
    → Inspired by: OpenClaw Dashboard's activity heatmap

  progressBar(percent: number, color: string, label?: string): string
    → Returns a horizontal bar with fill + percentage text.
    → Inspired by: OpenClaw Dashboard's rate limit bars

  progressRing(percent: number, color: string, size?: number): string
    → Returns circular SVG ring gauge.
    → Inspired by: LobsterBoard's disk usage widget (concept only — BSL licensed)

  statusDot(state: 'idle' | 'active' | 'error' | 'offline'): string
    → Returns a small dot with animated pulse for 'active'.
    → Inspired by: Clawd Control's agent health indicators

  animateCountUp(element: HTMLElement, target: number, duration?: number): void
    → Animates a number from 0 to target. Used for stat cards on load.
    → Inspired by: Mission Control's task counters
```

**CSS for data viz:**
```css
/* Sparkline */
.viz-sparkline { display: inline-block; vertical-align: middle; }
.viz-sparkline path { fill: none; stroke-width: 1.5; stroke-linecap: round; }

/* Heatmap */
.viz-heatmap { display: flex; gap: 2px; flex-wrap: wrap; }
.viz-heatmap-cell {
  width: 10px; height: 10px;
  border-radius: 2px;
  background: var(--bg-tertiary);
}

/* Progress bar */
.viz-progress { height: 6px; border-radius: 3px; background: var(--bg-tertiary); overflow: hidden; }
.viz-progress-fill { height: 100%; border-radius: 3px; transition: width 0.5s ease-out; }

/* Status dot */
.viz-dot { width: 8px; height: 8px; border-radius: 50%; display: inline-block; }
.viz-dot-active { animation: dotPulse 2s ease-in-out infinite; }
@keyframes dotPulse {
  0%, 100% { box-shadow: 0 0 0 0 currentColor; opacity: 1; }
  50% { box-shadow: 0 0 0 4px currentColor; opacity: 0.6; }
}
```

CSS namespace: `viz-*`

#### 3E. Sidebar Visual Refresh

The sidebar is the app's skeleton. It currently uses pixel font section labels, thin nav items, and a gradient border-image. Every repo we looked at had a cleaner sidebar.

```css
/* Section labels — kill pixel font, use clean caps */
.nav-section-label {
  font-family: var(--font-sans);  /* was: var(--font-pixel) at 8px */
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--text-muted);
}

/* Nav items — slightly larger, better padding */
.nav-item {
  padding: 8px 12px;
  border-radius: var(--radius-md);  /* was: var(--radius-sm) = 2px */
  font-size: 13px;
  border: none;  /* was: 1px solid transparent */
}

.nav-item:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
  text-shadow: none;  /* was: cyan glow */
}

.nav-item.active {
  background: var(--accent-subtle);
  color: var(--accent);
  font-weight: 600;
}
```

#### 3F. Heading Treatment Across All Views

Every view currently does: `font-family: var(--font-pixel); color: var(--cyan); text-shadow: glow`. That's the retro holdover. New treatment:

- View titles: `font-sans`, 20px, bold, white, no glow
- Section titles within views: 15px, semi-bold, `--text-primary`
- Card headers: 13px, 600 weight, with icon + title pattern
- The ONLY place `font-pixel` survives: the "OpenPawz" wordmark in the sidebar header

This is a global find-replace across ~30 `.view-title`, `.view-header`, section heading usages in styles.css.

#### Design System Summary — What Changes at a Glance

| Element | Before | After |
|---------|--------|-------|
| Card corners | 2-6px (almost square) | 10-14px (modern rounded) |
| Card background | Same as page | Elevated with shadow layers |
| Card hover | 1px translate + faint glow | 2px lift + border highlight + smooth shadow |
| Transitions | `steps(3)` pixel jumps | `ease-out` smooth curves |
| Page headings | Pixel font, cyan, glow | Sans-serif, 20px, bold, white |
| Section labels | Pixel font, 8px | Sans-serif, 10px, 600 weight |
| Stat numbers | 13-14px gray text | 32px mono bold |
| Data viz | None | Sparklines, progress bars, heatmap, status dots |
| Nav item shape | 2px radius, 1px border | 10px radius, no border |
| Nav hover | Cyan text + glow | Clean hover bg, primary text |
| Active nav | Magenta + border | Accent bg, accent text, bold |
| Sidebar title | Pixel font "OpenPawz" 11px | Keep pixel font here ONLY |

---

### Phase 4: Empty States That Guide

**Goal:** When a feature isn't configured, show an actionable empty state — not a blank screen.

Every unconfigured view should show:
1. **What this does** (one sentence)
2. **What you need** (prerequisites)
3. **How to set it up** (button that navigates to the right settings tab)
4. **What it looks like** (preview/illustration)

Example for Mail:
```
┌──────────────────────────────────────────────┐
│                                              │
│           📧                                 │
│                                              │
│     Your AI agent can read, write,           │
│     and manage your email.                   │
│                                              │
│     Connect an IMAP account to get started.  │
│                                              │
│     [ Set Up Email → ]                       │
│                                              │
│     Supports Gmail, Outlook, Fastmail,       │
│     and any IMAP provider.                   │
│                                              │
└──────────────────────────────────────────────┘
```

This replaces the current pattern of:
- Empty white space
- "No items" text with no context
- Error messages when services aren't configured

---

### Phase 5: Keyboard Power + Command Palette 2.0

**Goal:** Make the app feel snappy for power users.

1. **Keyboard shortcuts overlay** — Press `?` to see all shortcuts in a modal
2. **Enhanced command palette** — Already exists but expand it:
   - `>agent` → list agents, pick one to chat with
   - `>skill` → search skills, toggle enable/disable
   - `>task` → create task inline
   - `>mode` → switch chat mode
   - `>go mail` → navigate to view
3. **Global shortcuts:**
   - `1-9` → switch to sidebar tab by position
   - `Cmd+K` → command palette (already works?)
   - `Cmd+N` → new chat / new task (context-aware)
   - `Cmd+,` → settings

---

### Phase 6: "What Can I Do?" Showcase

**Goal:** When someone opens OpenPawz, they should see its capabilities showcased — not discover them by accident.

Ideas from the OpenClaw repos:
- **Mission Control's AI Planning flow**: Before giving a task to an agent, have a brief AI Q&A to clarify scope → this showcases multi-agent orchestration
- **VidClaw's Skills Manager page**: Shows all skills with enable/disable toggles and a "create custom skill" button → we already built this in Phase 1-4
- **Clawd Control's Fleet Overview**: All agents on one screen with health, status, sessions → our Agents page could learn from this layout

What we'd add:
- A **"Capabilities"** card on the Today page: "Your agent can: read email, browse the web, write code, trade crypto, manage GitHub PRs, ..." — dynamically generated from enabled skills
- **First-run guided tour**: 5-step overlay highlighting: Chat → Agents → Skills → Tasks → Settings
- **Showcase mode** (like Mission Control's demo): Pre-populated data so you can see what the app looks like when it's actually being used, even before real setup

---

## Priority Order

| Phase | Theme | Impact | Effort | Dependencies |
|-------|-------|--------|--------|---|
| **3** | Visual Design System | 🔥🔥🔥🔥 | Medium | **DO FIRST** — everything else builds on this |
| **1** | Today → Command Center | 🔥🔥🔥 | Medium | Needs Phase 3 design system |
| **2** | Sidebar Consolidation | 🔥🔥🔥 | Medium | Can parallel with Phase 1 |
| **4** | Empty States | 🔥🔥 | Low | Needs Phase 3 card styles |
| **5** | Keyboard Power | 🔥 | Low | Independent |
| **6** | Showcase | 🔥🔥 | High | After Phase 1-3 |

**Recommendation:** Phase 3 goes first — it's the foundation. The design token reset + card elevation + typography scale + data viz atoms is maybe 4-6 hours of CSS + one new TypeScript file. Once that's in, the Today command center (Phase 1) builds on top of it with the new card styles and viz atoms. Do Phase 2 (sidebar cuts) in parallel.

**The sequence that makes sense:**
1. Phase 3A-3C (token reset + cards + typography) — immediate visual upgrade to EVERY view
2. Phase 3D (data viz atoms) — build the sparkline/heatmap/progress components
3. Phase 1 (Today command center) — uses the new cards + viz to build the landing experience
4. Phase 2 (sidebar consolidation) — clean up dead tabs
5. Phase 3E-3F (sidebar + heading refresh) — polish pass on the new slimmer sidebar
6. Phase 4-6 (empty states, keyboard, showcase) — polish

---

## What We Should NOT Do

- **Don't add web auth / login** — We're a desktop app. No passwords.
- **Don't add cron management UI** — We have Automations inside Tasks.
- **Don't add a file browser just because others have one** — Our agent HAS file tools. The browser is the Chat view.
- **Don't add shell/terminal** — Desktop apps have terminals. Not our job.
- **Don't try to be Grafana** — We're an AI agent desktop app, not an ops dashboard.

---

## The One-Sentence Test

After this overhaul, when someone opens OpenPawz for the first time:

> "Oh, it's like a control center for my AI agents — I can see what they're doing, give them tasks, and add capabilities from a marketplace."

That's what they should think. Right now they think: "There's a lot of tabs. Most of them are empty."
