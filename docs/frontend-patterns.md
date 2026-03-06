# Frontend Patterns

This guide explains how the TypeScript frontend is organized and how to add or extend a view without fighting the existing structure.

The frontend is plain TypeScript plus DOM APIs. There is no React or Vue layer. Most screens follow a simple rule: an `index.ts` file orchestrates the page, `atoms.ts` holds pure helpers and types, and `molecules.ts` owns DOM updates or IPC-heavy behavior.

## Mental Model

- `src/views/`: screen-level modules such as Today, Channels, Tasks, and Settings tabs.
- `src/components/`: shared UI pieces and generic DOM helpers.
- `src/features/`: reusable feature modules that use the same atoms/molecules split.
- `src/engine/molecules/ipc_client.ts`: the typed wrapper around all Tauri `invoke()` calls.
- `src/views/router.ts`: the central place that maps sidebar/view names to actual view loaders.

## A Typical View Shape

Many views follow this layout:

- `atoms.ts`: types, constants, and pure transformation helpers
- `molecules.ts`: rendering, DOM listeners, and IPC-heavy functions
- `index.ts`: public entry point that wires the module together

Examples:

- [`src/views/today/index.ts`](../src/views/today/index.ts): orchestration-first view that renders once, then loads cards in parallel
- [`src/views/channels/index.ts`](../src/views/channels/index.ts): setup-heavy view that wires event listeners and delegates to `molecules.ts`
- [`src/views/channels/atoms.ts`](../src/views/channels/atoms.ts): pure configuration data for supported channels

## Routing Pattern

[`src/views/router.ts`](../src/views/router.ts) is the first file to read when you want to know how a screen is activated.

Key ideas:

- `allViewIds` contains the DOM container IDs that can become active.
- `viewMap` maps logical navigation names to those DOM containers.
- `switchView()` handles nav highlighting, container activation, and per-view load functions.
- Views are loaded lazily by calling exported functions like `loadToday()`, `loadChannels()`, or `loadSettings()`.

If your new view needs to appear in navigation, it usually requires:

1. A container in the HTML
2. A route entry in `allViewIds`
3. A mapping in `viewMap`
4. A case in `switchView()` that calls your loader

## Rendering Pattern

The codebase does not use a virtual DOM. Most UI updates are direct DOM operations.

Common patterns:

- Build HTML strings with template literals for larger chunks.
- Insert them with `innerHTML` when replacing an entire region.
- Use helper functions like `$`, `escHtml()`, and `escAttr()` from shared helpers.
- Keep pure mapping logic in `atoms.ts` and let `molecules.ts` touch the DOM.
- For expensive pages, render the shell first and load cards or subsections afterward.

The Today view is a good example of progressive rendering:

1. Load critical state
2. Render the page once
3. Kick off parallel async fetches
4. Let each card update itself independently

That pattern keeps one slow card from blocking the full page.

## State Pattern

State is usually local to the view unless several modules need to share it.

Typical options in this repo:

- Module-local variables in `index.ts` for view state
- Lightweight setter/getter bridges from `index.ts` into `molecules.ts`
- Shared app-level state in `src/state/`
- Backend-backed state via `pawEngine.*` calls

The Today view uses a small state bridge:

- `index.ts` owns `_tasks`
- `initMoleculesState()` injects getters and setters into `molecules.ts`
- `molecules.ts` can render using shared state without owning the source of truth

Use this pattern when you want `molecules.ts` to stay testable and not depend on hidden globals.

## IPC Pattern

Frontend code should not call Tauri `invoke()` directly in random files. The standard path is:

1. Add or reuse a typed method in [`src/engine/molecules/ipc_client.ts`](../src/engine/molecules/ipc_client.ts)
2. Import `pawEngine` from `src/engine`
3. Call `pawEngine.someMethod()` from the view, component, or feature

Benefits:

- One place to find all backend APIs
- Shared request and response typing
- Easier refactors when command names or payloads change

## Adding A New View

Use this checklist:

1. Create `src/views/<view-name>/`
2. Add `atoms.ts` if you need pure types or mapping helpers
3. Add `molecules.ts` for DOM rendering and side effects
4. Add `index.ts` as the public surface for the view
5. Export loader/init functions from `index.ts`
6. Wire the view into [`src/views/router.ts`](../src/views/router.ts)
7. Use `pawEngine` for backend calls instead of raw `invoke()`
8. Reuse shared helpers from `src/components/` before creating new utility code

## When To Use `components/` vs `features/` vs `views/`

- Put code in `views/` if it belongs to one page or screen.
- Put code in `components/` if it is shared UI or generic helper logic.
- Put code in `features/` if it is a reusable capability with its own small atoms/molecules structure.

If you are unsure, start in the owning view. You can extract later once duplication becomes real.

## Common Mistakes

- Mixing raw `invoke()` calls directly into many UI files
- Putting DOM manipulation inside `atoms.ts`
- Skipping escaping helpers when rendering user-controlled content
- Re-rendering a whole page when only one panel or card needs to change
- Adding global state for something that only one view uses

## Recommended Read Order

1. [`src/views/router.ts`](../src/views/router.ts)
2. One simple view such as [`src/views/today/index.ts`](../src/views/today/index.ts)
3. One setup-heavy view such as [`src/views/channels/index.ts`](../src/views/channels/index.ts)
4. [`src/engine/molecules/ipc_client.ts`](../src/engine/molecules/ipc_client.ts)
