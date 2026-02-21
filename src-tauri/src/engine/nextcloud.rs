// Paw Agent Engine — Nextcloud Talk Bridge
//
// Connects Paw to Nextcloud Talk via HTTP long-polling.
// Pure outbound HTTP — no webhooks, no public URL.
//
// Setup: Nextcloud → Settings → Security → Create App Password.
//        Give a server URL (e.g. "https://cloud.example.com"),
//        username, and the app password.
//
// Security:
//   - HTTPS enforced — `http://` URLs are auto-coerced to `https://`
//   - Allowlist by Nextcloud user ID (display name)
//   - Optional pairing mode
//   - Basic auth over TLS

use crate::engine::channels::{self, PendingUser, ChannelStatus};
use log::{debug, info, warn, error};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use tauri::Emitter;
use crate::atoms::error::EngineResult;

// ── Nextcloud Talk Config ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextcloudConfig {
    /// Nextcloud server URL (e.g. "https://cloud.example.com")
    pub server_url: String,
    /// Login username
    pub username: String,
    /// App password (NOT the main password)
    pub password: String,
    pub enabled: bool,
    /// "open" | "allowlist" | "pairing"
    pub dm_policy: String,
    pub allowed_users: Vec<String>,
    #[serde(default)]
    pub pending_users: Vec<PendingUser>,
    pub agent_id: Option<String>,
    /// Whether to respond in group conversations (not just 1-on-1)
    #[serde(default)]
    pub respond_in_groups: bool,
}

impl Default for NextcloudConfig {
    fn default() -> Self {
        NextcloudConfig {
            server_url: String::new(),
            username: String::new(),
            password: String::new(),
            enabled: false,
            dm_policy: "pairing".into(),
            allowed_users: vec![],
            pending_users: vec![],
            agent_id: None,
            respond_in_groups: false,
        }
    }
}

// ── Global State ───────────────────────────────────────────────────────

static BRIDGE_RUNNING: AtomicBool = AtomicBool::new(false);
static MESSAGE_COUNT: AtomicI64 = AtomicI64::new(0);
static BOT_USERNAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
static STOP_SIGNAL: std::sync::OnceLock<Arc<AtomicBool>> = std::sync::OnceLock::new();

fn get_stop_signal() -> Arc<AtomicBool> {
    STOP_SIGNAL.get_or_init(|| Arc::new(AtomicBool::new(false))).clone()
}

const CONFIG_KEY: &str = "nextcloud_config";

/// Normalize the server URL to enforce HTTPS.
/// - Strips trailing slashes
/// - Coerces `http://` → `https://` with a warning
/// - Adds `https://` if no scheme is present
/// - Rejects URLs with non-http(s) schemes
fn normalize_server_url(raw: &str) -> EngineResult<String> {
    let url = raw.trim().trim_end_matches('/');
    if url.is_empty() {
        return Err("Server URL is required.".into());
    }

    if url.starts_with("http://") {
        let secure = format!("https://{}", &url["http://".len()..]);
        warn!(
            "[nextcloud] Coerced server URL from http:// to https:// — \
             Basic Auth credentials must not be sent over plaintext HTTP"
        );
        return Ok(secure);
    }

    if url.starts_with("https://") {
        return Ok(url.to_string());
    }

    // Reject other schemes (ftp://, ws://, etc.)
    if let Some(colon_pos) = url.find("://") {
        let scheme = &url[..colon_pos];
        return Err(format!(
            "Unsupported URL scheme '{}://'. Use https:// for your Nextcloud server.",
            scheme
        ).into());
    }

    // No scheme — assume https
    warn!("[nextcloud] No URL scheme provided, assuming https://{}", url);
    Ok(format!("https://{}", url))
}

// ── Bridge Core ────────────────────────────────────────────────────────

pub fn start_bridge(app_handle: tauri::AppHandle) -> EngineResult<()> {
    if BRIDGE_RUNNING.load(Ordering::Relaxed) {
        return Err("Nextcloud Talk bridge is already running".into());
    }

    let mut config: NextcloudConfig = channels::load_channel_config(&app_handle, CONFIG_KEY)?;
    if config.server_url.is_empty() || config.username.is_empty() || config.password.is_empty() {
        return Err("Server URL, username, and app password are required.".into());
    }
    if !config.enabled {
        return Err("Nextcloud Talk bridge is disabled.".into());
    }

    // Enforce HTTPS — coerce http:// or bare hostnames to https://
    config.server_url = normalize_server_url(&config.server_url)?;

    let stop = get_stop_signal();
    stop.store(false, Ordering::Relaxed);
    BRIDGE_RUNNING.store(true, Ordering::Relaxed);

    info!("[nextcloud] Starting bridge to {}", config.server_url);

    tauri::async_runtime::spawn(async move {
        if let Err(e) = run_poll_loop(app_handle, config).await {
            error!("[nextcloud] Bridge crashed: {}", e);
        }
        BRIDGE_RUNNING.store(false, Ordering::Relaxed);
        info!("[nextcloud] Bridge stopped");
    });

    Ok(())
}

pub fn stop_bridge() {
    let stop = get_stop_signal();
    stop.store(true, Ordering::Relaxed);
    BRIDGE_RUNNING.store(false, Ordering::Relaxed);
    info!("[nextcloud] Stop signal sent");
}

pub fn get_status(app_handle: &tauri::AppHandle) -> ChannelStatus {
    let config: NextcloudConfig = channels::load_channel_config(app_handle, CONFIG_KEY).unwrap_or_default();
    ChannelStatus {
        running: BRIDGE_RUNNING.load(Ordering::Relaxed),
        connected: BRIDGE_RUNNING.load(Ordering::Relaxed),
        bot_name: BOT_USERNAME.get().cloned(),
        bot_id: BOT_USERNAME.get().cloned(),
        message_count: MESSAGE_COUNT.load(Ordering::Relaxed) as u64,
        allowed_users: config.allowed_users,
        pending_users: config.pending_users,
        dm_policy: config.dm_policy,
    }
}

// ── Nextcloud Talk Polling Loop ────────────────────────────────────────

async fn run_poll_loop(app_handle: tauri::AppHandle, config: NextcloudConfig) -> EngineResult<()> {
    let stop = get_stop_signal();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()?;

    let base = config.server_url.trim_end_matches('/');
    let bot_user = config.username.clone();
    let _ = BOT_USERNAME.set(bot_user.clone());

    // Verify credentials by getting user status
    let caps_url = format!("{}/ocs/v2.php/cloud/capabilities", base);
    let caps_resp = client.get(&caps_url)
        .basic_auth(&config.username, Some(&config.password))
        .header("OCS-APIREQUEST", "true")
        .header("Accept", "application/json")
        .send().await?;

    if !caps_resp.status().is_success() {
        return Err(format!("Auth failed: HTTP {}", caps_resp.status()).into());
    }

    let caps: serde_json::Value = caps_resp.json().await?;

    // Check that Talk is enabled
    let has_talk = caps["ocs"]["data"]["capabilities"]["spreed"].is_object();
    if !has_talk {
        return Err("Nextcloud Talk (spreed) is not enabled on this server.".into());
    }

    info!("[nextcloud] Authenticated as {}", bot_user);

    let _ = app_handle.emit("nextcloud-status", json!({
        "kind": "connected",
        "username": &bot_user,
    }));

    // Get initial list of conversations
    let rooms_url = format!("{}/ocs/v2.php/apps/spreed/api/v4/room", base);

    let mut current_config = config.clone();
    let mut last_config_reload = std::time::Instant::now();

    // Track last known message ID per room to avoid reprocessing
    let mut last_known_id: std::collections::HashMap<String, i64> = std::collections::HashMap::new();

    // Initial pass: get rooms and set last_known_id to current timestamp
    let rooms = nc_get_rooms(&client, &rooms_url, &config.username, &config.password).await?;
    for room in &rooms {
        let token = room["token"].as_str().unwrap_or("");
        let last_msg_id = room["lastMessage"]["id"].as_i64().unwrap_or(0);
        if !token.is_empty() {
            last_known_id.insert(token.to_string(), last_msg_id);
        }
    }
    info!("[nextcloud] Monitoring {} conversations", rooms.len());

    // Polling loop
    loop {
        if stop.load(Ordering::Relaxed) { break; }

        // Poll for new messages across all rooms with unread messages
        let rooms = nc_get_rooms(&client, &rooms_url, &config.username, &config.password).await
            .unwrap_or_default();

        for room in &rooms {
            if stop.load(Ordering::Relaxed) { break; }

            let token = room["token"].as_str().unwrap_or("").to_string();
            let room_type = room["type"].as_u64().unwrap_or(0);
            // type 1 = one-to-one, 2 = group, 3 = public, 4 = changelog
            let is_dm = room_type == 1;
            let unread = room["unreadMessages"].as_u64().unwrap_or(0);
            if unread == 0 { continue; }

            // Skip group conversations unless configured
            if !is_dm && !current_config.respond_in_groups { continue; }

            let last_id = last_known_id.get(&token).copied().unwrap_or(0);

            // Fetch new messages
            let chat_url = format!(
                "{}/ocs/v2.php/apps/spreed/api/v1/chat/{}?lookIntoFuture=0&limit=50&lastKnownMessageId={}",
                base, token, last_id
            );

            let msgs_resp = client.get(&chat_url)
                .basic_auth(&config.username, Some(&config.password))
                .header("OCS-APIREQUEST", "true")
                .header("Accept", "application/json")
                .send().await;

            let msgs: Vec<serde_json::Value> = match msgs_resp {
                Ok(r) if r.status().is_success() => {
                    let body: serde_json::Value = r.json().await.unwrap_or_default();
                    body["ocs"]["data"].as_array().cloned().unwrap_or_default()
                }
                Ok(r) => {
                    warn!("[nextcloud] Chat poll {}: HTTP {}", token, r.status());
                    continue;
                }
                Err(e) => {
                    warn!("[nextcloud] Chat poll {}: {}", token, e);
                    continue;
                }
            };

            for msg in &msgs {
                let msg_id = msg["id"].as_i64().unwrap_or(0);
                if msg_id <= last_id { continue; }

                let actor_type = msg["actorType"].as_str().unwrap_or("");
                let actor_id = msg["actorId"].as_str().unwrap_or("");
                let actor_name = msg["actorDisplayName"].as_str().unwrap_or(actor_id);
                let message_type = msg["messageType"].as_str().unwrap_or("");
                let text = msg["message"].as_str().unwrap_or("").to_string();

                // Skip system messages, own messages, bot messages
                if message_type == "system" { continue; }
                if actor_type != "users" { continue; }
                if actor_id == bot_user { continue; }
                if text.is_empty() { continue; }

                debug!("[nextcloud] Message from {} in {}: {}",
                    actor_name, token,
                    if text.len() > 50 { format!("{}...", &text[..50]) } else { text.clone() });

                // Access control for DMs
                if is_dm {
                    match channels::check_access(
                        &current_config.dm_policy,
                        actor_id,
                        actor_name,
                        actor_name,
                        &current_config.allowed_users,
                        &mut current_config.pending_users,
                    ) {
                        Err(denial_msg) => {
                            let denial_str = denial_msg.to_string();
                            let _ = channels::save_channel_config(&app_handle, CONFIG_KEY, &current_config);
                            let _ = app_handle.emit("nextcloud-status", json!({
                                "kind": "pairing_request",
                                "user_id": actor_id,
                                "username": actor_name,
                            }));
                            let _ = nc_send_message(&client, base, &config.username, &config.password, &token, &denial_str).await;
                            continue;
                        }
                        Ok(()) => {}
                    }
                }

                MESSAGE_COUNT.fetch_add(1, Ordering::Relaxed);

                let agent_id = current_config.agent_id.as_deref().unwrap_or("default");
                let ctx = "You are chatting via Nextcloud Talk. Use plain text or simple markdown. \
                           Keep responses concise.";

                let response = channels::run_channel_agent(
                    &app_handle, "nextcloud", ctx, &text, actor_id, agent_id,
                ).await;

                match response {
                    Ok(reply) if !reply.is_empty() => {
                        // Nextcloud Talk max message length is 32000 chars
                        for chunk in channels::split_message(&reply, 32000) {
                            let _ = nc_send_message(&client, base, &config.username, &config.password, &token, &chunk).await;
                        }
                    }
                    Err(e) => {
                        error!("[nextcloud] Agent error for {}: {}", actor_id, e);
                        let _ = nc_send_message(&client, base, &config.username, &config.password, &token,
                            &format!("⚠️ Error: {}", e)).await;
                    }
                    _ => {}
                }

                // Update last known ID
                last_known_id.insert(token.clone(), msg_id);
            }

            // Mark the latest message as last known even if we didn't process it
            if let Some(last_msg) = msgs.last() {
                if let Some(id) = last_msg["id"].as_i64() {
                    let entry = last_known_id.entry(token).or_insert(0);
                    if id > *entry {
                        *entry = id;
                    }
                }
            }
        }

        // Reload config periodically
        if last_config_reload.elapsed() > std::time::Duration::from_secs(30) {
            if let Ok(fresh) = channels::load_channel_config::<NextcloudConfig>(&app_handle, CONFIG_KEY) {
                current_config = fresh;
            }
            last_config_reload = std::time::Instant::now();
        }

        // Wait before next poll cycle (Nextcloud doesn't do long-poll well, so 5s interval)
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }

    let _ = app_handle.emit("nextcloud-status", json!({
        "kind": "disconnected",
    }));

    Ok(())
}

// ── Nextcloud Talk REST Helpers ────────────────────────────────────────

async fn nc_get_rooms(
    client: &reqwest::Client,
    rooms_url: &str,
    username: &str,
    password: &str,
) -> EngineResult<Vec<serde_json::Value>> {
    let resp = client.get(rooms_url)
        .basic_auth(username, Some(password))
        .header("OCS-APIREQUEST", "true")
        .header("Accept", "application/json")
        .send().await?;

    let body: serde_json::Value = resp.json().await?;

    Ok(body["ocs"]["data"].as_array().cloned().unwrap_or_default())
}

async fn nc_send_message(
    client: &reqwest::Client,
    base: &str,
    username: &str,
    password: &str,
    room_token: &str,
    message: &str,
) -> EngineResult<()> {
    let url = format!("{}/ocs/v2.php/apps/spreed/api/v1/chat/{}", base, room_token);

    match client.post(&url)
        .basic_auth(username, Some(password))
        .header("OCS-APIREQUEST", "true")
        .header("Accept", "application/json")
        .json(&json!({ "message": message }))
        .send().await
    {
        Ok(r) if !r.status().is_success() => {
            warn!("[nextcloud] sendMessage failed: {}", r.status());
        }
        Err(e) => warn!("[nextcloud] sendMessage error: {}", e),
        _ => {}
    }
    Ok(())
}

// ── Config Persistence ─────────────────────────────────────────────────

pub fn load_config(app_handle: &tauri::AppHandle) -> EngineResult<NextcloudConfig> {
    channels::load_channel_config(app_handle, CONFIG_KEY)
}

pub fn save_config(app_handle: &tauri::AppHandle, config: &NextcloudConfig) -> EngineResult<()> {
    // Normalize URL at save time so the UI reflects the coerced value
    let mut config = config.clone();
    if !config.server_url.is_empty() {
        config.server_url = normalize_server_url(&config.server_url)?;
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
