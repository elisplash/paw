// ── Paw Atoms: Constants ───────────────────────────────────────────────────
// All named constants for the crate live here.
// Rationale: collecting constants in one place eliminates magic strings,
// makes auditing easier, and keeps every layer's code self-documenting.

// ── Database encryption key identifiers ───────────────────────────────────
// Used by `get_db_encryption_key()` / `has_db_encryption_key()` in lib.rs.
// The keychain entry is keyed on (service, user) — changing either value
// would cause existing keys to become unreachable. Treat as stable identifiers.
pub(crate) const DB_KEY_SERVICE: &str = "paw-db-encryption";
pub(crate) const DB_KEY_USER: &str = "paw-db-key";

// ── Lock screen passphrase identifiers ────────────────────────────────────
// Used by `lock_screen_*` commands in commands/utility.rs.
// Stores a SHA-256 hash of the user's passphrase in the OS keychain.
pub(crate) const LOCK_SERVICE: &str = "paw-lock-screen";
pub(crate) const LOCK_USER: &str = "paw-lock-passphrase";

// ── Cron task execution cost-control limits ────────────────────────────────
// Used by `run_cron_heartbeat()` in engine/commands.rs.
//
// Background: cron sessions reuse the same session_id across runs, causing
// message history to grow unboundedly (up to 500 messages / 100k tokens).
// This is the #1 driver of runaway API costs in unattended execution.
// We prune old messages before each run and cap tool rounds.
pub(crate) const CRON_SESSION_KEEP_MESSAGES: i64 = 20; // keep ~2-3 runs of context
pub(crate) const CRON_MAX_TOOL_ROUNDS: u32 = 10; // prevent runaway tool loops

// ── Chat session message retention ─────────────────────────────────────
// After each chat turn, prune the session if it exceeds this many stored
// messages.  Only the most recent N messages are kept; older ones are deleted
// from the DB.  The agent still has access to past context via memory_search.
pub(crate) const CHAT_SESSION_MAX_MESSAGES: i64 = 200;

// ── Startup housekeeping ───────────────────────────────────────────────
// Sessions older than this with 0 messages are purged on startup.
pub(crate) const STARTUP_EMPTY_SESSION_MAX_AGE_SECS: i64 = 3600; // 1 hour
                                                                 // Sessions with no activity for this long have their messages pruned to
                                                                 // CHAT_SESSION_MAX_MESSAGES on startup.
pub(crate) const STARTUP_STALE_SESSION_MAX_AGE_DAYS: i64 = 30;
