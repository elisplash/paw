## Code-Generation Guidelines

When you write, modify, or install code (skills, MCP servers, scripts, Rust modules, extensions, or any executable artifact), you **must** follow every rule below. Violations will be reverted.

### 1. Repository Hygiene
- **Never commit build artifacts.** `target/`, `node_modules/`, `dist/`, `build/`, `*.o`, `*.so`, `*.dylib`, `*.exe`, `*.wasm` (unless intentional release assets) must never be staged.
- **Keep `.gitignore` current.** If you create a new build pipeline, add its output directory to `.gitignore` before the first commit.
- **Atomic commits.** One logical change per commit. Don't bundle unrelated fixes.
- **No generated files in source.** Lock-files (`Cargo.lock` for binaries, `package-lock.json`) are fine; generated code is not.

### 2. Architecture Compliance
- **Use shared modules.** Before creating new infrastructure, check if `src-tauri/src/engine/channels/`, `src-tauri/src/engine/skills/`, or another existing module already covers the need. Extend, don't duplicate.
- **No standalone binaries.** Never compile a separate sidecar binary when the functionality should integrate with the engine. All Rust code compiles into the single Tauri binary.
- **Follow the layered architecture.** `commands/` (thin system layer) → `engine/` (organisms + molecules + atoms). Organisms contain business logic; atoms are pure helpers. Never import `commands/` from `engine/`.
- **Use engine error types.** Return `EngineResult<T>` from engine functions. Map external errors with `EngineError::Internal(msg)`. Never `unwrap()` in production paths — use `?` or explicit error handling.

### 3. Rust Code Standards
- **Logging.** Use the `log` crate (`info!`, `warn!`, `error!`). Prefix every message with a bracketed tag: `info!("[discord] Connected to gateway")`. Never use `println!` or `eprintln!` in library code.
- **Error handling.** Propagate with `?`. Add context: `.map_err(|e| EngineError::Internal(format!("[skill] load failed: {}", e)))?`.
- **String safety.** When truncating strings, always use `floor_char_boundary()` to avoid slicing inside a multi-byte UTF-8 character.
- **Async discipline.** Use `tokio` for async. Never block the async runtime with `std::thread::sleep` — use `tokio::time::sleep`. Never spawn detached threads for work that should be a `tokio::spawn` task with proper cancellation.
- **Dependencies.** Do not add new crate dependencies without justification. Prefer what's already in `Cargo.toml`. If you must add a crate, verify it's maintained and has a compatible license (MIT/Apache-2.0).

### 4. TypeScript / Frontend Standards
- **State management.** Use Jotai atoms in `src/state/`. Never use global mutable variables.
- **Reactivity.** Components use Lit (`@lit/reactive-element`). Follow existing patterns in `src/components/` and `src/engine/`.
- **No raw DOM manipulation.** Use Lit's reactive properties and templates.
- **Imports.** Relative paths within `src/`. No circular imports.

### 5. Security — Non-Negotiable
- **No hardcoded secrets.** API keys, tokens, passwords, and private keys must **never** appear in source code, commit messages, or logs. Use the TOML `[[credentials]]` system or environment variables.
- **Credential injection.** In TOML skill instructions, reference credentials as `{{KEY_NAME}}`. The engine replaces these at runtime from the encrypted credential store.
- **Sanitize all external input.** Shell arguments via `exec` must be escaped. User-supplied strings must never be interpolated into SQL, shell commands, or file paths without validation.
- **File-system boundaries.** Skills and scripts must operate within the agent workspace (`~/.paw/agents/{id}/workspace/`) or designated config dirs. Never write outside these paths without explicit user approval.

### 6. TOML Skill Authoring
- **Manifest required.** Every skill directory under `~/.paw/skills/{id}/` must contain a valid `pawz-skill.toml` with at minimum `[skill]` (id, name, version, author, category, description) and `[instructions]`.
- **ID format.** Lowercase alphanumeric + hyphens only: `my-cool-skill`. No underscores, spaces, or uppercase.
- **Instruction text.** The `[instructions].text` field tells the agent how to use the skill. Include: base URL, auth header format, available endpoints, and example payloads. Keep under 2000 chars — instructions are subject to compression.
- **Credentials.** Declare every required secret in `[[credentials]]` with clear labels and placeholders. Reference in instructions as `{{KEY_NAME}}`.
- **Category.** Use one of: `vault`, `cli`, `api`, `productivity`, `media`, `smart_home`, `communication`, `development`, `system`.

### 7. MCP Server Authoring
- **Transport.** Default is `stdio`. Only use `sse` or `streamable-http` if the server is remote.
- **Keep it lean.** An MCP server should expose a focused set of tools. Don't build monoliths — split into multiple skills if the domain is broad.
- **Error responses.** Return structured JSON errors with `isError: true` and a human-readable message. Never crash on bad input.
- **Startup.** The MCP `command` must be executable on the user's system. Document `required_binaries` and `install_hint` in the TOML manifest.

### 8. Testing & Validation
- **Tests are mandatory for new logic.** Rust: add `#[cfg(test)] mod tests` in the same file or a dedicated `tests/` module. TypeScript: add `.test.ts` alongside the source file using Vitest.
- **Run tests before committing.** `cargo test` for Rust, `npx vitest run` for TypeScript.
- **Don't break existing tests.** If your change causes test failures, fix them in the same commit.

### 9. Process
- **Read before writing.** Before creating a new file, use `list_directory` and `read_file` to check for existing implementations.
- **Explain the plan first.** Before writing more than 50 lines of code, briefly describe what you're building and which existing modules you'll integrate with.
- **One skill = one PR scope.** Don't mix unrelated skill work. Each skill or feature is a self-contained unit.
- **Clean up after yourself.** Remove temporary files, test artifacts, and debug logging before committing.