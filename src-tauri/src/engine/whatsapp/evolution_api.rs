// WhatsApp Bridge — Evolution API Helpers
// create_evolution_instance, extract_qr_from_response, delete_evolution_instance,
// connect_evolution_instance, send_whatsapp_message

use crate::engine::channels;
use log::{info, warn};
use serde_json::json;
use super::config::{WhatsAppConfig, CONFIG_KEY};
use crate::atoms::error::EngineResult;

// ── Instance Management ────────────────────────────────────────────────

/// Create a WhatsApp instance in Evolution API and get the QR code.
pub(crate) async fn create_evolution_instance(config: &WhatsAppConfig) -> EngineResult<String> {
    let client = reqwest::Client::new();
    let url = format!("{}/instance/create", config.api_url);

    // v1.x format: webhook is a plain string URL, no "integration" field.
    // Webhook events are configured via container env vars (WEBHOOK_GLOBAL_*).
    // Provide a unique token per instance to avoid "Token already exists" collisions.
    let instance_token = format!("paw-{}", &uuid::Uuid::new_v4().to_string().replace('-', "")[..12]);
    let body = json!({
        "instanceName": config.instance_name,
        "token": instance_token,
        "qrcode": true,
        "webhook": format!("http://host.docker.internal:{}/webhook/whatsapp", config.webhook_port),
    });

    info!("[whatsapp] Creating instance '{}' with token '{}'", config.instance_name, instance_token);

    let resp = client.post(&url)
        .header("apikey", &config.api_key)
        .json(&body)
        .send().await?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    info!("[whatsapp] Instance create response [{}]: {}", status, &text[..text.len().min(500)]);

    if !status.is_success() {
        // Instance with this name already exists — delete it and retry
        let lower = text.to_lowercase();
        let is_instance_exists = lower.contains("instance") && (lower.contains("already") || lower.contains("exists"));
        let is_token_exists = lower.contains("token") && lower.contains("already");

        if is_instance_exists || is_token_exists {
            info!("[whatsapp] Instance/token conflict, deleting instance and recreating...");
            delete_evolution_instance(config).await;

            // Generate a fresh token for the retry
            let retry_token = format!("paw-{}", &uuid::Uuid::new_v4().to_string().replace('-', "")[..12]);
            let retry_body = json!({
                "instanceName": config.instance_name,
                "token": retry_token,
                "qrcode": true,
                "webhook": format!("http://host.docker.internal:{}/webhook/whatsapp", config.webhook_port),
            });

            // Retry create after delete
            let resp2 = client.post(&url)
                .header("apikey", &config.api_key)
                .json(&retry_body)
                .send().await?;

            let status2 = resp2.status();
            let text2 = resp2.text().await.unwrap_or_default();
            info!("[whatsapp] Instance create (retry) response [{}]: {}", status2, &text2[..text2.len().min(500)]);

            if !status2.is_success() {
                return Err(format!("Create instance failed after delete ({}): {}", status2, text2).into());
            }

            let resp_json: serde_json::Value = serde_json::from_str(&text2).unwrap_or_default();
            return Ok(extract_qr_from_response(&resp_json));
        }
        return Err(format!("Create instance failed ({}): {}", status, text).into());
    }

    info!("[whatsapp] Instance create response [{}]: {}", status, &text[..text.len().min(500)]);

    // Parse QR code from response
    let resp_json: serde_json::Value = serde_json::from_str(&text)?;

    Ok(extract_qr_from_response(&resp_json))
}

/// Extract QR code base64 from various Evolution API response formats.
pub(crate) fn extract_qr_from_response(resp: &serde_json::Value) -> String {
    // v1.x create: { "qrcode": { "base64": "data:image/..." } }
    // v1.x connect: { "base64": "data:image/..." }
    // Also try nested: { "qrcode": "data:image/..." } or { "qrcode": { "code": "...", "base64": "..." } }
    let qr = resp["qrcode"]["base64"].as_str()
        .or_else(|| resp["base64"].as_str())
        .or_else(|| resp["qrcode"].as_str().filter(|s| s.starts_with("data:")))
        .unwrap_or("")
        .to_string();

    if qr.is_empty() {
        // Log what we got so we can debug unexpected response formats
        let qr_field = &resp["qrcode"];
        warn!("[whatsapp] QR extraction returned empty. qrcode field type: {}, keys: {:?}",
            if qr_field.is_object() { "object" } else if qr_field.is_string() { "string" } else if qr_field.is_null() { "null" } else { "other" },
            qr_field.as_object().map(|o| o.keys().collect::<Vec<_>>())
        );
    } else {
        info!("[whatsapp] QR code extracted ({} bytes, starts with: {})",
            qr.len(), &qr[..qr.len().min(40)]);
    }

    qr
}

/// Delete an existing Evolution API instance.
pub(crate) async fn delete_evolution_instance(config: &WhatsAppConfig) {
    let client = reqwest::Client::new();
    let url = format!("{}/instance/delete/{}", config.api_url, config.instance_name);

    match client.delete(&url)
        .header("apikey", &config.api_key)
        .send().await
    {
        Ok(resp) => {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            info!("[whatsapp] Delete instance response [{}]: {}", status, &text[..text.len().min(200)]);
        }
        Err(e) => {
            warn!("[whatsapp] Delete instance failed: {}", e);
        }
    }
    // Brief pause to let the API settle
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
}

/// Connect an existing Evolution API instance (fallback if delete+create fails).
#[allow(dead_code)]
pub(crate) async fn connect_evolution_instance(config: &WhatsAppConfig) -> EngineResult<String> {
    let client = reqwest::Client::new();
    let url = format!("{}/instance/connect/{}", config.api_url, config.instance_name);

    let resp = client.get(&url)
        .header("apikey", &config.api_key)
        .send().await?;

    let text = resp.text().await.unwrap_or_default();
    info!("[whatsapp] Connect instance response: {}", &text[..text.len().min(500)]);
    let resp_json: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();

    Ok(extract_qr_from_response(&resp_json))
}

// ── Message Sending ────────────────────────────────────────────────────

/// Send a text message via Evolution API.
pub(crate) async fn send_whatsapp_message(
    app_handle: &tauri::AppHandle,
    to_jid: &str,
    text: &str,
) -> EngineResult<()> {
    let config: WhatsAppConfig = channels::load_channel_config(app_handle, CONFIG_KEY)?;
    let client = reqwest::Client::new();

    let url = format!("{}/message/sendText/{}", config.api_url, config.instance_name);

    // WhatsApp has no hard character limit, but split very long messages
    let chunks = channels::split_message(text, 4000);

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
