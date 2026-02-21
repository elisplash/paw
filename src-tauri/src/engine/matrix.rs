// Paw Agent Engine — Matrix Bridge (E2EE via matrix-sdk + vodozemac)
//
// Connects Paw to Matrix using the official matrix-sdk crate which provides:
//   - Full Client-Server API support via /sync long-polling
//   - E2EE (Olm/Megolm) via vodozemac — automatic key exchange & session mgmt
//   - SQLite-backed crypto store for device keys, sessions, and room keys
//   - Automatic device verification and key sharing
//
// Setup: Register a bot account on any Matrix homeserver → get access token
//        → paste in Paw → start the channel. The bot auto-joins rooms on invite,
//        decrypts E2EE messages, and encrypts replies.
//
// Security:
//   - End-to-end encryption with Olm/Megolm (vodozemac)
//   - Crypto state persisted in SQLite at ~/Documents/Paw/matrix-store/
//   - Allowlist by Matrix user ID (@user:server.org)
//   - Optional pairing mode
//   - All communication through the homeserver's TLS API

use crate::engine::channels::{self, PendingUser, ChannelStatus};
use log::{debug, info, warn, error};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use tauri::Emitter;

use crate::atoms::error::EngineResult;
use matrix_sdk::{
    Client, Room,
    config::SyncSettings,
    authentication::matrix::MatrixSession,
    ruma::{
        OwnedUserId, OwnedDeviceId,
        events::room::{
            member::StrippedRoomMemberEvent,
            message::{
                MessageType, OriginalSyncRoomMessageEvent,
                RoomMessageEventContent,
            },
        },
    },
    SessionMeta,
};

// ── Matrix Config ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatrixConfig {
    /// Homeserver URL (e.g. "https://matrix.org")
    pub homeserver: String,
    /// Access token for the bot account
    pub access_token: String,
    pub enabled: bool,
    /// "open" | "allowlist" | "pairing"
    pub dm_policy: String,
    /// Matrix user IDs (@user:server) allowed to DM
    pub allowed_users: Vec<String>,
    #[serde(default)]
    pub pending_users: Vec<PendingUser>,
    pub agent_id: Option<String>,
    /// Whether to respond in group rooms (not just DMs)
    #[serde(default)]
    pub respond_in_rooms: bool,
    /// Device ID persisted from previous session (for E2EE continuity)
    #[serde(default)]
    pub device_id: Option<String>,
    /// User ID persisted from previous session
    #[serde(default)]
    pub user_id: Option<String>,
}

impl Default for MatrixConfig {
    fn default() -> Self {
        MatrixConfig {
            homeserver: "https://matrix.org".into(),
            access_token: String::new(),
            enabled: false,
            dm_policy: "pairing".into(),
            allowed_users: vec![],
            pending_users: vec![],
            agent_id: None,
            respond_in_rooms: false,
            device_id: None,
            user_id: None,
        }
    }
}

// ── Global State ───────────────────────────────────────────────────────

static BRIDGE_RUNNING: AtomicBool = AtomicBool::new(false);
static MESSAGE_COUNT: AtomicI64 = AtomicI64::new(0);
static BOT_USER_ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
static STOP_SIGNAL: std::sync::OnceLock<Arc<AtomicBool>> = std::sync::OnceLock::new();

fn get_stop_signal() -> Arc<AtomicBool> {
    STOP_SIGNAL.get_or_init(|| Arc::new(AtomicBool::new(false))).clone()
}

const CONFIG_KEY: &str = "matrix_config";

/// Where to store the matrix-sdk SQLite crypto/state database.
fn store_path() -> std::path::PathBuf {
    let base = dirs::document_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    base.join("Paw").join("matrix-store")
}

// ── Bridge Core ────────────────────────────────────────────────────────

pub fn start_bridge(app_handle: tauri::AppHandle) -> EngineResult<()> {
    if BRIDGE_RUNNING.load(Ordering::Relaxed) {
        return Err("Matrix bridge is already running".into());
    }

    let config: MatrixConfig = channels::load_channel_config(&app_handle, CONFIG_KEY)?;
    if config.homeserver.is_empty() || config.access_token.is_empty() {
        return Err("Homeserver URL and access token are required.".into());
    }
    if !config.enabled {
        return Err("Matrix bridge is disabled.".into());
    }

    let stop = get_stop_signal();
    stop.store(false, Ordering::Relaxed);
    BRIDGE_RUNNING.store(true, Ordering::Relaxed);

    info!("[matrix] Starting E2EE bridge to {}", config.homeserver);

    tauri::async_runtime::spawn(async move {
        let mut reconnect_attempt: u32 = 0;
        loop {
            match run_sdk_bridge(app_handle.clone(), config.clone()).await {
                Ok(()) => break, // Clean shutdown
                Err(e) => {
                    if get_stop_signal().load(Ordering::Relaxed) { break; }
                    error!("[matrix] Bridge error: {} — reconnecting", e);
                    let delay = crate::engine::http::reconnect_delay(reconnect_attempt).await;
                    warn!("[matrix] Reconnecting in {}ms (attempt {})", delay.as_millis(), reconnect_attempt + 1);
                    reconnect_attempt += 1;
                    if get_stop_signal().load(Ordering::Relaxed) { break; }
                }
            }
            reconnect_attempt = 0;
        }
        BRIDGE_RUNNING.store(false, Ordering::Relaxed);
        info!("[matrix] Bridge stopped");
    });

    Ok(())
}

pub fn stop_bridge() {
    let stop = get_stop_signal();
    stop.store(true, Ordering::Relaxed);
    BRIDGE_RUNNING.store(false, Ordering::Relaxed);
    info!("[matrix] Stop signal sent");
}

pub fn get_status(app_handle: &tauri::AppHandle) -> ChannelStatus {
    let config: MatrixConfig = channels::load_channel_config(app_handle, CONFIG_KEY).unwrap_or_default();
    ChannelStatus {
        running: BRIDGE_RUNNING.load(Ordering::Relaxed),
        connected: BRIDGE_RUNNING.load(Ordering::Relaxed),
        bot_name: BOT_USER_ID.get().cloned(),
        bot_id: BOT_USER_ID.get().cloned(),
        message_count: MESSAGE_COUNT.load(Ordering::Relaxed) as u64,
        allowed_users: config.allowed_users,
        pending_users: config.pending_users,
        dm_policy: config.dm_policy,
    }
}

// ── matrix-sdk E2EE Bridge ─────────────────────────────────────────────

async fn run_sdk_bridge(app_handle: tauri::AppHandle, mut config: MatrixConfig) -> EngineResult<()> {
    let stop = get_stop_signal();
    let hs = config.homeserver.trim_end_matches('/');

    // ── Build client with SQLite crypto store for E2EE ────────────────
    let sdk_client = Client::builder()
        .homeserver_url(hs)
        .sqlite_store(store_path(), None)
        .build()
        .await?;

    // ── Restore session from access token ─────────────────────────────
    // We need user_id and device_id to restore. If not cached, resolve
    // them via /whoami and persist for E2EE continuity across restarts.
    let (user_id_str, device_id_str) = match (&config.user_id, &config.device_id) {
        (Some(uid), Some(did)) => (uid.clone(), did.clone()),
        _ => {
            let whoami = resolve_whoami(hs, &config.access_token).await?;
            config.user_id = Some(whoami.0.clone());
            config.device_id = Some(whoami.1.clone());
            let _ = channels::save_channel_config(&app_handle, CONFIG_KEY, &config);
            whoami
        }
    };

    let user_id: OwnedUserId = user_id_str.parse()
        .map_err(|e| format!("Invalid user_id '{}': {}", user_id_str, e))?;
    let device_id: OwnedDeviceId = device_id_str.into();

    let session = MatrixSession {
        meta: SessionMeta {
            user_id: user_id.clone(),
            device_id,
        },
        tokens: matrix_sdk::authentication::matrix::MatrixSessionTokens {
            access_token: config.access_token.clone(),
            refresh_token: None,
        },
    };

    sdk_client.restore_session(session).await?;

    let bot_user_id_str = user_id.to_string();
    let _ = BOT_USER_ID.set(bot_user_id_str.clone());
    info!("[matrix] Authenticated as {} (E2EE enabled)", bot_user_id_str);

    let _ = app_handle.emit("matrix-status", json!({
        "kind": "connected",
        "user_id": &bot_user_id_str,
    }));

    // ── Register event handlers ───────────────────────────────────────

    // Auto-join rooms on invite
    sdk_client.add_event_handler(
        |ev: StrippedRoomMemberEvent, room: Room, client: Client| async move {
            if ev.state_key != client.user_id().expect("logged in").as_str() {
                return;
            }
            info!("[matrix] Invited to room {}", room.room_id());
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            if let Err(e) = room.join().await {
                warn!("[matrix] Failed to join {}: {}", room.room_id(), e);
            } else {
                info!("[matrix] Joined room {}", room.room_id());
            }
        }
    );

    // Handle incoming messages (decrypted automatically by matrix-sdk)
    let app_for_handler = app_handle.clone();
    let config_for_handler = config.clone();
    let bot_uid = user_id.clone();

    sdk_client.add_event_handler(
        move |ev: OriginalSyncRoomMessageEvent, room: Room| {
            let app = app_for_handler.clone();
            let cfg = config_for_handler.clone();
            let bot = bot_uid.clone();
            async move {
                handle_room_message(ev, room, app, cfg, bot).await;
            }
        }
    );

    // ── Initial sync (catch up, don't process old messages) ───────────
    let sync_settings = SyncSettings::default()
        .timeout(std::time::Duration::from_secs(30));
    let initial_response = sdk_client.sync_once(sync_settings.clone()).await?;
    info!("[matrix] Initial sync complete");

    // ── Long-polling sync loop ────────────────────────────────────────
    // We use sync_once in a loop so we can check the stop signal.
    let mut current_token: Option<String> = Some(initial_response.next_batch);
    let mut sync_errors: u32 = 0;
    loop {
        if stop.load(Ordering::Relaxed) { break; }

        let mut ss = SyncSettings::default()
            .timeout(std::time::Duration::from_secs(30));
        if let Some(ref token) = current_token {
            ss = ss.token(token.clone());
        }

        match sdk_client.sync_once(ss).await {
            Ok(response) => {
                current_token = Some(response.next_batch);
                sync_errors = 0;
            }
            Err(e) => {
                warn!("[matrix] Sync error: {} — backing off (attempt {})", e, sync_errors + 1);
                let delay = crate::engine::http::reconnect_delay(sync_errors).await;
                debug!("[matrix] Next sync retry in {}ms", delay.as_millis());
                sync_errors += 1;
            }
        }
    }

    let _ = app_handle.emit("matrix-status", json!({
        "kind": "disconnected",
    }));

    Ok(())
}

// ── Message Handler ────────────────────────────────────────────────────

async fn handle_room_message(
    ev: OriginalSyncRoomMessageEvent,
    room: Room,
    app_handle: tauri::AppHandle,
    config: MatrixConfig,
    bot_user_id: OwnedUserId,
) {
    // Skip own messages
    if ev.sender == bot_user_id { return; }

    // Only handle text messages
    let body = match &ev.content.msgtype {
        MessageType::Text(text) => text.body.clone(),
        _ => return,
    };
    if body.is_empty() { return; }

    let sender = ev.sender.to_string();
    let room_id = room.room_id().to_string();

    // DM detection via matrix-sdk
    let is_dm = room.is_direct().await.unwrap_or(false);

    // In group rooms, only respond if directly mentioned or respond_in_rooms is on
    let bot_id_str = bot_user_id.to_string();
    if !is_dm {
        let mentioned = body.contains(&bot_id_str);
        if !config.respond_in_rooms && !mentioned { return; }
    }

    let content = body.replace(&bot_id_str, "").trim().to_string();
    if content.is_empty() { return; }

    debug!("[matrix] {} from {} in {}: {}",
        if is_dm { "DM" } else { "Message" },
        sender, room_id,
        if content.len() > 50 { format!("{}...", &content[..50]) } else { content.clone() });

    // ── Access control ────────────────────────────────────────────────
    if is_dm {
        let mut current_config: MatrixConfig = channels::load_channel_config(&app_handle, CONFIG_KEY)
            .unwrap_or(config.clone());
        match channels::check_access(
            &current_config.dm_policy,
            &sender,
            &sender,
            &sender,
            &current_config.allowed_users,
            &mut current_config.pending_users,
        ) {
            Err(denial_msg) => {
                let _ = channels::save_channel_config(&app_handle, CONFIG_KEY, &current_config);
                let _ = app_handle.emit("matrix-status", json!({
                    "kind": "pairing_request",
                    "user_id": &sender,
                }));
                send_room_message(&room, &denial_msg).await;
                return;
            }
            Ok(()) => {}
        }
    }

    MESSAGE_COUNT.fetch_add(1, Ordering::Relaxed);

    // ── Route to agent ────────────────────────────────────────────────
    let current_config: MatrixConfig = channels::load_channel_config(&app_handle, CONFIG_KEY)
        .unwrap_or(config);
    let agent_id = current_config.agent_id.as_deref().unwrap_or("default");
    let ctx = if is_dm {
        "You are replying to a private Matrix DM (end-to-end encrypted). \
         Use plain text or simple markdown. Keep responses concise."
    } else {
        "You are chatting in a Matrix room. Use plain text or simple markdown. \
         Keep responses concise."
    };

    let response = channels::run_channel_agent(
        &app_handle, "matrix", ctx, &content, &sender, agent_id,
    ).await;

    match response {
        Ok(reply) if !reply.is_empty() => {
            send_room_message(&room, &reply).await;
        }
        Err(e) => {
            error!("[matrix] Agent error for {}: {}", sender, e);
            send_room_message(&room, &format!("⚠️ Error: {}", e)).await;
        }
        _ => {}
    }
}

// ── Helpers ────────────────────────────────────────────────────────────

/// Send a text message to a room (auto-encrypted in E2EE rooms by matrix-sdk).
async fn send_room_message(room: &Room, text: &str) {
    let content = RoomMessageEventContent::text_plain(text);
    if let Err(e) = room.send(content).await {
        warn!("[matrix] Failed to send message to {}: {}", room.room_id(), e);
    }
}

/// Resolve user_id and device_id from an access token via the /whoami endpoint.
async fn resolve_whoami(homeserver: &str, access_token: &str) -> EngineResult<(String, String)> {
    let http = reqwest::Client::new();
    let whoami_url = format!("{}/_matrix/client/v3/account/whoami", homeserver);
    let resp: serde_json::Value = http.get(&whoami_url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send().await?
        .json().await?;

    if let Some(err) = resp.get("errcode") {
        return Err(format!("Matrix auth error: {} - {}",
            err, resp["error"].as_str().unwrap_or("")).into());
    }

    let user_id = resp["user_id"].as_str()
        .ok_or("Missing user_id in /whoami response")?
        .to_string();
    let device_id = resp["device_id"].as_str()
        .unwrap_or(&format!("PAW_{}", &uuid::Uuid::new_v4().to_string()[..8]))
        .to_string();

    Ok((user_id, device_id))
}

// ── Config Persistence ─────────────────────────────────────────────────

pub fn load_config(app_handle: &tauri::AppHandle) -> EngineResult<MatrixConfig> {
    channels::load_channel_config(app_handle, CONFIG_KEY)
}

pub fn save_config(app_handle: &tauri::AppHandle, config: &MatrixConfig) -> EngineResult<()> {
    channels::save_channel_config(app_handle, CONFIG_KEY, config)
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
