// Paw Agent Engine — Matrix Bridge
//
// Connects Paw to Matrix via the Client-Server API using long-polling (/sync).
// No webhooks, no public URL — pure outbound HTTP polling.
//
// Setup: Register a bot account on any Matrix homeserver → get access token → paste in Paw.
//        Works with matrix.org, self-hosted Synapse/Dendrite, or any spec-compliant server.
//
// Security:
//   - Allowlist by Matrix user ID (@user:server.org)
//   - Optional pairing mode
//   - All communication through the homeserver's TLS API

use crate::engine::channels::{self, PendingUser, ChannelStatus};
use log::{info, warn, error};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use tauri::Emitter;

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

// ── Bridge Core ────────────────────────────────────────────────────────

pub fn start_bridge(app_handle: tauri::AppHandle) -> Result<(), String> {
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

    info!("[matrix] Starting bridge to {}", config.homeserver);

    tauri::async_runtime::spawn(async move {
        if let Err(e) = run_sync_loop(app_handle, config).await {
            error!("[matrix] Bridge crashed: {}", e);
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

// ── Matrix /sync Long-Polling ──────────────────────────────────────────

async fn run_sync_loop(app_handle: tauri::AppHandle, config: MatrixConfig) -> Result<(), String> {
    let stop = get_stop_signal();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(90))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let hs = config.homeserver.trim_end_matches('/');

    // Verify credentials with /whoami
    let whoami_url = format!("{}/_matrix/client/v3/account/whoami", hs);
    let whoami: serde_json::Value = client.get(&whoami_url)
        .header("Authorization", format!("Bearer {}", config.access_token))
        .send().await.map_err(|e| format!("whoami failed: {}", e))?
        .json().await.map_err(|e| format!("whoami parse: {}", e))?;

    if let Some(err) = whoami.get("errcode") {
        return Err(format!("Matrix auth error: {} - {}", err, whoami["error"].as_str().unwrap_or("")));
    }

    let bot_user_id = whoami["user_id"].as_str().unwrap_or("unknown").to_string();
    let _ = BOT_USER_ID.set(bot_user_id.clone());
    info!("[matrix] Authenticated as {}", bot_user_id);

    let _ = app_handle.emit("matrix-status", json!({
        "kind": "connected",
        "user_id": &bot_user_id,
    }));

    // Initial sync to get the since token (filter to reduce payload)
    let filter = json!({
        "room": {
            "timeline": { "limit": 1 },
            "state": { "types": ["m.room.member"] }
        },
        "presence": { "types": [] },
        "account_data": { "types": [] }
    });
    let filter_str = serde_json::to_string(&filter).unwrap();

    let initial_url = format!(
        "{}/_matrix/client/v3/sync?filter={}&timeout=0",
        hs,
        urlencoded(&filter_str)
    );
    let initial_resp: serde_json::Value = client.get(&initial_url)
        .header("Authorization", format!("Bearer {}", config.access_token))
        .send().await.map_err(|e| format!("initial sync: {}", e))?
        .json().await.map_err(|e| format!("initial sync parse: {}", e))?;

    let mut since = initial_resp["next_batch"].as_str()
        .ok_or("No next_batch in initial sync")?
        .to_string();

    info!("[matrix] Initial sync done, since={}", &since[..20.min(since.len())]);

    // Auto-join invites
    if let Some(invited) = initial_resp["rooms"]["invite"].as_object() {
        for room_id in invited.keys() {
            let _ = matrix_join_room(&client, hs, &config.access_token, room_id).await;
        }
    }

    let mut current_config = config.clone();
    let mut last_config_reload = std::time::Instant::now();

    // Long-polling sync loop
    loop {
        if stop.load(Ordering::Relaxed) { break; }

        let sync_url = format!(
            "{}/_matrix/client/v3/sync?since={}&timeout=30000&filter={}",
            hs,
            urlencoded(&since),
            urlencoded(&filter_str)
        );

        let sync_resp = client.get(&sync_url)
            .header("Authorization", format!("Bearer {}", config.access_token))
            .send().await;

        let sync_json: serde_json::Value = match sync_resp {
            Ok(r) => match r.json().await {
                Ok(j) => j,
                Err(e) => {
                    warn!("[matrix] sync parse error: {} — retrying", e);
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    continue;
                }
            },
            Err(e) => {
                warn!("[matrix] sync error: {} — retrying in 5s", e);
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }
        };

        if let Some(nb) = sync_json["next_batch"].as_str() {
            since = nb.to_string();
        }

        // Auto-join new invites
        if let Some(invited) = sync_json["rooms"]["invite"].as_object() {
            for room_id in invited.keys() {
                let _ = matrix_join_room(&client, hs, &config.access_token, room_id).await;
            }
        }

        // Process joined rooms → timeline events
        if let Some(joined) = sync_json["rooms"]["join"].as_object() {
            for (room_id, room_data) in joined {
                let events = room_data["timeline"]["events"].as_array();
                let events = match events {
                    Some(e) => e,
                    None => continue,
                };

                // Determine if this is a DM (heuristic: <= 2 joined members)
                let is_dm = room_data["summary"]["m.joined_member_count"]
                    .as_u64()
                    .map(|n| n <= 2)
                    .unwrap_or(false);

                for event in events {
                    let sender = event["sender"].as_str().unwrap_or("");
                    if sender == bot_user_id { continue; } // Skip own messages

                    let event_type = event["type"].as_str().unwrap_or("");
                    if event_type != "m.room.message" { continue; }

                    let msgtype = event["content"]["msgtype"].as_str().unwrap_or("");
                    if msgtype != "m.text" { continue; }

                    let body = event["content"]["body"].as_str().unwrap_or("").to_string();
                    if body.is_empty() { continue; }

                    // In group rooms, only respond if directly mentioned or respond_in_rooms is on
                    if !is_dm {
                        let mentioned = body.contains(&bot_user_id);
                        if !current_config.respond_in_rooms && !mentioned { continue; }
                    }

                    let content = body.replace(&bot_user_id, "").trim().to_string();
                    if content.is_empty() { continue; }

                    info!("[matrix] Message from {} in {}: {}", sender, room_id,
                        if content.len() > 50 { format!("{}...", &content[..50]) } else { content.clone() });

                    // Access control (DMs)
                    if is_dm {
                        match channels::check_access(
                            &current_config.dm_policy,
                            sender,
                            sender,
                            sender,
                            &current_config.allowed_users,
                            &mut current_config.pending_users,
                        ) {
                            Err(denial_msg) => {
                                let _ = channels::save_channel_config(&app_handle, CONFIG_KEY, &current_config);
                                let _ = app_handle.emit("matrix-status", json!({
                                    "kind": "pairing_request",
                                    "user_id": sender,
                                }));
                                let _ = matrix_send_message(&client, hs, &config.access_token, room_id, &denial_msg).await;
                                continue;
                            }
                            Ok(()) => {}
                        }
                    }

                    MESSAGE_COUNT.fetch_add(1, Ordering::Relaxed);

                    // Route to agent
                    let agent_id = current_config.agent_id.as_deref().unwrap_or("default");
                    let ctx = "You are chatting via Matrix. Use plain text or simple markdown. \
                               Matrix supports basic formatting. Keep responses concise.";

                    let response = channels::run_channel_agent(
                        &app_handle, "matrix", ctx, &content, sender, agent_id,
                    ).await;

                    match response {
                        Ok(reply) if !reply.is_empty() => {
                            let _ = matrix_send_message(&client, hs, &config.access_token, room_id, &reply).await;
                        }
                        Err(e) => {
                            error!("[matrix] Agent error for {}: {}", sender, e);
                            let _ = matrix_send_message(&client, hs, &config.access_token, room_id,
                                &format!("⚠️ Error: {}", e)).await;
                        }
                        _ => {}
                    }
                }
            }
        }

        // Reload config
        if last_config_reload.elapsed() > std::time::Duration::from_secs(30) {
            if let Ok(fresh) = channels::load_channel_config::<MatrixConfig>(&app_handle, CONFIG_KEY) {
                current_config = fresh;
            }
            last_config_reload = std::time::Instant::now();
        }
    }

    let _ = app_handle.emit("matrix-status", json!({
        "kind": "disconnected",
    }));

    Ok(())
}

// ── Matrix REST API Helpers ────────────────────────────────────────────

async fn matrix_send_message(
    client: &reqwest::Client,
    hs: &str,
    token: &str,
    room_id: &str,
    text: &str,
) -> Result<(), String> {
    let txn_id = uuid::Uuid::new_v4().to_string();
    let url = format!(
        "{}/_matrix/client/v3/rooms/{}/send/m.room.message/{}",
        hs,
        urlencoded(room_id),
        txn_id
    );

    let body = json!({
        "msgtype": "m.text",
        "body": text,
    });

    match client.put(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&body)
        .send().await
    {
        Ok(r) if !r.status().is_success() => {
            let status = r.status();
            let resp_body = r.text().await.unwrap_or_default();
            warn!("[matrix] sendMessage failed: {} {}", status, resp_body);
        }
        Err(e) => warn!("[matrix] sendMessage failed: {}", e),
        _ => {}
    }
    Ok(())
}

async fn matrix_join_room(
    client: &reqwest::Client,
    hs: &str,
    token: &str,
    room_id: &str,
) -> Result<(), String> {
    let url = format!("{}/_matrix/client/v3/join/{}", hs, urlencoded(room_id));
    match client.post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({}))
        .send().await
    {
        Ok(r) => {
            if r.status().is_success() {
                info!("[matrix] Joined room {}", room_id);
            } else {
                warn!("[matrix] Failed to join {}: {}", room_id, r.status());
            }
        }
        Err(e) => warn!("[matrix] Join error for {}: {}", room_id, e),
    }
    Ok(())
}

fn urlencoded(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

// ── Config Persistence ─────────────────────────────────────────────────

pub fn load_config(app_handle: &tauri::AppHandle) -> Result<MatrixConfig, String> {
    channels::load_channel_config(app_handle, CONFIG_KEY)
}

pub fn save_config(app_handle: &tauri::AppHandle, config: &MatrixConfig) -> Result<(), String> {
    channels::save_channel_config(app_handle, CONFIG_KEY, config)
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
