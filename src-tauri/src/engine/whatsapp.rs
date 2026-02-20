// Paw Agent Engine — WhatsApp Bridge (via Evolution API)
//
// Connects Paw to WhatsApp via a local Evolution API Docker container.
// Evolution API wraps the Baileys library (WhatsApp Web multi-device protocol)
// and exposes a clean REST + Webhook interface — zero WebSocket code in Paw.
//
// Architecture:
//   1. Paw auto-manages an Evolution API Docker container (pull + run)
//   2. User scans a QR code to link their WhatsApp account
//   3. Inbound messages arrive via webhook (Evolution → Paw's local HTTP listener)
//   4. Paw routes through the agent loop and replies via Evolution's REST API
//
// Setup: Enable WhatsApp in Channels → Paw handles Docker + QR code automatically.
//
// Security:
//   - Allowlist by phone number or WhatsApp JID
//   - Optional pairing mode (first message from unknown → pending approval)
//   - All WhatsApp traffic stays on your machine (Docker container)
//   - Evolution API bound to localhost only

use crate::engine::channels::{self, PendingUser, ChannelStatus};
use crate::engine::sandbox; // reuse Docker health check
use log::{info, warn, error};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use tauri::Emitter;

// ── WhatsApp Config ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhatsAppConfig {
    pub enabled: bool,
    /// Instance name for Evolution API (default: "paw")
    pub instance_name: String,
    /// Evolution API base URL (auto-set when Docker container starts)
    pub api_url: String,
    /// Evolution API key (auto-generated on first run)
    pub api_key: String,
    /// Port for the Evolution API container (default: 8085)
    pub api_port: u16,
    /// Port for the local webhook listener (default: 8086)
    pub webhook_port: u16,
    /// "open" | "allowlist" | "pairing"
    pub dm_policy: String,
    /// Allowed phone numbers or WhatsApp JIDs (e.g. "1234567890" or "1234567890@s.whatsapp.net")
    pub allowed_users: Vec<String>,
    #[serde(default)]
    pub pending_users: Vec<PendingUser>,
    /// Which agent to route messages to
    pub agent_id: Option<String>,
    /// Whether to respond in group chats (when mentioned)
    #[serde(default)]
    pub respond_in_groups: bool,
    /// Docker container ID (managed internally)
    #[serde(default)]
    pub container_id: Option<String>,
    /// Whether the WhatsApp session is connected (QR scanned)
    #[serde(default)]
    pub session_connected: bool,
    /// QR code data (base64) for the frontend to display
    #[serde(default)]
    pub qr_code: Option<String>,
}

impl Default for WhatsAppConfig {
    fn default() -> Self {
        // Generate a random API key on first creation
        let api_key = format!("paw-wa-{}", uuid::Uuid::new_v4().to_string().replace('-', "")[..16].to_string());
        WhatsAppConfig {
            enabled: false,
            instance_name: "paw".into(),
            api_url: "http://127.0.0.1:8085".into(),
            api_key,
            api_port: 8085,
            webhook_port: 8086,
            dm_policy: "pairing".into(),
            allowed_users: vec![],
            pending_users: vec![],
            agent_id: None,
            respond_in_groups: false,
            container_id: None,
            session_connected: false,
            qr_code: None,
        }
    }
}

// ── Global State ───────────────────────────────────────────────────────

static BRIDGE_RUNNING: AtomicBool = AtomicBool::new(false);
static MESSAGE_COUNT: AtomicI64 = AtomicI64::new(0);
static STOP_SIGNAL: std::sync::OnceLock<Arc<AtomicBool>> = std::sync::OnceLock::new();

fn get_stop_signal() -> Arc<AtomicBool> {
    STOP_SIGNAL.get_or_init(|| Arc::new(AtomicBool::new(false))).clone()
}

const CONFIG_KEY: &str = "whatsapp_config";
const EVOLUTION_IMAGE: &str = "atende/evolution-api:latest";
const CONTAINER_NAME: &str = "paw-whatsapp-evolution";

// ── Bridge Core ────────────────────────────────────────────────────────

pub fn start_bridge(app_handle: tauri::AppHandle) -> Result<(), String> {
    if BRIDGE_RUNNING.load(Ordering::Relaxed) {
        return Err("WhatsApp bridge is already running".into());
    }

    let config: WhatsAppConfig = channels::load_channel_config(&app_handle, CONFIG_KEY)?;
    if !config.enabled {
        return Err("WhatsApp bridge is disabled. Enable it in Channels settings.".into());
    }

    let stop = get_stop_signal();
    stop.store(false, Ordering::Relaxed);
    BRIDGE_RUNNING.store(true, Ordering::Relaxed);

    info!("[whatsapp] Starting bridge via Evolution API");

    let app = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = run_whatsapp_bridge(app, config).await {
            error!("[whatsapp] Bridge crashed: {}", e);
        }
        BRIDGE_RUNNING.store(false, Ordering::Relaxed);
        info!("[whatsapp] Bridge stopped");
    });

    Ok(())
}

pub fn stop_bridge() {
    let stop = get_stop_signal();
    stop.store(true, Ordering::Relaxed);
    BRIDGE_RUNNING.store(false, Ordering::Relaxed);
    info!("[whatsapp] Stop signal sent");
}

pub fn get_status(app_handle: &tauri::AppHandle) -> ChannelStatus {
    let config: WhatsAppConfig = channels::load_channel_config(app_handle, CONFIG_KEY).unwrap_or_default();
    ChannelStatus {
        running: BRIDGE_RUNNING.load(Ordering::Relaxed),
        connected: config.session_connected && BRIDGE_RUNNING.load(Ordering::Relaxed),
        bot_name: Some("WhatsApp".into()),
        bot_id: config.container_id.clone(),
        message_count: MESSAGE_COUNT.load(Ordering::Relaxed) as u64,
        allowed_users: config.allowed_users,
        pending_users: config.pending_users,
        dm_policy: config.dm_policy,
    }
}

// ── Docker Container Management ────────────────────────────────────────

/// Ensure the Evolution API Docker container is running.
/// Pulls the image if needed, creates and starts the container.
async fn ensure_evolution_container(config: &WhatsAppConfig) -> Result<String, String> {
    use bollard::Docker;
    use bollard::container::{Config as ContainerConfig, CreateContainerOptions, StartContainerOptions, ListContainersOptions};
    use bollard::models::HostConfig;
    use bollard::image::CreateImageOptions;
    use futures::StreamExt;

    let docker = Docker::connect_with_local_defaults()
        .map_err(|e| format!("Docker not available: {}. Install Docker Desktop to use WhatsApp.", e))?;

    // Check if our container already exists
    let mut filters = std::collections::HashMap::new();
    filters.insert("name".to_string(), vec![CONTAINER_NAME.to_string()]);
    let opts = ListContainersOptions {
        all: true,
        filters,
        ..Default::default()
    };

    let containers = docker.list_containers(Some(opts)).await
        .map_err(|e| format!("Failed to list containers: {}", e))?;

    if let Some(existing) = containers.first() {
        let container_id = existing.id.clone().unwrap_or_default();
        let state = existing.state.as_deref().unwrap_or("");

        if state == "running" {
            info!("[whatsapp] Evolution API container already running: {}", &container_id[..12]);
            return Ok(container_id);
        }

        // Container exists but stopped — start it
        info!("[whatsapp] Starting existing Evolution API container");
        docker.start_container(&container_id, None::<StartContainerOptions<String>>).await
            .map_err(|e| format!("Failed to start container: {}", e))?;
        return Ok(container_id);
    }

    // Pull image if not present
    info!("[whatsapp] Pulling Evolution API image (first time setup)...");
    match docker.inspect_image(EVOLUTION_IMAGE).await {
        Ok(_) => info!("[whatsapp] Image already present"),
        Err(_) => {
            let pull_opts = CreateImageOptions {
                from_image: EVOLUTION_IMAGE,
                ..Default::default()
            };
            let mut stream = docker.create_image(Some(pull_opts), None, None);
            while let Some(result) = stream.next().await {
                if let Err(e) = result {
                    return Err(format!("Failed to pull Evolution API image: {}", e));
                }
            }
            info!("[whatsapp] Image pulled successfully");
        }
    }

    // Create container
    let host_config = HostConfig {
        port_bindings: Some({
            let mut ports = std::collections::HashMap::new();
            ports.insert(
                "8080/tcp".to_string(),
                Some(vec![bollard::models::PortBinding {
                    host_ip: Some("127.0.0.1".to_string()),
                    host_port: Some(config.api_port.to_string()),
                }]),
            );
            ports
        }),
        restart_policy: Some(bollard::models::RestartPolicy {
            name: Some(bollard::models::RestartPolicyNameEnum::UNLESS_STOPPED),
            maximum_retry_count: None,
        }),
        ..Default::default()
    };

    let container_config = ContainerConfig {
        image: Some(EVOLUTION_IMAGE.to_string()),
        env: Some(vec![
            format!("AUTHENTICATION_API_KEY={}", config.api_key),
            "SERVER_PORT=8080".to_string(),
            // Webhook: point back to Paw's webhook listener
            format!("WEBHOOK_GLOBAL_URL=http://host.docker.internal:{}/webhook/whatsapp", config.webhook_port),
            "WEBHOOK_GLOBAL_ENABLED=true".to_string(),
            "WEBHOOK_GLOBAL_WEBHOOK_BY_EVENTS=true".to_string(),
            // Events we care about
            "WEBHOOK_EVENTS_MESSAGES_UPSERT=true".to_string(),
            "WEBHOOK_EVENTS_QRCODE_UPDATED=true".to_string(),
            "WEBHOOK_EVENTS_CONNECTION_UPDATE=true".to_string(),
            // Disable features we don't need
            "WEBHOOK_EVENTS_MESSAGES_UPDATE=false".to_string(),
            "WEBHOOK_EVENTS_SEND_MESSAGE=false".to_string(),
            // Database: SQLite inside the container (persisted via volume)
            "DATABASE_PROVIDER=sqlite".to_string(),
            "DATABASE_CONNECTION_URI=file:./data/evolution.db".to_string(),
        ]),
        host_config: Some(host_config),
        exposed_ports: Some({
            let mut ports = std::collections::HashMap::new();
            ports.insert("8080/tcp".to_string(), std::collections::HashMap::new());
            ports
        }),
        ..Default::default()
    };

    let create_opts = CreateContainerOptions {
        name: CONTAINER_NAME,
        platform: None,
    };

    let container = docker.create_container(Some(create_opts), container_config).await
        .map_err(|e| format!("Failed to create Evolution API container: {}", e))?;

    let container_id = container.id.clone();
    info!("[whatsapp] Created Evolution API container: {}", &container_id[..12]);

    // Start it
    docker.start_container(&container_id, None::<StartContainerOptions<String>>).await
        .map_err(|e| format!("Failed to start Evolution API container: {}", e))?;

    info!("[whatsapp] Evolution API container started on port {}", config.api_port);

    // Wait for the API to be ready
    let client = reqwest::Client::new();
    let api_url = format!("http://127.0.0.1:{}", config.api_port);
    for attempt in 1..=30 {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        match client.get(&api_url).send().await {
            Ok(resp) if resp.status().is_success() || resp.status().as_u16() == 401 => {
                info!("[whatsapp] Evolution API ready after {} attempts", attempt);
                return Ok(container_id);
            }
            _ => {
                if attempt % 5 == 0 {
                    info!("[whatsapp] Waiting for Evolution API to start... (attempt {}/30)", attempt);
                }
            }
        }
    }

    Err("Evolution API container started but API didn't become ready within 60 seconds".into())
}

/// Create a WhatsApp instance in Evolution API and get the QR code.
async fn create_evolution_instance(config: &WhatsAppConfig) -> Result<String, String> {
    let client = reqwest::Client::new();
    let url = format!("{}/instance/create", config.api_url);

    let body = json!({
        "instanceName": config.instance_name,
        "integration": "WHATSAPP-BAILEYS",
        "qrcode": true,
        "webhook": {
            "url": format!("http://host.docker.internal:{}/webhook/whatsapp", config.webhook_port),
            "byEvents": true,
            "events": ["MESSAGES_UPSERT", "QRCODE_UPDATED", "CONNECTION_UPDATE"],
        }
    });

    let resp = client.post(&url)
        .header("apikey", &config.api_key)
        .json(&body)
        .send().await
        .map_err(|e| format!("Failed to create instance: {}", e))?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        // Instance might already exist — try to connect it
        if text.contains("already") || text.contains("exists") {
            info!("[whatsapp] Instance already exists, connecting...");
            return connect_evolution_instance(config).await;
        }
        return Err(format!("Create instance failed ({}): {}", status, text));
    }

    // Parse QR code from response
    let resp_json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("Parse create response: {}", e))?;

    let qr = resp_json["qrcode"]["base64"]
        .as_str()
        .or_else(|| resp_json["qrcode"].as_str())
        .unwrap_or("")
        .to_string();

    Ok(qr)
}

/// Connect an existing Evolution API instance.
async fn connect_evolution_instance(config: &WhatsAppConfig) -> Result<String, String> {
    let client = reqwest::Client::new();
    let url = format!("{}/instance/connect/{}", config.api_url, config.instance_name);

    let resp = client.get(&url)
        .header("apikey", &config.api_key)
        .send().await
        .map_err(|e| format!("Failed to connect instance: {}", e))?;

    let text = resp.text().await.unwrap_or_default();
    let resp_json: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();

    let qr = resp_json["base64"]
        .as_str()
        .or_else(|| resp_json["qrcode"]["base64"].as_str())
        .unwrap_or("")
        .to_string();

    Ok(qr)
}

// ── Main Bridge Loop ───────────────────────────────────────────────────

/// The main bridge loop:
/// 1. Ensure Docker container is running
/// 2. Create/connect WhatsApp instance (get QR code)
/// 3. Start local webhook HTTP listener
/// 4. Route inbound messages through the agent loop
async fn run_whatsapp_bridge(app_handle: tauri::AppHandle, mut config: WhatsAppConfig) -> Result<(), String> {
    let stop = get_stop_signal();

    // Step 1: Ensure Docker container is running
    let _ = app_handle.emit("whatsapp-status", json!({
        "kind": "starting",
        "message": "Starting Evolution API container...",
    }));

    let container_id = ensure_evolution_container(&config).await?;
    config.container_id = Some(container_id.clone());
    let _ = channels::save_channel_config(&app_handle, CONFIG_KEY, &config);

    // Step 2: Create/connect instance and get QR code
    let _ = app_handle.emit("whatsapp-status", json!({
        "kind": "connecting",
        "message": "Creating WhatsApp instance...",
    }));

    let qr_code = create_evolution_instance(&config).await?;
    if !qr_code.is_empty() {
        config.qr_code = Some(qr_code.clone());
        let _ = channels::save_channel_config(&app_handle, CONFIG_KEY, &config);

        let _ = app_handle.emit("whatsapp-status", json!({
            "kind": "qr_code",
            "qr": &qr_code,
            "message": "Scan this QR code with WhatsApp on your phone",
        }));

        info!("[whatsapp] QR code generated — waiting for scan");
    }

    // Step 3: Start webhook listener
    let webhook_port = config.webhook_port;
    let app_for_webhook = app_handle.clone();
    let stop_for_webhook = stop.clone();

    let webhook_handle = tauri::async_runtime::spawn(async move {
        if let Err(e) = run_webhook_listener(app_for_webhook, webhook_port, stop_for_webhook).await {
            error!("[whatsapp] Webhook listener error: {}", e);
        }
    });

    // Step 4: Poll for connection status until connected or stopped
    let client = reqwest::Client::new();
    let mut check_interval = tokio::time::interval(std::time::Duration::from_secs(5));
    let mut connected = config.session_connected;

    while !stop.load(Ordering::Relaxed) {
        check_interval.tick().await;

        if !connected {
            // Check connection status
            let url = format!("{}/instance/connectionState/{}", config.api_url, config.instance_name);
            match client.get(&url).header("apikey", &config.api_key).send().await {
                Ok(resp) => {
                    if let Ok(body) = resp.json::<serde_json::Value>().await {
                        let state = body["instance"]["state"].as_str()
                            .or_else(|| body["state"].as_str())
                            .unwrap_or("");

                        if state == "open" || state == "connected" {
                            connected = true;
                            config.session_connected = true;
                            config.qr_code = None;
                            let _ = channels::save_channel_config(&app_handle, CONFIG_KEY, &config);

                            let _ = app_handle.emit("whatsapp-status", json!({
                                "kind": "connected",
                                "message": "WhatsApp connected successfully",
                            }));

                            info!("[whatsapp] Session connected — ready to receive messages");
                        }
                    }
                }
                Err(e) => {
                    warn!("[whatsapp] Connection check failed: {}", e);
                }
            }
        }
    }

    // Cleanup
    webhook_handle.abort();

    let _ = app_handle.emit("whatsapp-status", json!({
        "kind": "disconnected",
    }));

    Ok(())
}

// ── Webhook HTTP Listener ──────────────────────────────────────────────

/// Minimal HTTP listener that receives webhooks from Evolution API.
/// Runs on `webhook_port` (default 8086), bound to 127.0.0.1.
async fn run_webhook_listener(
    app_handle: tauri::AppHandle,
    port: u16,
    stop: Arc<AtomicBool>,
) -> Result<(), String> {
    use tokio::net::TcpListener;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr).await
        .map_err(|e| format!("Failed to bind webhook listener on {}: {}", addr, e))?;

    info!("[whatsapp] Webhook listener started on {}", addr);

    loop {
        if stop.load(Ordering::Relaxed) { break; }

        let accept_result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            listener.accept(),
        ).await;

        let (mut stream, _peer) = match accept_result {
            Ok(Ok(conn)) => conn,
            Ok(Err(e)) => {
                warn!("[whatsapp] Accept error: {}", e);
                continue;
            }
            Err(_) => continue, // Timeout — check stop signal
        };

        // Read the full HTTP request
        let mut buf = vec![0u8; 65536];
        let n = match stream.read(&mut buf).await {
            Ok(n) => n,
            Err(_) => continue,
        };
        let request = String::from_utf8_lossy(&buf[..n]).to_string();

        // Send 200 OK immediately (Evolution expects quick response)
        let response = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK";
        let _ = stream.write_all(response.as_bytes()).await;
        drop(stream);

        // Parse the JSON body (after the blank line)
        let body = if let Some(idx) = request.find("\r\n\r\n") {
            &request[idx + 4..]
        } else {
            continue;
        };

        let payload: serde_json::Value = match serde_json::from_str(body) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Handle different webhook events
        let event = payload["event"].as_str().unwrap_or("");

        match event {
            "qrcode.updated" => {
                let qr = payload["data"]["qrcode"]["base64"].as_str()
                    .or_else(|| payload["data"]["qrcode"].as_str())
                    .unwrap_or("");
                if !qr.is_empty() {
                    let _ = app_handle.emit("whatsapp-status", json!({
                        "kind": "qr_code",
                        "qr": qr,
                        "message": "Scan this QR code with WhatsApp",
                    }));
                }
            }
            "connection.update" => {
                let state = payload["data"]["state"].as_str().unwrap_or("");
                if state == "open" || state == "connected" {
                    let _ = app_handle.emit("whatsapp-status", json!({
                        "kind": "connected",
                        "message": "WhatsApp connected",
                    }));
                    info!("[whatsapp] Connection confirmed via webhook");
                }
            }
            "messages.upsert" => {
                // Process inbound message
                let app = app_handle.clone();
                let msg_payload = payload.clone();
                tauri::async_runtime::spawn(async move {
                    handle_inbound_message(app, msg_payload).await;
                });
            }
            _ => {
                // Ignore other events
            }
        }
    }

    Ok(())
}

// ── Message Handling ───────────────────────────────────────────────────

/// Process an inbound WhatsApp message from the Evolution API webhook.
async fn handle_inbound_message(app_handle: tauri::AppHandle, payload: serde_json::Value) {
    let data = &payload["data"];

    // Extract message details
    let messages = match data.as_array() {
        Some(arr) => arr.clone(),
        None => vec![data.clone()],
    };

    for msg in messages {
        // Skip status messages, reactions, etc.
        let key = &msg["key"];
        let from_me = key["fromMe"].as_bool().unwrap_or(false);
        if from_me { continue; }

        // Extract text content
        let text = msg["message"]["conversation"].as_str()
            .or_else(|| msg["message"]["extendedTextMessage"]["text"].as_str())
            .unwrap_or("");
        if text.is_empty() { continue; }

        // Extract sender info
        let remote_jid = key["remoteJid"].as_str().unwrap_or("");
        let participant = key["participant"].as_str().unwrap_or(remote_jid);
        let is_group = remote_jid.contains("@g.us");

        // Normalize sender ID (strip @s.whatsapp.net)
        let sender_id = participant.split('@').next().unwrap_or(participant).to_string();
        let push_name = msg["pushName"].as_str().unwrap_or(&sender_id).to_string();

        info!("[whatsapp] Message from {} ({}): {}", push_name, sender_id,
            if text.len() > 50 { format!("{}...", &text[..50]) } else { text.to_string() });

        // Load config for access control
        let mut config: WhatsAppConfig = match channels::load_channel_config(&app_handle, CONFIG_KEY) {
            Ok(c) => c,
            Err(e) => { error!("[whatsapp] Load config: {}", e); continue; }
        };

        // Skip groups unless configured
        if is_group && !config.respond_in_groups {
            continue;
        }

        // Access control
        match channels::check_access(
            &config.dm_policy,
            &sender_id,
            &push_name,
            &push_name,
            &config.allowed_users,
            &mut config.pending_users,
        ) {
            Err(denial_msg) => {
                let _ = channels::save_channel_config(&app_handle, CONFIG_KEY, &config);
                let _ = app_handle.emit("whatsapp-status", json!({
                    "kind": "pairing_request",
                    "user_id": &sender_id,
                    "user_name": &push_name,
                }));
                // Send denial message back
                let _ = send_whatsapp_message(&app_handle, remote_jid, &denial_msg).await;
                continue;
            }
            Ok(()) => {}
        }

        MESSAGE_COUNT.fetch_add(1, Ordering::Relaxed);

        // Route to agent
        let agent_id = config.agent_id.as_deref().unwrap_or("default");
        let ctx = "You are chatting via WhatsApp. Keep responses concise and mobile-friendly. \
                   Use WhatsApp formatting: *bold*, _italic_, ~strikethrough~, ```code```. \
                   Avoid very long responses — WhatsApp truncates long messages.";

        let response = channels::run_channel_agent(
            &app_handle, "whatsapp", ctx, text, &sender_id, agent_id,
        ).await;

        match response {
            Ok(reply) if !reply.is_empty() => {
                if let Err(e) = send_whatsapp_message(&app_handle, remote_jid, &reply).await {
                    error!("[whatsapp] Send failed: {}", e);
                }
            }
            Err(e) => {
                error!("[whatsapp] Agent error for {}: {}", sender_id, e);
                let _ = send_whatsapp_message(&app_handle, remote_jid,
                    &format!("Error: {}", e)).await;
            }
            _ => {}
        }
    }
}

// ── Evolution API Helpers ──────────────────────────────────────────────

/// Send a text message via Evolution API.
async fn send_whatsapp_message(
    app_handle: &tauri::AppHandle,
    to_jid: &str,
    text: &str,
) -> Result<(), String> {
    let config: WhatsAppConfig = channels::load_channel_config(app_handle, CONFIG_KEY)?;
    let client = reqwest::Client::new();

    let url = format!("{}/message/sendText/{}", config.api_url, config.instance_name);

    // WhatsApp has no hard character limit, but split very long messages
    let chunks = split_message(text, 4000);

    for chunk in &chunks {
        let body = json!({
            "number": to_jid,
            "text": chunk,
        });

        let resp = client.post(&url)
            .header("apikey", &config.api_key)
            .json(&body)
            .send().await;

        match resp {
            Ok(r) => {
                if !r.status().is_success() {
                    let err_text = r.text().await.unwrap_or_default();
                    warn!("[whatsapp] sendText error: {}", err_text);
                }
            }
            Err(e) => warn!("[whatsapp] sendText failed: {}", e),
        }
    }

    Ok(())
}

fn split_message(text: &str, max_len: usize) -> Vec<String> {
    if text.len() <= max_len {
        return vec![text.to_string()];
    }
    let mut chunks = Vec::new();
    let mut remaining = text;
    while !remaining.is_empty() {
        if remaining.len() <= max_len {
            chunks.push(remaining.to_string());
            break;
        }
        let split_at = remaining[..max_len]
            .rfind('\n')
            .or_else(|| remaining[..max_len].rfind(' '))
            .unwrap_or(max_len);
        chunks.push(remaining[..split_at].to_string());
        remaining = remaining[split_at..].trim_start();
    }
    chunks
}

// ── Config Persistence ─────────────────────────────────────────────────

pub fn load_config(app_handle: &tauri::AppHandle) -> Result<WhatsAppConfig, String> {
    channels::load_channel_config(app_handle, CONFIG_KEY)
}

pub fn save_config(app_handle: &tauri::AppHandle, config: &WhatsAppConfig) -> Result<(), String> {
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
