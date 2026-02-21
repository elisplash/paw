# Audit TODO

Full code audit completed 2025-02-21. Findings below organized by priority.
Codebase: 24,750 lines TS · 30,935 lines Rust · 327 tests passing · ESLint 0 warnings

---

## Tier 1 — Security-Critical

### ~~1. Channel bridge auto-approves ALL tool calls (CRITICAL)~~ ✅ FIXED
- **File:** `src-tauri/src/engine/channels.rs` L259-275
- **Bug:** Remote channel users (Discord, Telegram, IRC, etc.) had every tool call auto-approved — including `exec`, `write_file`, `delete_file`. Any approved channel user could trigger arbitrary command execution without HIL review.
- **Fix applied:** Changed the channel bridge auto-approver to **deny** all side-effect tool calls from remote channels. Read-only tools (fetch, read_file, web_search, etc.) remain auto-approved via the agent_loop's own `auto_approved_tools` list. Dangerous tools that would normally require HIL approval are now denied outright for remote channel users since no one is present at the Pawz UI to review them.

### ~~2. XSS via attachment filename (CRITICAL)~~ ✅ FIXED
- **File:** `src/engine/organisms/chat_controller.ts` L618
- **Bug:** `att.name` was injected directly into `innerHTML` without HTML-escaping. A malicious filename like `<img onerror=alert(1)>` could execute JavaScript.
- **Fix applied:** Replaced string interpolation into `innerHTML` with DOM API — icon set via `innerHTML` (safe, static SVG), filename set via `textContent` on a separate `<span>` element. Matches the pattern already used for image attachment labels at L611.

### ~~3. XSS via LLM-controlled markdown (CRITICAL)~~ ✅ FIXED
- **File:** `src/engine/organisms/chat_controller.ts` L536, L432
- **Bug:** Audit found that `formatMarkdown()` already escapes raw HTML tags via `escHtml()` (DOM-based `textContent` → `innerHTML` round-trip). However, the markdown link regex `[text](url)` placed the URL directly into `href="$2"` without attribute-escaping. Since `escHtml()` does not escape `"`, an LLM could craft `[click](https://evil.com" onmouseover="alert(1))` to break out of the href attribute.
- **Fix applied:** Changed the link regex replacement from string form to function form, applying `escAttr()` to the URL before interpolation into the `href` attribute. `escAttr()` escapes `"` and `'` to their HTML entities.

### 4. Compact button calls sessionClear instead of sessionCompact (CRITICAL)
- **File:** `src/engine/organisms/chat_controller.ts` L1107-1115
- **Bug:** The "Compact" button handler calls `pawEngine.sessionClear()` (destroys all messages) instead of `pawEngine.sessionCompact()` (summarizes them). Users lose their entire session.
- **Fix:** Change `sessionClear` → `sessionCompact` at L1110.

### 5. Nostr crypto uses SHA-256 placeholder (CRITICAL)
- **File:** `src-tauri/src/engine/nostr.rs` L418-436
- **Bug:** `derive_pubkey()` uses SHA-256 instead of secp256k1, producing invalid Nostr signatures. There's a TODO in production code.
- **Fix:** Implement proper secp256k1 key derivation or use the `secp256k1` crate.

### 6. Docs claim unimplemented security features (CRITICAL)
- **File:** `docs/docs/channels/nostr.md` L31, `docs/docs/channels/matrix.md` L35
- **Bug:** Docs claim "NIP-04 encrypted DMs" and "End-to-end encryption support" — neither is implemented. Misleads users about security posture.
- **Fix:** Remove both claims. Add "Planned" labels if desired.

---

## Tier 2 — High Impact

### 7. ReDoS in security allowlist/denylist
- **File:** `src/security.ts` L353-364
- **Bug:** User-configured patterns compiled with `new RegExp(p, 'i')` without complexity checks. Catastrophic backtracking possible (e.g. `(a+)+$`).
- **Fix:** Wrap regex compilation in a try/catch with timeout, or validate pattern complexity before saving.

### 8. Security settings stored in unprotected localStorage
- **File:** `src/security.ts` L200-315
- **Bug:** Allowlist, denylist, and `sessionOverrideUntil` stored in `localStorage` — accessible from DevTools or any XSS. An attacker could `activateSessionOverride(99999)` to disable all checks.
- **Fix:** Move to the encrypted SQLite database or Tauri secure storage.

### 9. chatAbort is a no-op
- **File:** `src/engine/molecules/ipc_client.ts` L107-109
- **Bug:** Logs a warning, does nothing. Users see an abort button that has no effect.
- **Fix:** Implement backend abort or remove the abort button from the UI.

### 10. Encryption silently falls back to plaintext
- **File:** `src/db.ts` L62-68
- **Bug:** If `_cryptoKey` is null (keychain unavailable), `encryptField()` returns plaintext with no user-visible indicator. Sensitive data stored unencrypted.
- **Fix:** Show a persistent warning banner when encryption is unavailable. Consider blocking credential storage without encryption.

### 11. Non-null assertion crash on empty encryption key
- **File:** `src/db.ts` L38
- **Bug:** `hexKey.match(/.{1,2}/g)!` crashes if `hexKey` is empty string (`.match()` returns `null`).
- **Fix:** Add `if (!hexKey) return false;` guard before the match.

### 12. Event listener leak on task-updated
- **File:** `src/main.ts` L128-130
- **Bug:** `listen('task-updated', ...)` unlisten function never stored or called. Duplicates stack on hot reload.
- **Fix:** Store the unlisten function and call it on cleanup / before re-registration.

### 13. Global streaming timeout (not per-session)
- **File:** `src/state/index.ts` L138
- **Bug:** `streamingTimeout` is a single value so concurrent streams overwrite each other's timeouts.
- **Fix:** Move timeout into the `StreamState` map entry per session key.

### 14. Non-null assertion on stream state
- **File:** `src/engine/organisms/chat_controller.ts` L858-861
- **Bug:** `activeStreams.get(streamKey)!` can crash if session key mutated between setup and use.
- **Fix:** Add null check with graceful fallback.

### 15. Timer leak in voice recording
- **File:** `src/engine/organisms/chat_controller.ts` L1215
- **Bug:** 30s auto-stop timeout never cancelled on manual stop. Stale callbacks accumulate.
- **Fix:** Store timeout ID, clear on manual stop.

### 16. Hardcoded username "Eli" in greeting
- **File:** `src/views/today/molecules.ts` L206
- **Bug:** Every user sees "Hello, Eli" on the dashboard.
- **Fix:** Replace with user-configurable name or remove the name.

### 17. `rm` in default command allowlist
- **File:** `src/security.ts` L309-315
- **Bug:** `'^rm\\b'` auto-approves all `rm` commands even though `rm -rf /` is classified as critical. Allowlist vs danger-pattern precedence is undefined.
- **Fix:** Remove `rm` from default allowlist or ensure danger patterns take precedence over allowlist.

### 18. Exfiltration tools in safe_tools
- **Rust channel bridge auto-approves `email_send`, `slack_send`, `webhook_send` — data exfiltration vectors.
- **Fix:** Remove exfiltration-capable tools from the auto-approve set.

### 19. Path traversal in filesystem tools
- **Rust filesystem tools accept absolute paths with no sandbox enforcement. Agents can read/write anywhere on the host.
- **Fix:** Validate paths against project root or a configured sandbox.

### 20. DB init failure silently swallowed
- **File:** `src/main.ts` L177-178
- **Bug:** `.catch(e => console.warn(...))` on `initDb()` — app runs without database, all DB ops return null.
- **Fix:** Show error state or retry logic when DB fails to initialize.

### 21. Duplicate / dead error boundary
- **File:** `src/main.ts` L71-78, `src/error-boundary.ts`
- **Bug:** `main.ts` installs its own global error handlers. `installErrorBoundary()` is exported but never called — dead code.
- **Fix:** Wire up `installErrorBoundary()` from `main.ts` and remove the duplicate handlers, or delete `error-boundary.ts`.

### 22. Event listeners re-bound on every render (12 views)
- Multiple view files re-bind click handlers on every `load*()` call without cleanup or delegation.
- **Fix:** Use event delegation or track/remove previous listeners.

---

## Tier 3 — Code Quality

### 23. 7 functions over 100 lines
- `loadChannels` (~280 lines), `openAgentEditor` (~290 lines), `renderMessages` (148 lines), `sendMessage` (144 lines), `runMigrations` (160 lines) + 2 Rust functions (orchestrator 514 lines)
- **Fix:** Decompose into smaller helper functions.

### 24. No migration versioning
- **File:** `src/db.ts` L86-245
- All migrations are `CREATE TABLE IF NOT EXISTS` with no version tracking. `ALTER TABLE` has no detection mechanism. No transaction wrapping.
- **Fix:** Add a `schema_version` table and sequential migration numbers.

### 25. `confirm()`/`alert()` used in Tauri context (8 files)
- Native `confirm()` / `alert()` may not render in Tauri webview context.
- **Fix:** Replace with Tauri dialog API or custom modal.

### 26. `setTimeout` as polling substitute (10+ instances)
- Arbitrary `setTimeout` delays instead of listening for backend completion events.
- **Fix:** Use Tauri events or IPC callbacks.

### 27. `setInterval` without view-switch cleanup (3 views)
- Orchestrator, settings, tasks views start intervals that aren't cleared on navigation.
- **Fix:** Clear intervals when view unmounts.

### 28. Hardcoded model prices
- **File:** `src/state/index.ts` L30-80
- Model context sizes and per-token costs will become stale.
- **Fix:** Fetch from config, backend, or a remote source.

### 29. `INSERT OR REPLACE` resets created_at
- **File:** `src/db.ts` L293-311
- `INSERT OR REPLACE` deletes and re-creates the row, losing `created_at`.
- **Fix:** Use `INSERT ... ON CONFLICT ... DO UPDATE`.

### 30. No file/network log transport
- **File:** `src/logger.ts`
- Logs go only to console and in-memory buffer. No way to retrieve after crash.
- **Fix:** Add file transport or Tauri-side log persistence.

### 31. Singleton prevents test mocking
- **File:** `src/engine/molecules/ipc_client.ts` L735
- `export const pawEngine = new PawEngineClient()` at module level — no mock seam for tests.
- **Fix:** Export a factory function or accept dependency injection.

### 32. `activeStreams` Map never bounded
- **File:** `src/state/index.ts` L100
- On error paths the map entry may not be deleted, causing slow memory leak over long sessions.
- **Fix:** Add cleanup sweep or bounded map.

### 33. Token cost drift
- **File:** `src/engine/organisms/chat_controller.ts` L340
- `recordTokenUsage` replaces `sessionInputTokens` but adds to `sessionOutputTokens`. Over multiple turns, running total becomes inaccurate.
- **Fix:** Accumulate both consistently.

---

## Docs Site Fixes

### 34. Remove false feature claims
- `docs/docs/channels/nostr.md` L31 — remove "NIP-04 encrypted DMs"
- `docs/docs/channels/matrix.md` L35 — remove "End-to-end encryption support"

### 35. Fix incorrect counts
- Channel count: 10 → 11 (in `getting-started.md`, `architecture.md`, `SECURITY.md`)
- Skill count: 37 → 40 (in `getting-started.md`)
- Settings tabs: 12 → 11 (in `architecture.md`)
- Trading tools: 8 → 7 (in `trading.md`)

### 36. Fix architecture.md view paths
- All view paths listed as `views/today.ts` etc — should be `views/today/` (directories with atoms/molecules/index)
- `web.rs` listed as channel bridge — it's the web scraping module
- `whatsapp.rs` missing from channel bridge list

### 37. Fix projects.md sensitive paths list
- Current list (`.azure`, `.gcloud`, `.npmrc`, etc.) doesn't match actual code
- Real patterns: `.gnome-keyring`, `.password-store`, `/dev`, `.openclaw`, Windows paths, etc.

### 38. Fix injection category count
- Docs say 9 categories — Rust scanner has 8 (no `obfuscation`)
- Document the frontend/backend scanner distinction

### 39. Update pricing table
- Missing: `claude-haiku-4`, `o4-mini`, `o3-mini`, `deepseek-reasoner`, all `gpt-4.1` variants

### 40. Fix button text inconsistency
- `first-agent.md` says "Create Agent", `agents.md` says "New Agent" — standardize

### 41. Add missing docs
- WhatsApp channel guide (engine module exists, no docs page)
- `gpt-4.1` model family documentation
- `claude-haiku-4` model documentation

---

## Stats

| Severity | Count |
|----------|-------|
| Critical | 6 |
| High | 16 |
| Medium / Quality | 11 |
| Docs | 8 |
| **Total** | **41** |
