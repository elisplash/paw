// commands/webhook.rs — Tauri IPC commands for the generic webhook server (Phase D)

use crate::engine::webhook::{self, WebhookConfig};
use crate::engine::channels;

/// Start the webhook HTTP server.
#[tauri::command]
pub async fn engine_webhook_start(
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    webhook::start_bridge(app_handle).map_err(|e| e.to_string())
}

/// Stop the webhook HTTP server.
#[tauri::command]
pub fn engine_webhook_stop() -> Result<(), String> {
    webhook::stop_bridge();
    Ok(())
}

/// Get webhook server status.
#[tauri::command]
pub fn engine_webhook_status(
    app_handle: tauri::AppHandle,
) -> Result<channels::ChannelStatus, String> {
    Ok(webhook::get_status(&app_handle))
}

/// Read webhook configuration.
#[tauri::command]
pub fn engine_webhook_get_config(
    app_handle: tauri::AppHandle,
) -> Result<WebhookConfig, String> {
    channels::load_channel_config::<WebhookConfig>(&app_handle, "webhook_config")
        .map_err(|e| e.to_string())
}

/// Save webhook configuration.
#[tauri::command]
pub fn engine_webhook_set_config(
    app_handle: tauri::AppHandle,
    config: WebhookConfig,
) -> Result<(), String> {
    channels::save_channel_config(&app_handle, "webhook_config", &config)
        .map_err(|e| e.to_string())
}

/// Regenerate the webhook auth token (returns the new token).
#[tauri::command]
pub fn engine_webhook_regenerate_token(
    app_handle: tauri::AppHandle,
) -> Result<String, String> {
    let mut config: WebhookConfig = channels::load_channel_config(&app_handle, "webhook_config")
        .map_err(|e| e.to_string())?;
    config.auth_token = uuid::Uuid::new_v4().to_string().replace('-', "");
    let new_token = config.auth_token.clone();
    channels::save_channel_config(&app_handle, "webhook_config", &config)
        .map_err(|e| e.to_string())?;
    Ok(new_token)
}
