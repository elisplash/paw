// WhatsApp Bridge — Inbound Message Handling
// handle_inbound_message

use crate::engine::channels;
use log::{debug, error};
use serde_json::json;
use std::sync::atomic::Ordering;
use tauri::Emitter;
use super::bridge::MESSAGE_COUNT;
use super::config::{WhatsAppConfig, CONFIG_KEY};
use super::evolution_api::send_whatsapp_message;

/// Process an inbound WhatsApp message from the Evolution API webhook.
pub(crate) async fn handle_inbound_message(app_handle: tauri::AppHandle, payload: serde_json::Value) {
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

        debug!("[whatsapp] Message from {} ({}): {}", push_name, sender_id,
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
        if let Err(denial_msg) = channels::check_access(
            &config.dm_policy,
            &sender_id,
            &push_name,
            &push_name,
            &config.allowed_users,
            &mut config.pending_users,
        ) {
            let denial_str = denial_msg.to_string();
            let _ = channels::save_channel_config(&app_handle, CONFIG_KEY, &config);
            let _ = app_handle.emit("whatsapp-status", json!({
                "kind": "pairing_request",
                "user_id": &sender_id,
                "user_name": &push_name,
            }));
            // Send denial message back
            let _ = send_whatsapp_message(&app_handle, remote_jid, &denial_str).await;
            continue;
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
