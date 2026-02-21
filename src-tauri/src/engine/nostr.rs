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
//   - kind 1 (text notes): Respond to @mentions in public
//   - kind 4 (encrypted DMs): NOT supported yet (needs NIP-04/NIP-44 crypto)
//   - Events are signed with secp256k1 Schnorr (BIP-340) via the k256 crate
//
// Security:
//   - Private key stored in OS keychain, never in the config DB
//   - Allowlist by npub / hex pubkey
//   - Optional pairing mode
//   - All communication through relay TLS WebSockets

use crate::engine::channels::{self, PendingUser, ChannelStatus};
use log::{debug, info, warn, error};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use tauri::Emitter;
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};
use futures::{SinkExt, StreamExt};

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
fn keychain_set_private_key(hex_key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| format!("Keyring init failed: {}", e))?;
    entry.set_password(hex_key)
        .map_err(|e| format!("Failed to store Nostr key in keychain: {}", e))?;
    info!("[nostr] Private key stored in OS keychain");
    Ok(())
}

/// Retrieve the Nostr private key from the OS keychain.
fn keychain_get_private_key() -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| format!("Keyring init failed: {}", e))?;
    match entry.get_password() {
        Ok(key) if !key.is_empty() => Ok(Some(key)),
        Ok(_) => Ok(None),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Keyring error: {}", e)),
    }
}

/// Delete the Nostr private key from the OS keychain.
#[allow(dead_code)]
fn keychain_delete_private_key() -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| format!("Keyring init failed: {}", e))?;
    match entry.delete_credential() {
        Ok(()) => { info!("[nostr] Private key removed from OS keychain"); Ok(()) }
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Keyring delete failed: {}", e)),
    }
}

// ── Bridge Core ────────────────────────────────────────────────────────

pub fn start_bridge(app_handle: tauri::AppHandle) -> Result<(), String> {
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
                loop {
                    if get_stop_signal().load(Ordering::Relaxed) { break; }
                    if let Err(e) = run_relay_loop(&app, &cfg, &relay_url, &pk_hex, &sk).await {
                        warn!("[nostr] Relay {} error: {}", relay_url, e);
                    }
                    if get_stop_signal().load(Ordering::Relaxed) { break; }
                    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
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

// ── Single Relay WebSocket Loop ────────────────────────────────────────

async fn run_relay_loop(
    app_handle: &tauri::AppHandle,
    config: &NostrConfig,
    relay_url: &str,
    pubkey_hex: &str,
    secret_key: &[u8],
) -> Result<(), String> {
    let stop = get_stop_signal();

    let (ws_stream, _) = connect_async(relay_url).await
        .map_err(|e| format!("WS connect to {}: {}", relay_url, e))?;
    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    info!("[nostr] Connected to relay {}", relay_url);

    let _ = app_handle.emit("nostr-status", json!({
        "kind": "connected",
        "relay": relay_url,
    }));

    // Subscribe to events mentioning our pubkey (NIP-01)
    // kind 1 = text notes with #p tag pointing to us
    let sub_id = format!("paw-{}", &pubkey_hex[..8]);
    let req = json!(["REQ", &sub_id, {
        "#p": [pubkey_hex],
        "kinds": [1],
        "since": chrono::Utc::now().timestamp() - 10, // Only new events
    }]);
    ws_tx.send(WsMessage::Text(req.to_string())).await
        .map_err(|e| format!("REQ send: {}", e))?;

    let mut current_config = config.clone();
    let mut last_config_reload = std::time::Instant::now();
    let mut seen_events: std::collections::HashSet<String> = std::collections::HashSet::new();

    loop {
        if stop.load(Ordering::Relaxed) { break; }

        let msg = tokio::select! {
            msg = ws_rx.next() => msg,
            _ = tokio::time::sleep(std::time::Duration::from_secs(60)) => {
                // Keepalive: re-subscribe to refresh
                continue;
            }
        };

        let text = match msg {
            Some(Ok(WsMessage::Text(t))) => t,
            Some(Ok(WsMessage::Close(_))) => break,
            Some(Err(e)) => {
                warn!("[nostr] WS error from {}: {}", relay_url, e);
                break;
            }
            None => break,
            _ => continue,
        };

        // Nostr messages are JSON arrays: ["EVENT", sub_id, event] or ["EOSE", sub_id] etc
        let arr: Vec<serde_json::Value> = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if arr.is_empty() { continue; }

        let msg_type = arr[0].as_str().unwrap_or("");

        match msg_type {
            "EVENT" => {
                if arr.len() < 3 { continue; }
                let event = &arr[2];

                let event_id = event["id"].as_str().unwrap_or("").to_string();
                if event_id.is_empty() { continue; }

                // Dedup
                if seen_events.contains(&event_id) { continue; }
                seen_events.insert(event_id.clone());
                // Limit dedup set size
                if seen_events.len() > 10000 {
                    seen_events.clear();
                }

                let kind = event["kind"].as_u64().unwrap_or(0);
                if kind != 1 { continue; } // Only text notes for now

                let sender_pk = event["pubkey"].as_str().unwrap_or("").to_string();
                if sender_pk == pubkey_hex { continue; } // Skip own events

                let content = event["content"].as_str().unwrap_or("").to_string();
                if content.is_empty() { continue; }

                debug!("[nostr] Event from {}...{}: {}",
                    &sender_pk[..8], &sender_pk[sender_pk.len()-4..],
                    if content.len() > 50 { format!("{}...", &content[..50]) } else { content.clone() });

                // Access control
                match channels::check_access(
                    &current_config.dm_policy,
                    &sender_pk,
                    &sender_pk[..12],
                    &sender_pk[..12],
                    &current_config.allowed_users,
                    &mut current_config.pending_users,
                ) {
                    Err(_denial_msg) => {
                        let _ = channels::save_channel_config(app_handle, CONFIG_KEY, &current_config);
                        let _ = app_handle.emit("nostr-status", json!({
                            "kind": "pairing_request",
                            "pubkey": &sender_pk,
                        }));
                        // Don't reply to denied users on public Nostr
                        continue;
                    }
                    Ok(()) => {}
                }

                MESSAGE_COUNT.fetch_add(1, Ordering::Relaxed);

                let agent_id = current_config.agent_id.as_deref().unwrap_or("default");
                let ctx = "You are chatting via Nostr (a decentralized social network). \
                           Use plain text. Keep responses concise. \
                           Your reply will be published as a kind-1 note.";

                let response = channels::run_channel_agent(
                    app_handle, "nostr", ctx, &content, &sender_pk, agent_id,
                ).await;

                match response {
                    Ok(reply) if !reply.is_empty() => {
                        // Build and sign reply event
                        match build_reply_event(secret_key, pubkey_hex, &reply, &event_id, &sender_pk) {
                            Ok(reply_event) => {
                                // Publish: ["EVENT", event_json]
                                let publish = json!(["EVENT", reply_event]);
                                if let Err(e) = ws_tx.send(WsMessage::Text(publish.to_string())).await {
                                    warn!("[nostr] Failed to publish reply: {}", e);
                                }
                            }
                            Err(e) => {
                                error!("[nostr] Failed to sign reply: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("[nostr] Agent error for {}...{}: {}", &sender_pk[..8], &sender_pk[sender_pk.len()-4..], e);
                    }
                    _ => {}
                }
            }
            "EOSE" => {
                info!("[nostr] End of stored events from {}", relay_url);
            }
            "NOTICE" => {
                let notice = arr.get(1).and_then(|v| v.as_str()).unwrap_or("");
                warn!("[nostr] NOTICE from {}: {}", relay_url, notice);
            }
            "OK" => {
                // Event acceptance confirmation
                let accepted = arr.get(2).and_then(|v| v.as_bool()).unwrap_or(false);
                if !accepted {
                    let reason = arr.get(3).and_then(|v| v.as_str()).unwrap_or("");
                    warn!("[nostr] Event rejected by {}: {}", relay_url, reason);
                }
            }
            _ => {}
        }

        // Reload config
        if last_config_reload.elapsed() > std::time::Duration::from_secs(30) {
            if let Ok(fresh) = channels::load_channel_config::<NostrConfig>(app_handle, CONFIG_KEY) {
                current_config = fresh;
            }
            last_config_reload = std::time::Instant::now();
        }
    }

    let _ = app_handle.emit("nostr-status", json!({
        "kind": "disconnected",
        "relay": relay_url,
    }));

    Ok(())
}

// ── Nostr Event Signing (secp256k1 Schnorr / BIP-340) ─────────────────
//
// NIP-01 event structure:
//   id: sha256([0, pubkey, created_at, kind, tags, content])
//   sig: schnorr signature of id using secret key (via k256 crate)

fn build_reply_event(
    secret_key: &[u8],
    pubkey_hex: &str,
    content: &str,
    reply_to_id: &str,
    reply_to_pk: &str,
) -> Result<serde_json::Value, String> {
    use sha2::{Sha256, Digest};
    use k256::schnorr::SigningKey;

    let created_at = chrono::Utc::now().timestamp();
    let kind = 1u64;

    // Tags: reply to the event + mention the sender
    let tags = json!([
        ["e", reply_to_id, "", "reply"],
        ["p", reply_to_pk]
    ]);

    // Serialize for id computation: [0, pubkey, created_at, kind, tags, content]
    let serialized = json!([0, pubkey_hex, created_at, kind, tags, content]);
    let serialized_str = serde_json::to_string(&serialized)
        .map_err(|e| format!("serialize: {}", e))?;

    let mut hasher = Sha256::new();
    hasher.update(serialized_str.as_bytes());
    let id_bytes = hasher.finalize();
    let id_hex = hex_encode(&id_bytes);

    // BIP-340 Schnorr signature over the event id
    let signing_key = SigningKey::from_bytes(secret_key)
        .map_err(|e| format!("Invalid signing key: {}", e))?;
    let aux_rand: [u8; 32] = rand::random();
    let sig = signing_key.sign_raw(&id_bytes, &aux_rand)
        .map_err(|e| format!("Schnorr sign failed: {}", e))?;
    let sig_hex = hex_encode(&sig.to_bytes());

    Ok(json!({
        "id": id_hex,
        "pubkey": pubkey_hex,
        "created_at": created_at,
        "kind": kind,
        "tags": tags,
        "content": content,
        "sig": sig_hex,
    }))
}

// ── secp256k1 Pubkey Derivation (BIP-340 x-only) ──────────────────────
//
// Nostr uses the x-coordinate of the secp256k1 public key (BIP-340).
// We use the `k256` crate (already a dependency for DEX/Ethereum wallet)
// to perform proper elliptic curve point multiplication.

fn derive_pubkey(secret_key: &[u8]) -> Result<Vec<u8>, String> {
    use k256::elliptic_curve::sec1::ToEncodedPoint;

    let sk = k256::SecretKey::from_slice(secret_key)
        .map_err(|e| format!("Invalid secret key: {}", e))?;
    let pk = sk.public_key();
    let point = pk.to_encoded_point(true); // compressed
    // BIP-340 x-only: skip the 0x02/0x03 prefix byte, take the 32-byte x-coordinate
    let compressed = point.as_bytes();
    if compressed.len() != 33 {
        return Err("Unexpected compressed pubkey length".into());
    }
    Ok(compressed[1..].to_vec())
}

// ── Hex Utils ──────────────────────────────────────────────────────────

fn hex_decode(hex: &str) -> Result<Vec<u8>, String> {
    if hex.len() % 2 != 0 {
        return Err("Odd hex length".into());
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|e| format!("hex: {}", e)))
        .collect()
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

// ── Config Persistence ─────────────────────────────────────────────────

pub fn load_config(app_handle: &tauri::AppHandle) -> Result<NostrConfig, String> {
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

pub fn save_config(app_handle: &tauri::AppHandle, config: &NostrConfig) -> Result<(), String> {
    let mut config = config.clone();

    // If a private key is being saved, store it in the OS keychain
    // and clear it from the config struct before persisting to DB
    if !config.private_key_hex.is_empty() {
        keychain_set_private_key(&config.private_key_hex)?;
        config.private_key_hex = String::new();
    }

    channels::save_channel_config(app_handle, CONFIG_KEY, &config)
}

pub fn approve_user(app_handle: &tauri::AppHandle, user_id: &str) -> Result<(), String> {
    channels::approve_user_generic(app_handle, CONFIG_KEY, user_id)
}

pub fn deny_user(app_handle: &tauri::AppHandle, user_id: &str) -> Result<(), String> {
    channels::deny_user_generic(app_handle, CONFIG_KEY, user_id)
}

pub fn remove_user(app_handle: &tauri::AppHandle, user_id: &str) -> Result<(), String> {
    channels::remove_user_generic(app_handle, CONFIG_KEY, user_id)
}
