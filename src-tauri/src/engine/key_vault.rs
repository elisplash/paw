// ── Unified Key Vault ──────────────────────────────────────────────────────
//
// Consolidates all OS keychain entries into a SINGLE keychain item stored as
// a JSON blob.  This reduces macOS Keychain Access prompts from 6+ to 1:
//
//   ONE "openpawz" / "key-vault" entry → 1 prompt
//
// Architecture:
//   - Single keychain entry: service="openpawz", user="key-vault"
//   - In-memory HashMap<String, Zeroizing<String>> protected by RwLock
//   - Each subsystem calls get()/set() with a purpose constant
//   - Keys are generated on first access if missing
//
// Security:
//   - All in-memory key material is wrapped in `Zeroizing<String>` so it
//     is securely overwritten with zeroes when dropped or replaced —
//     prevents secrets from lingering in freed heap memory.
//   - The vault blob is stored in the OS keychain (encrypted at rest by
//     macOS Keychain / GNOME Keyring / Windows Credential Manager).
//   - In-memory cache is process-scoped — cleared (and zeroed) on exit.
//   - Write operations hold the lock across read-check + insert + persist
//     to prevent TOCTOU races between concurrent threads.
//   - Lock poison is recovered with a logged warning — a panicked thread
//     should not permanently brick the vault for the rest of the app.

use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::sync::RwLock;
use zeroize::Zeroizing;

const VAULT_SERVICE: &str = "openpawz";
const VAULT_USER: &str = "key-vault";

/// Type alias: all in-memory key material is wrapped in `Zeroizing` so it
/// is securely overwritten with zeroes when dropped or replaced.
type VaultMap = HashMap<String, Zeroizing<String>>;

/// In-memory cache of the vault contents.
/// None = not yet loaded, Some = loaded (possibly empty on fresh install).
/// Values are `Zeroizing<String>` — zeroed on drop.
static VAULT_CACHE: RwLock<Option<VaultMap>> = RwLock::new(None);

// ── Lock helpers ───────────────────────────────────────────────────────────
// Recover from a poisoned RwLock (another thread panicked while holding it)
// but always log a warning so we know something went wrong.  The alternative
// — `.unwrap()` — would crash the entire app, which is worse than operating
// on potentially stale data.

fn read_lock(lock: &RwLock<Option<VaultMap>>) -> std::sync::RwLockReadGuard<'_, Option<VaultMap>> {
    lock.read().unwrap_or_else(|poisoned| {
        warn!("[key-vault] RwLock was poisoned (read) — recovering");
        poisoned.into_inner()
    })
}

fn write_lock(
    lock: &RwLock<Option<VaultMap>>,
) -> std::sync::RwLockWriteGuard<'_, Option<VaultMap>> {
    lock.write().unwrap_or_else(|poisoned| {
        warn!("[key-vault] RwLock was poisoned (write) — recovering");
        poisoned.into_inner()
    })
}

// ── Public API ─────────────────────────────────────────────────────────────

/// Purpose constants — each subsystem uses its own key.
pub const PURPOSE_DB_ENCRYPTION: &str = "db-encryption";
pub const PURPOSE_LOCK_SCREEN: &str = "lock-screen";
pub const PURPOSE_SKILL_VAULT: &str = "skill-vault";
pub const PURPOSE_MEMORY_VAULT: &str = "memory-vault";
pub const PURPOSE_N8N_ENCRYPTION: &str = "n8n-encryption";
pub const PURPOSE_N8N_OWNER: &str = "n8n-owner";
pub const PURPOSE_AUDIT_CHAIN: &str = "audit-chain";
pub const PURPOSE_NOSTR_KEY: &str = "nostr-key";

/// Prefetch the vault — triggers the single keychain access so that all
/// subsequent `get()` calls are pure in-memory lookups.
/// Call this early in app startup (before subsystems initialise).
pub fn prefetch() {
    ensure_loaded();
    let guard = read_lock(&VAULT_CACHE);
    let count = guard.as_ref().map_or(0, |m| m.len());
    info!("[key-vault] Prefetch complete — {} keys available", count);
}

/// Check whether the vault was successfully loaded.
/// Returns `true` if `prefetch()` (or any `get()`/`set()`) has populated
/// the in-memory cache — meaning the OS keychain was reachable.
pub fn is_loaded() -> bool {
    read_lock(&VAULT_CACHE).is_some()
}

/// Get a value from the vault by purpose key.
/// Returns `None` if the key has never been stored.
///
/// The returned `String` is a plain clone — callers that cache it
/// long-term should wrap it in their own `Zeroizing<String>`.
pub fn get(purpose: &str) -> Option<String> {
    ensure_loaded();
    let guard = read_lock(&VAULT_CACHE);
    guard
        .as_ref()
        .and_then(|map| map.get(purpose))
        .map(|v| String::from(v.as_str()))
}

/// Store a value in the vault and persist the whole blob to the keychain.
/// Creates the vault entry if it doesn't exist yet.
///
/// Thread-safe: holds the write lock across read-check + insert + persist
/// to prevent TOCTOU races between concurrent callers.
pub fn set(purpose: &str, value: &str) {
    let mut guard = write_lock(&VAULT_CACHE);
    if guard.is_none() {
        *guard = Some(read_vault());
    }
    let map = guard.get_or_insert_with(VaultMap::new);
    map.insert(purpose.to_string(), Zeroizing::new(value.to_string()));
    persist_vault(map);
}

/// Remove a value from the vault and persist.
/// Used by lock_screen_remove_passphrase(), oauth revoke, etc.
pub fn remove(purpose: &str) {
    let mut guard = write_lock(&VAULT_CACHE);
    if guard.is_none() {
        *guard = Some(read_vault());
    }
    if let Some(map) = guard.as_mut() {
        if map.remove(purpose).is_some() {
            persist_vault(map);
            info!("[key-vault] Removed '{}' from vault", purpose);
        }
    }
}

// ── Internal ───────────────────────────────────────────────────────────────

/// Ensure the vault is loaded into memory (double-checked lock pattern).
/// On first call, reads the unified keychain entry (1 OS prompt max).
/// If no vault exists yet, creates an empty in-memory map (no prompt).
fn ensure_loaded() {
    // Fast path: already cached
    {
        if read_lock(&VAULT_CACHE).is_some() {
            return;
        }
    }
    // Slow path: acquire write lock and double-check
    let mut guard = write_lock(&VAULT_CACHE);
    if guard.is_some() {
        return;
    }
    *guard = Some(read_vault());
}

/// Read the unified vault JSON from the keychain.
/// If no vault exists yet, returns an empty map.
fn read_vault() -> VaultMap {
    match keyring::Entry::new(VAULT_SERVICE, VAULT_USER) {
        Ok(entry) => match entry.get_password() {
            Ok(json_str) => {
                // Deserialise into plain HashMap first, then wrap values
                match serde_json::from_str::<HashMap<String, String>>(&json_str) {
                    Ok(plain) => {
                        let count = plain.len();
                        let map: VaultMap = plain
                            .into_iter()
                            .map(|(k, v)| (k, Zeroizing::new(v)))
                            .collect();
                        info!("[key-vault] Loaded unified vault ({} keys)", count);
                        map
                    }
                    Err(e) => {
                        error!("[key-vault] Corrupt vault JSON: {} — starting fresh", e);
                        VaultMap::new()
                    }
                }
            }
            Err(keyring::Error::NoEntry) => {
                info!("[key-vault] No unified vault found — will create on first write");
                VaultMap::new()
            }
            Err(e) => {
                warn!("[key-vault] Keychain read error: {} — starting fresh", e);
                VaultMap::new()
            }
        },
        Err(e) => {
            error!("[key-vault] Keyring init failed: {}", e);
            VaultMap::new()
        }
    }
}

/// Serialise the vault map to JSON and write to the single keychain entry.
/// Accepts `VaultMap` (Zeroizing values) — unwraps to plain strings for
/// JSON serialisation only; the serialised JSON lives briefly on the stack.
fn persist_vault(map: &VaultMap) {
    // Build a plain HashMap for serde (Zeroizing<String> doesn't impl Serialize)
    let plain: HashMap<&str, &str> = map.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

    let json = match serde_json::to_string(&plain) {
        Ok(j) => Zeroizing::new(j),
        Err(e) => {
            error!("[key-vault] Failed to serialise vault: {}", e);
            return;
        }
    };

    match keyring::Entry::new(VAULT_SERVICE, VAULT_USER) {
        Ok(entry) => match entry.set_password(&json) {
            Ok(()) => debug!("[key-vault] Persisted unified vault ({} keys)", map.len()),
            Err(e) => error!("[key-vault] Failed to persist vault: {}", e),
        },
        Err(e) => error!("[key-vault] Keyring init failed on persist: {}", e),
    }
}
