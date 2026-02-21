// WhatsApp Bridge — Core Bridge Lifecycle
// statics, get_stop_signal, start_bridge, stop_bridge, get_status, run_whatsapp_bridge

use crate::engine::channels::{self, ChannelStatus};
use log::{info, error, warn};
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use tauri::Emitter;
use super::config::{WhatsAppConfig, CONFIG_KEY};
use super::docker::ensure_evolution_container;
use super::evolution_api::create_evolution_instance;
use super::webhook::run_webhook_listener;
use crate::atoms::error::EngineResult;

// ── Global State ───────────────────────────────────────────────────────

pub(crate) static BRIDGE_RUNNING: AtomicBool = AtomicBool::new(false);
/// Each start_bridge() call increments this. Old bridge tasks compare their
/// generation against the current value to know if they've been superseded.
static BRIDGE_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub(crate) static MESSAGE_COUNT: AtomicI64 = AtomicI64::new(0);
static STOP_SIGNAL: std::sync::OnceLock<Arc<AtomicBool>> = std::sync::OnceLock::new();

pub(crate) fn get_stop_signal() -> Arc<AtomicBool> {
    STOP_SIGNAL.get_or_init(|| Arc::new(AtomicBool::new(false))).clone()
}

// ── Bridge Control ─────────────────────────────────────────────────────

pub fn start_bridge(app_handle: tauri::AppHandle) -> EngineResult<()> {
    // If bridge is already running, stop it first so Start always works
    if BRIDGE_RUNNING.load(Ordering::Relaxed) {
        info!("[whatsapp] Bridge already running — restarting");
        stop_bridge();
        // Give the old bridge a moment to wind down
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    let mut config: WhatsAppConfig = channels::load_channel_config(&app_handle, CONFIG_KEY)?;
    if !config.enabled {
        return Err("WhatsApp bridge is disabled. Enable it in Channels settings.".into());
    }
    // Ensure API key is never empty (old configs may have been saved without one)
    if config.api_key.is_empty() {
        config.api_key = format!("paw-wa-{}", &uuid::Uuid::new_v4().to_string().replace('-', "")[..16]);
        let _ = channels::save_channel_config(&app_handle, CONFIG_KEY, &config);
    }

    let stop = get_stop_signal();
    stop.store(false, Ordering::Relaxed);
    BRIDGE_RUNNING.store(true, Ordering::Relaxed);

    // Increment generation so old bridge tasks know they've been superseded
    let my_gen = BRIDGE_GENERATION.fetch_add(1, Ordering::Relaxed) + 1;

    info!("[whatsapp] Starting bridge via Evolution API (gen {})", my_gen);

    let app = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        let is_current = || BRIDGE_GENERATION.load(Ordering::Relaxed) == my_gen;

        if let Err(e) = run_whatsapp_bridge(app.clone(), config).await {
            // Only emit error if we're still the current bridge (not superseded)
            if is_current() {
                error!("[whatsapp] Bridge crashed: {}", e);
                let _ = app.emit("whatsapp-status", json!({
                    "kind": "error",
                    "message": format!("{}", e),
                }));
            } else {
                info!("[whatsapp] Old bridge (gen {}) exited — superseded", my_gen);
            }
        }

        // Only emit disconnected if we're still the current bridge
        if is_current() {
            let _ = app.emit("whatsapp-status", json!({
                "kind": "disconnected",
            }));
        }

        BRIDGE_RUNNING.store(false, Ordering::Relaxed);
        info!("[whatsapp] Bridge stopped (gen {})", my_gen);
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

// ── Main Bridge Loop ───────────────────────────────────────────────────

/// The main bridge loop:
/// 1. Ensure Docker container is running
/// 2. Create/connect WhatsApp instance (get QR code)
/// 3. Start local webhook HTTP listener
/// 4. Route inbound messages through the agent loop
pub(crate) async fn run_whatsapp_bridge(app_handle: tauri::AppHandle, mut config: WhatsAppConfig) -> EngineResult<()> {
    let stop = get_stop_signal();

    // Step 1: Ensure backend service is available and container is running
    let _ = app_handle.emit("whatsapp-status", json!({
        "kind": "starting",
        "message": "Setting up WhatsApp...",
    }));

    let container_id = ensure_evolution_container(&app_handle, &config).await?;
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
    // Note: disconnected event is emitted by the spawned task wrapper,
    // not here, so the generation check can gate it properly.

    Ok(())
}
