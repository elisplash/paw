# Contributing to Pawz

Thanks for your interest in contributing! This guide covers everything you need to get started.

---

## Development Setup

### Prerequisites

- **Node.js** 18+ — [nodejs.org](https://nodejs.org/)
- **Rust** (latest stable) — [rustup.rs](https://rustup.rs/)
- **Tauri v2 prerequisites** — [platform-specific dependencies](https://v2.tauri.app/start/prerequisites/)

### Getting Running

```bash
git clone https://github.com/OpenPawz/openpawz.git
cd paw
npm install
npm run tauri dev
```

This starts the Tauri dev server with hot-reload for the frontend and live-rebuild for the Rust backend.

### Verifying Changes

```bash
# Run all TypeScript tests (360 tests)
npx vitest run

# Run all Rust tests (242 tests)
cd src-tauri && cargo test

# TypeScript type-check + lint
npx tsc --noEmit
npx eslint src/

# Rust lint (zero warnings enforced)
cd src-tauri && cargo clippy -- -D warnings

# Code formatting
npx prettier --check "src/**/*.ts"
cd src-tauri && cargo fmt --check

# Full production build
npm run tauri build
```

---

## Project Structure

| Directory | Language | What's there |
|-----------|----------|-------------|
| `src/` | TypeScript | Frontend — views, features, components, styles |
| `src-tauri/src/` | Rust | Backend engine — agent loop, tools, channels, providers |
| `src/views/` | TypeScript | One file per UI page (agents, tasks, mail, etc.) |
| `src/features/` | TypeScript | Feature modules using atomic design (atoms → molecules → index) |
| `src-tauri/src/engine/` | Rust | All engine modules (19k LOC) |

See [ARCHITECTURE.md](ARCHITECTURE.md) for the full breakdown.

---

## Code Style

### TypeScript
- Vanilla DOM — no React, no Vue, no framework
- Each view renders into its HTML container via `document.getElementById`
- Use `const` over `let` where possible
- Template literals for HTML generation
- Material Symbols for icons — use `<span class="ms">icon_name</span>`
- Escape user content with `escHtml()` / `escAttr()` from `components/helpers.ts`

### Rust
- Standard Rust formatting (`cargo fmt`)
- Tauri commands are `async` functions with `#[tauri::command]`
- All commands registered in `lib.rs`
- Channel bridges follow a uniform pattern (start/stop/status/config/approve/deny)
- Error handling via typed `EngineError` enum (12 variants) — Tauri command boundaries convert with `.map_err(|e| e.to_string())`

### CSS
- Single `styles.css` file for all styles
- CSS custom properties for theming (`--bg-primary`, `--text`, `--accent`, etc.)
- BEM-ish class naming (`.view-header`, `.agent-dock-toggle`, `.nav-item`)
- No CSS preprocessors

---

## Making Changes

### Frontend (TypeScript)

1. Views live in `src/views/` — one file per page
2. Shared logic goes in `src/components/`
3. Feature-specific code uses the atomic pattern in `src/features/{feature}/`
4. IPC calls to the Rust backend use `invoke()` from Tauri
5. All IPC types are defined in `src/engine/atoms/types.ts`

### Backend (Rust)

1. New Tauri commands go in the appropriate file under `src-tauri/src/engine/`
2. Register commands in `src-tauri/src/lib.rs`
3. Commands module declaration in `src-tauri/src/commands/mod.rs` (if adding a new command file)

### Adding a Channel Bridge

Each bridge follows the same pattern. Create a new file (or directory module for complex bridges) in `src-tauri/src/engine/` with:
- `start_*` / `stop_*` — spawn/kill the bridge task
- `get_*_config` / `set_*_config` — configuration CRUD
- `*_status` — running state check
- `approve_user` / `deny_user` / `remove_user` — access control
- Message handler → route to configured agent → send response back

### Adding an AI Provider

For OpenAI-compatible providers:
1. Add model-prefix routing in `commands.rs` → `resolve_provider_for_model()`
2. Add the provider kind to frontend constants in `settings-models.ts`

For non-compatible providers:
1. Add a new match arm in `providers.rs` with the provider's streaming API
2. Handle the response format, tool calling convention, and error mapping

---

## Pull Requests

1. Fork the repo and create a feature branch
2. Make your changes
3. Run `npx tsc --noEmit` and `cd src-tauri && cargo check` — both must pass
4. Write a clear PR description explaining what changed and why
5. Keep PRs focused — one feature or fix per PR

---

## CI Pipeline

Every push and PR triggers 3 parallel CI jobs:

| Job | What it checks | Timeout |
|-----|---------------|--------|
| **TypeScript** | `tsc --noEmit` → `eslint` → `vitest run` (360 tests) → `prettier --check` | 10 min |
| **Rust** | `cargo check` → `cargo test` (242 tests) → `cargo clippy -- -D warnings` | 30 min |
| **Security Audit** | `cargo audit` → `npm audit --audit-level=high` | 10 min |

All 3 jobs must pass. Zero clippy warnings enforced. Zero known vulnerabilities enforced.

### Writing Tests

**Rust tests** live in `#[cfg(test)]` modules within each source file, plus 4 integration test files in `src-tauri/tests/`. Run with `cd src-tauri && cargo test`.

**TypeScript tests** use vitest. Test files are co-located with source (e.g., `security.test.ts` next to `security.ts`). Run with `npx vitest run`.

When adding new features, include tests for:
- Happy path and error cases
- Edge cases (empty input, boundary values)
- Security-relevant behavior (injection patterns, access control)

---

## Reporting Issues

Open an issue on GitHub with:
- What you expected to happen
- What actually happened
- Steps to reproduce
- OS and version
- Relevant error messages or screenshots

For security vulnerabilities, see [SECURITY.md](SECURITY.md).

---

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
