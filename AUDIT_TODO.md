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

### ~~4. Compact button calls sessionClear instead of sessionCompact (CRITICAL)~~ ✅ FIXED
- **File:** `src/engine/organisms/chat_controller.ts` L1110
- **Bug:** The "Compact" button handler called `pawEngine.sessionClear()` (destroys all messages) instead of `pawEngine.sessionCompact()` (summarizes them). Users lost their entire session.
- **Fix applied:** Changed to `pawEngine.sessionCompact()`, added toast showing before/after message count, and reloads compacted history into the UI so the user sees the summarized conversation.

### ~~5. Nostr crypto uses SHA-256 placeholder (CRITICAL)~~ ✅ FIXED
- **File:** `src-tauri/src/engine/nostr.rs` L418-436, `src-tauri/Cargo.toml`
- **Bug:** `derive_pubkey()` used double SHA-256 hash instead of secp256k1 point multiplication, producing invalid public keys. `build_reply_event()` set signature to 128 zeros instead of computing a BIP-340 Schnorr signature.
- **Fix applied:** Added `schnorr` feature to existing `k256` crate dependency. Replaced `derive_pubkey()` with proper secp256k1 `SecretKey → PublicKey → x-only` derivation. Replaced zero-signature with real BIP-340 Schnorr signing via `k256::schnorr::SigningKey::sign_raw()` with random auxiliary data. Nostr events are now cryptographically valid and will be accepted by relays.

### 6. Docs claim unimplemented security features (CRITICAL)
- **File:** `docs/docs/channels/nostr.md` L31, `docs/docs/channels/matrix.md` L35
- **Bug:** Docs claim "NIP-04 encrypted DMs" and "End-to-end encryption support" — neither is implemented. Misleads users about security posture.
- **Fix:** Remove both claims. Add "Planned" labels if desired.

---

## Tier 2 — High Impact

### ~~7. ReDoS in security allowlist/denylist~~ ✅ FIXED
- Added `isReDoSRisk()` detector (nested quantifiers, overlapping alternation), `validateRegexPattern()` for save-time validation, and `safeRegexTest()` wrapper. Updated `matchesAllowlist` and `matchesDenylist` to reject ReDoS-risk patterns silently. 19 new tests in `src/security.test.ts`.

### ~~8. Security settings stored in unprotected localStorage~~ ✅ FIXED
- Moved security settings from `localStorage` to the encrypted SQLite database. Added `security_settings` table (single-row upsert), DB CRUD in `db.ts`, in-memory cache in `security.ts` for sync access. `initSecuritySettings()` hydrates cache at startup, auto-migrates legacy localStorage data, then clears it. `resetSecurityPolicies()` now uses DB delete instead of `localStorage.removeItem()`. 4 new tests for cache behaviour.

### ~~9. chatAbort is a no-op~~ ✅ FIXED
- Added `active_runs: HashMap<String, AbortHandle>` to `EngineState`. `engine_chat_send` registers the spawned task's abort handle keyed by session_id. New `engine_chat_abort` Tauri command looks up and aborts the task. The panic safety monitor detects cancellation vs crash and emits appropriate `Complete`/`Error` events. Frontend `chatAbort()` now invokes the real backend command. Research abort also works via same path.

### ~~10. Encryption silently falls back to plaintext~~ ✅ FIXED
- Added persistent global warning banner (`#encryption-warning-banner`) that appears at the top of every view when the OS keychain is unavailable and encryption can't be initialised. `initDbEncryption()` return value now drives banner visibility. `encryptField()` logs an explicit warning on each plaintext fallback. Banner is dismissible but reappears on next app launch.

### ~~11. Non-null assertion crash on empty encryption key~~ ✅ FIXED
- **File:** `src/db.ts` L38
- **Bug:** `hexKey.match(/.{1,2}/g)!` crashes if `hexKey` is empty string (`.match()` returns `null`).
- **Fix:** Removed non-null assertion. Match result is now stored in a variable with an explicit `if (!hexPairs) return false;` guard, making the code defensive against future refactors that might remove the earlier `!hexKey` check.

### ~~12. Event listener leak on task-updated~~ ✅ FIXED
- **File:** `src/main.ts` L128-130
- **Bug:** `listen('task-updated', ...)` unlisten function never stored or called. Duplicates stack on hot reload.
- **Fix:** Added module-level `unlistenTaskUpdated` variable. Before registering a new listener, any previous one is unlistened. The `listen()` promise resolves the unlisten function and stores it for next cleanup.

### ~~13. Global streaming timeout (not per-session)~~ ✅ FIXED
- **File:** `src/state/index.ts` L138
- **Bug:** `streamingTimeout` is a single value so concurrent streams overwrite each other's timeouts.
- **Fix:** Removed global `appState.streamingTimeout`. Timeout now lives in the per-session `StreamState.timeout` field (which already existed but was unused). Set and cleared via `ss.timeout` in chat_controller.ts, so concurrent streams each have independent timeouts.

### ~~14. Non-null assertion on stream state~~ ✅ FIXED
- **File:** `src/engine/organisms/chat_controller.ts` L858-861
- **Bug:** `activeStreams.get(streamKey)!` can crash if session key mutated between setup and use.
- **Fix:** Replaced `!` assertion with explicit null check. If stream state is missing, logs an error, re-enables the send button, and returns early instead of crashing.

### ~~15. Timer leak in voice recording~~ ✅ FIXED
- **File:** `src/engine/organisms/chat_controller.ts` L1215
- **Bug:** 30s auto-stop timeout never cancelled on manual stop. Stale callbacks accumulate.
- **Fix:** Added `_chatTalkTimeout` module variable. The 30s auto-stop `setTimeout` ID is now stored and cleared in both `stopChatTalk()` and `cleanupChatTalk()`, preventing stale callbacks from firing after manual stop.

### ~~16. Hardcoded username "Eli" in greeting~~ ✅ FIXED
- **File:** `src/views/today/molecules.ts` L206
- **Bug:** Every user sees "Hello, Eli" on the dashboard.
- **Fix:** Replaced hardcoded name with `localStorage.getItem('paw-user-name')`. When set, greeting shows "Good morning, Name"; when unset, shows just "Good morning". Name is HTML-escaped via `escHtml()`.

### ~~17. `rm` in default command allowlist~~ ✅ FIXED
- **File:** `src/security.ts` L309-315
- **Bug:** `'^rm\\b'` auto-approves all `rm` commands even though `rm -rf /` is classified as critical. Allowlist vs danger-pattern precedence is undefined.
- **Fix:** Removed `rm` from the default allowlist. The HIL modal code already checks `!risk` before applying the allowlist (so `rm -rf /` was caught as critical first), but even plain `rm file.txt` is a destructive operation that should require explicit user approval. Users can still add `^rm\\b` to their personal allowlist if desired.

### ~~18. Exfiltration tools in safe_tools~~ ✅ FIXED
- **Rust channel bridge auto-approves `email_send`, `slack_send`, `webhook_send` — data exfiltration vectors.
- **Fix:** Removed `email_send`, `slack_send`, `webhook_send`, and `rest_api_call` from the auto-approved `safe_tools` lists in both the boss and worker orchestrator loops. These tools now require HIL approval. Read-only counterparts (`email_read`, `slack_read`) remain auto-approved.

### ~~19. Path traversal in filesystem tools~~ ✅ FIXED
- **Rust filesystem tools accept absolute paths with no sandbox enforcement. Agents can read/write anywhere on the host.
- **Fix:** Added `resolve_and_validate()` function to `filesystem.rs` that all 5 tools (read_file, write_file, list_directory, append_file, delete_file) now call. It canonicalizes paths (resolving `..` and symlinks), blocks `..` traversal that would escape the agent workspace, and checks against a `SENSITIVE_PATHS` deny-list covering credential stores (`.ssh`, `.gnupg`, `.aws/credentials`, etc.), system files (`/etc/shadow`, `/etc/passwd`), and engine internals.

### ~~20. DB init failure silently swallowed~~ ✅ FIXED
- Auto-retries 3× with backoff; shows red error banner with manual Retry button; skips encryption/security init if DB unavailable.

### ~~21. Duplicate / dead error boundary~~ ✅ FIXED
- Wired up `installErrorBoundary()` from `error-boundary.ts` in `main.ts` at module level. Removed duplicate inline `window.addEventListener` handlers. Connected `setErrorHandler()` callback to `crashLog()` for localStorage crash persistence.

### ~~22. Event listeners re-bound on every render~~ ✅ FIXED
- Audited all view files; only 2 had actual leaks on static DOM elements (most views use innerHTML replacement which is safe). Added boolean bind-once guards to: `initPalaceTabs`, `initPalaceRecall`, `initPalaceRemember`, `initPalaceGraph` (molecules/graph.ts), `initPalaceInstall` (index.ts), and `agents-create-btn` binding (agents/molecules.ts). Follows existing `_refreshBound` pattern from trading view.

---

## Tier 3 — Code Quality

### ~~23. 7 functions over 100 lines~~ ✅ FIXED
- Decomposed all 7 oversized functions: `loadChannels` → extracted `renderPendingSection`/`renderWhatsAppSection`; `openAgentEditor` → extracted `buildEditorHtml`/`wireToolPolicyUI`/`wireEditorBoundaries`/`handleEditorSave`; `renderMessages` → extracted `renderSingleMessage`/`renderScreenshotCard`/`renderAttachmentStrip`; `sendMessage` → extracted `buildSlashContext`/`encodeFileAttachments`/`handleSendResult`; `runMigrations` → versioned migration runner; Rust `execute_boss_tool` → 5 handler functions (`handle_delegate_task`, `handle_check_agent_status`, `handle_send_agent_message`, `handle_project_complete`, `handle_create_sub_agent`). All functions now under 100 lines.

### ~~24. No migration versioning~~ ✅ FIXED
- Added `schema_version` table (auto-created on first run). Migrations are now a typed `Migration[]` array with sequential version numbers. `runMigrations()` reads `MAX(version)` from `schema_version`, only runs pending migrations, wraps each in a `BEGIN/COMMIT` transaction with `ROLLBACK` on failure, and records the version + description + timestamp. Existing `CREATE TABLE IF NOT EXISTS` statements become migration v1 (idempotent on existing DBs). Future `ALTER TABLE` changes just append a new version entry.

### 25. ~~`confirm()`/`alert()` used in Tauri context (8 files)~~ ✅ FIXED
- Native `confirm()` / `alert()` may not render in Tauri webview context.
- **Fix:** Replace with Tauri dialog API or custom modal.
- Added `confirmModal()` in `helpers.ts` — a Promise-based custom modal (`#confirm-modal` in `index.html`) with keyboard support (Enter/Escape) and backdrop dismiss. Replaced all 21 `confirm()` calls across 14 files with `await confirmModal()`. Replaced all 11 `alert()` calls (4 files) with `showToast(message, 'error')`. Zero native dialog usage remains.

### 26. ~~`setTimeout` as polling substitute (10+ instances)~~ ✅ FIXED
- Arbitrary `setTimeout` delays instead of listening for backend completion events.
- **Fix:** Use Tauri events or IPC callbacks.
- Audited all 32 `setTimeout` calls. Identified 13 polling-pattern instances where an IPC call was already `await`ed but a blind `setTimeout` delayed before refreshing the UI. Replaced all 13 with immediate reload calls (`loadChannels()`, `loadCron()`, `loadPalaceStats()`, `loadTailscaleSettings()`, `_loadMail()`, `renderToday()`). Remaining 19 `setTimeout` calls are legitimate: CSS animation delays, button-revert timeouts, toast auto-dismiss, streaming safety timeouts, recording-cycle timing, retry backoff, and event-bus grace periods.

### 27. ~~`setInterval` without view-switch cleanup (3 views)~~ ✅ FIXED
- Orchestrator, settings, tasks views start intervals that aren't cleared on navigation.
- **Fix:** Clear intervals when view unmounts.
- Audited all 4 `setInterval` usages. Orchestrator message-poll (3s) and settings override-banner (30s) were view-specific but never cleared on navigation. Added `stopMessagePoll()` to orchestrator/index.ts and `stopOverrideBannerInterval()` to settings-main/index.ts. Wired both into `switchView()` in router.ts so they clear when navigating away. Settings usage-refresh was already handled. Tasks cron timer is intentionally global (runs cron scheduling regardless of active view) — left as-is.

### 28. ~~Hardcoded model prices~~ ✅ FIXED
- **File:** `src/state/index.ts` L30-80
- Model context sizes and per-token costs will become stale.
- **Fix:** Fetch from config, backend, or a remote source.
- Added `model_pricing` SQLite table (migration v2) with columns: `model_key`, `context_size`, `cost_input`, `cost_output`. Added CRUD functions (`listModelPricing`, `upsertModelPricing`, `deleteModelPricing`) in `db.ts`. Refactored `MODEL_CONTEXT_SIZES` and `MODEL_COST_PER_TOKEN` from immutable `const` to mutable `let` maps initialized from built-in defaults. New `applyModelPricingOverrides()` merges DB rows on top of defaults. Called at app init in `main.ts` after DB is ready. Users/admins can now update model pricing via DB without code changes; built-in defaults remain as fallback.

### 29. ~~`INSERT OR REPLACE` resets created_at~~ ✅ FIXED
- **File:** `src/db.ts` L293-311
- `INSERT OR REPLACE` deletes and re-creates the row, losing `created_at`.
- **Fix:** Use `INSERT ... ON CONFLICT ... DO UPDATE`.
- Converted all 4 `INSERT OR REPLACE` statements (`saveMode`, `saveProject`, `saveDoc`, `saveProjectFile`) to proper `INSERT ... ON CONFLICT(id) DO UPDATE SET ...` upserts. Each now explicitly updates only the mutable columns, preserving the original `created_at` timestamp set by the `DEFAULT (datetime('now'))` on first insert.

### 30. ~~No file/network log transport~~ ✅ FIXED
- **File:** `src/logger.ts`
- Logs go only to console and in-memory buffer. No way to retrieve after crash.
- **Fix:** Add file transport or Tauri-side log persistence.
- Added pluggable transport hook: `setLogTransport(fn)` receives every log entry that passes the level filter, with both the structured `LogEntry` and a pre-formatted string. Added `flushBufferToTransport()` to replay pre-init logs. In `main.ts`, wired a file transport after startup that writes to `~/Documents/Paw/logs/paw-YYYY-MM-DD.log` via `@tauri-apps/plugin-fs`, with batched I/O (1s flush interval) and automatic 7-day log rotation. 10 new tests (360 total).

### 31. ~~Singleton prevents test mocking~~ ✅ FIXED
- **File:** `src/engine/molecules/ipc_client.ts` L735
- `export const pawEngine = new PawEngineClient()` at module level — no mock seam for tests.
- **Fix:** Export a factory function or accept dependency injection.
- Exported the `PawEngineClient` class (was private) and added `createPawEngine()` factory function. The default `pawEngine` singleton is now created via the factory. Tests can import `createPawEngine()` or `PawEngineClient` directly for mocking/DI. Both are re-exported through the `src/engine.ts` barrel. All 40+ existing consumers continue to use the singleton unchanged.

### 32. ~~`activeStreams` Map never bounded~~ ✅ FIXED
- **File:** `src/state/index.ts` L100
- On error paths the map entry may not be deleted, causing slow memory leak over long sessions.
- **Fix:** Add cleanup sweep or bounded map.
- Added `createdAt` timestamp to `StreamState`. New `sweepStaleStreams()` function evicts entries older than 10 minutes (matching the streaming timeout) and enforces a hard cap of 50 entries. Stale entries have their timeouts cleared and resolve callbacks invoked before removal. Sweep runs automatically each time a new stream is created in `showStreamingMessage()`.

### 33. Token cost drift
- **File:** `src/engine/organisms/chat_controller.ts` L340
- `recordTokenUsage` replaces `sessionInputTokens` but adds to `sessionOutputTokens`. Over multiple turns, running total becomes inaccurate.
- **Fix:** Accumulate both consistently.

---

## Docs Site Fixes

### 34. Remove false feature claims
- `docs/docs/channels/nostr.md` L31 — remove "NIP-04 encrypted DMs"
- `docs/docs/channels/matrix.md` L35 — remove "End-to-end encryption support"

### 35. ~~Fix incorrect counts~~ ✅
- ~~Channel count: 10 → 11 (in `getting-started.md`, `architecture.md`, `SECURITY.md`)~~
- ~~Skill count: 37 → 40 (in `getting-started.md`)~~
- ~~Settings tabs: 12 → 11 (in `architecture.md`)~~
- ~~Trading tools: 8 → 7 (in `trading.md`)~~

### 36. ~~Fix architecture.md view paths~~ ✅
- ~~All view paths listed as `views/today.ts` etc — should be `views/today/` (directories with atoms/molecules/index)~~
- ~~`web.rs` listed as channel bridge — it's the web scraping module~~
- ~~`whatsapp.rs` missing from channel bridge list~~

### 37. ~~Fix projects.md sensitive paths list~~ ✅
- ~~Current list (`.azure`, `.gcloud`, `.npmrc`, etc.) doesn't match actual code~~
- ~~Real patterns: `.gnome-keyring`, `.password-store`, `/dev`, `.openclaw`, Windows paths, etc.~~

### 38. ~~Fix injection category count~~ ✅
- ~~Docs say 9 categories — Rust scanner has 8 (no `obfuscation`)~~
- ~~Document the frontend/backend scanner distinction~~

### 39. ~~Update pricing table~~ ✅
- ~~Missing: `claude-haiku-4`, `o4-mini`, `o3-mini`, `deepseek-reasoner`, all `gpt-4.1` variants~~

### 40. ~~Fix button text inconsistency~~ ✅
- ~~`first-agent.md` says "Create Agent", `agents.md` says "New Agent" — standardize~~

### 41. ~~Add missing docs~~ ✅
- ~~WhatsApp channel guide (engine module exists, no docs page)~~ — already exists (93 lines)
- ~~`gpt-4.1` model family documentation~~ — added to pricing table
- ~~`claude-haiku-4` model documentation~~ — added to pricing table

---

## Stats

| Severity | Count |
|----------|-------|
| Critical | 6 |
| High | 16 |
| Medium / Quality | 11 |
| Docs | 8 |
| **Total** | **41** |
