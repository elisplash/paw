// Paw Agent Engine — Nostr Bridge
//
// Connects Paw to the Nostr network via outbound WebSocket to relay(s).
// The bot subscribes to mentions and DMs, then publishes signed reply events.
//
// Setup: Generate or import a Nostr keypair → configure relay URL(s) → enable.
//        The private key is stored in the OS keychain (macOS Keychain /
//        Windows Credential Manager / Linux Secret Service), never in the config DB.
//
// Protocol:
//   - NIP-01: Basic event subscription + publishing
//   - NIP-04: Encrypted direct messages (ECDH + AES-256-CBC)
//   - kind 1 (text notes): Respond to @mentions in public
//   - kind 4 (encrypted DMs): Decrypt incoming, encrypt outgoing replies
//   - Events are signed with secp256k1 Schnorr (BIP-340) via the k256 crate
//
// Security:
//   - Private key stored in OS keychain, never in the config DB
//   - DM content encrypted end-to-end via ECDH shared secret
//   - Allowlist by npub / hex pubkey
//   - Optional pairing mode
//   - All communication through relay TLS WebSockets
//
// Future: NIP-44 (ChaCha20 + HMAC-SHA256) with NIP-17 gift wrapping for
//         improved metadata privacy. Kind-4 NIP-04 DMs remain widely supported.

mod crypto;
mod relay;

use crate::engine::channels::{self, PendingUser, ChannelStatus};
use crypto::{hex_decode, hex_encode, derive_pubkey};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use crate::atoms::error::{EngineResult, EngineError};

// ── Nostr Config ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NostrConfig {
    /// Hex-encoded private key (64 hex chars = 32 bytes)
    /// DO NOT use nsec format — convert to hex first
    pub private_key_hex: String,
    /// Relay URLs (e.g. ["wss://relay.damus.io", "wss://nos.lol"])
    pub relays: Vec<String>,
    pub enabled: bool,
    /// "open" | "allowlist" | "pairing"
    pub dm_policy: String,
    /// Hex pubkeys of allowed users
    pub allowed_users: Vec<String>,
    #[serde(default)]
    pub pending_users: Vec<PendingUser>,
    pub agent_id: Option<String>,
}

impl Default for NostrConfig {
    fn default() -> Self {
        NostrConfig {
            private_key_hex: String::new(),
            relays: vec!["wss://relay.damus.io".into(), "wss://nos.lol".into()],
            enabled: false,
            dm_policy: "open".into(),
            allowed_users: vec![],
            pending_users: vec![],
            agent_id: None,
        }
    }
}

// ── Global State ───────────────────────────────────────────────────────

static BRIDGE_RUNNING: AtomicBool = AtomicBool::new(false);
static MESSAGE_COUNT: AtomicI64 = AtomicI64::new(0);
static BOT_PUBKEY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
static STOP_SIGNAL: std::sync::OnceLock<Arc<AtomicBool>> = std::sync::OnceLock::new();

fn get_stop_signal() -> Arc<AtomicBool> {
    STOP_SIGNAL.get_or_init(|| Arc::new(AtomicBool::new(false))).clone()
}

const CONFIG_KEY: &str = "nostr_config";

// ── Keychain Helpers ───────────────────────────────────────────────────

const KEYRING_SERVICE: &str = "paw-nostr";
const KEYRING_USER: &str = "private-key";

/// Store the Nostr private key in the OS keychain.
fn keychain_set_private_key(hex_key: &str) -> EngineResult<()> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| EngineError::Keyring(e.to_string()))?;
    entry.set_password(hex_key)
        .map_err(|e| EngineError::Keyring(e.to_string()))?;
    info!("[nostr] Private key stored in OS keychain");
    Ok(())
}

/// Retrieve the Nostr private key from the OS keychain.
fn keychain_get_private_key() -> EngineResult<Option<String>> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| EngineError::Keyring(e.to_string()))?;
    match entry.get_password() {
        Ok(key) if !key.is_empty() => Ok(Some(key)),
        Ok(_) => Ok(None),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(EngineError::Keyring(e.to_string())),
    }
}

/// Delete the Nostr private key from the OS keychain.
#[allow(dead_code)]
fn keychain_delete_private_key() -> EngineResult<()> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| EngineError::Keyring(e.to_string()))?;
    match entry.delete_credential() {
        Ok(()) => { info!("[nostr] Private key removed from OS keychain"); Ok(()) }
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(EngineError::Keyring(e.to_string())),
    }
}

// ── Bridge Core ────────────────────────────────────────────────────────

pub fn start_bridge(app_handle: tauri::AppHandle) -> EngineResult<()> {
    if BRIDGE_RUNNING.load(Ordering::Relaxed) {
        return Err("Nostr bridge is already running".into());
    }

    let config: NostrConfig = channels::load_channel_config(&app_handle, CONFIG_KEY)?;
    if config.private_key_hex.is_empty() {
        return Err("Private key (hex) is required.".into());
    }
    if config.relays.is_empty() {
        return Err("At least one relay URL is required.".into());
    }
    if !config.enabled {
        return Err("Nostr bridge is disabled.".into());
    }

    // Validate and derive pubkey from private key
    let sk_bytes = hex_decode(&config.private_key_hex)
        .map_err(|_| "Invalid private key hex")?;
    if sk_bytes.len() != 32 {
        return Err("Private key must be 32 bytes (64 hex chars)".into());
    }

    let pubkey = derive_pubkey(&sk_bytes)?;
    let pubkey_hex = hex_encode(&pubkey);
    let _ = BOT_PUBKEY.set(pubkey_hex.clone());
    info!("[nostr] Bot pubkey: {}", pubkey_hex);

    let stop = get_stop_signal();
    stop.store(false, Ordering::Relaxed);
    BRIDGE_RUNNING.store(true, Ordering::Relaxed);

    tauri::async_runtime::spawn(async move {
        // Connect to all relays in parallel
        let mut handles = vec![];
        for relay in &config.relays {
            let app = app_handle.clone();
            let cfg = config.clone();
            let relay_url = relay.clone();
            let pk_hex = pubkey_hex.clone();
            let sk = sk_bytes.clone();
            let handle = tauri::async_runtime::spawn(async move {
                let mut attempt: u32 = 0;
                loop {
                    if get_stop_signal().load(Ordering::Relaxed) { break; }
                    match relay::run_relay_loop(&app, &cfg, &relay_url, &pk_hex, &sk).await {
                        Ok(()) => { attempt = 0; }
                        Err(e) => {
                            warn!("[nostr] Relay {} error: {}", relay_url, e);
                        }
                    }
                    if get_stop_signal().load(Ordering::Relaxed) { break; }
                    let delay = crate::engine::http::reconnect_delay(attempt).await;
                    debug!("[nostr] Relay {} reconnect in {}ms (attempt {})", relay_url, delay.as_millis(), attempt + 1);
                    attempt += 1;
                }
            });
            handles.push(handle);
        }

        // Wait for all relay tasks (they loop until stop)
        for h in handles {
            let _ = h.await;
        }

        BRIDGE_RUNNING.store(false, Ordering::Relaxed);
        info!("[nostr] Bridge stopped");
    });

    Ok(())
}

pub fn stop_bridge() {
    let stop = get_stop_signal();
    stop.store(true, Ordering::Relaxed);
    BRIDGE_RUNNING.store(false, Ordering::Relaxed);
    info!("[nostr] Stop signal sent");
}

pub fn get_status(app_handle: &tauri::AppHandle) -> ChannelStatus {
    let config: NostrConfig = channels::load_channel_config(app_handle, CONFIG_KEY).unwrap_or_default();
    ChannelStatus {
        running: BRIDGE_RUNNING.load(Ordering::Relaxed),
        connected: BRIDGE_RUNNING.load(Ordering::Relaxed),
        bot_name: BOT_PUBKEY.get().map(|pk| format!("{}...", &pk[..12])),
        bot_id: BOT_PUBKEY.get().cloned(),
        message_count: MESSAGE_COUNT.load(Ordering::Relaxed) as u64,
        allowed_users: config.allowed_users,
        pending_users: config.pending_users,
        dm_policy: config.dm_policy,
    }
}

// ── Config Persistence ─────────────────────────────────────────────────

pub fn load_config(app_handle: &tauri::AppHandle) -> EngineResult<NostrConfig> {
    let mut config: NostrConfig = channels::load_channel_config(app_handle, CONFIG_KEY)?;

    // Hydrate private key from OS keychain
    if let Ok(Some(key)) = keychain_get_private_key() {
        config.private_key_hex = key;
    }

    // Auto-migrate: if DB still has a plaintext key, move it to keychain
    if !config.private_key_hex.is_empty() {
        let mut db_config: NostrConfig = channels::load_channel_config(app_handle, CONFIG_KEY)?;
        if !db_config.private_key_hex.is_empty() {
            // Key is still in the DB — migrate it to keychain and clear from DB
            if keychain_set_private_key(&db_config.private_key_hex).is_ok() {
                db_config.private_key_hex = String::new();
                let _ = channels::save_channel_config(app_handle, CONFIG_KEY, &db_config);
                info!("[nostr] Migrated private key from config DB to OS keychain");
            }
        }
    }

    Ok(config)
}

pub fn save_config(app_handle: &tauri::AppHandle, config: &NostrConfig) -> EngineResult<()> {
    let mut config = config.clone();

    // If a private key is being saved, store it in the OS keychain
    // and clear it from the config struct before persisting to DB
    if !config.private_key_hex.is_empty() {
        keychain_set_private_key(&config.private_key_hex)?;
        config.private_key_hex = String::new();
    }

    channels::save_channel_config(app_handle, CONFIG_KEY, &config)
}

pub fn approve_user(app_handle: &tauri::AppHandle, user_id: &str) -> EngineResult<()> {
    channels::approve_user_generic(app_handle, CONFIG_KEY, user_id)
}

pub fn deny_user(app_handle: &tauri::AppHandle, user_id: &str) -> EngineResult<()> {
    channels::deny_user_generic(app_handle, CONFIG_KEY, user_id)
}

pub fn remove_user(app_handle: &tauri::AppHandle, user_id: &str) -> EngineResult<()> {
    channels::remove_user_generic(app_handle, CONFIG_KEY, user_id)
}
