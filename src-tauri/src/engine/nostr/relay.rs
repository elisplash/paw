// Paw Agent Engine — Nostr Relay WebSocket Loop
//
// Connects to a single Nostr relay, subscribes to mentions and DMs,
// handles incoming events, and publishes signed replies.

use super::crypto::{sign_event, build_reply_event, nip04_encrypt, nip04_decrypt};
use super::{NostrConfig, get_stop_signal, MESSAGE_COUNT};

use crate::engine::channels;
use log::{debug, info, warn, error};
use serde_json::json;
use std::sync::atomic::Ordering;
use tauri::Emitter;
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};
use futures::{SinkExt, StreamExt};
use crate::atoms::error::{EngineResult, EngineError};

// ── Single Relay WebSocket Loop ────────────────────────────────────────

pub(crate) async fn run_relay_loop(
    app_handle: &tauri::AppHandle,
    config: &NostrConfig,
    relay_url: &str,
    pubkey_hex: &str,
    secret_key: &[u8],
) -> EngineResult<()> {
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
    // kind 1 = text notes, kind 4 = encrypted DMs (NIP-04)
    let sub_id = format!("paw-{}", &pubkey_hex[..8]);
    let req = json!(["REQ", &sub_id, {
        "#p": [pubkey_hex],
        "kinds": [1, 4],
        "since": chrono::Utc::now().timestamp() - 10, // Only new events
    }]);
    ws_tx.send(WsMessage::Text(req.to_string())).await
        .map_err(|e| EngineError::Channel { channel: "nostr".into(), message: e.to_string() })?;

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
                if kind != 1 && kind != 4 { continue; }

                let sender_pk = event["pubkey"].as_str().unwrap_or("").to_string();
                if sender_pk == pubkey_hex { continue; } // Skip own events

                let raw_content = event["content"].as_str().unwrap_or("").to_string();
                if raw_content.is_empty() { continue; }

                // Decrypt kind-4 DMs (NIP-04), pass through kind-1 text notes
                let (content, is_dm) = if kind == 4 {
                    match nip04_decrypt(secret_key, &sender_pk, &raw_content) {
                        Ok(pt) => (pt, true),
                        Err(e) => {
                            warn!("[nostr] Failed to decrypt DM from {}...{}: {}",
                                &sender_pk[..sender_pk.len().min(8)],
                                &sender_pk[sender_pk.len().saturating_sub(4)..], e);
                            continue;
                        }
                    }
                } else {
                    (raw_content, false)
                };
                if content.is_empty() { continue; }

                debug!("[nostr] {} from {}...{}: {}",
                    if is_dm { "DM" } else { "Event" },
                    &sender_pk[..8], &sender_pk[sender_pk.len()-4..],
                    if content.len() > 50 { format!("{}...", &content[..50]) } else { content.clone() });

                // Access control
                if let Err(_denial_msg) = channels::check_access(
                    &current_config.dm_policy,
                    &sender_pk,
                    &sender_pk[..12],
                    &sender_pk[..12],
                    &current_config.allowed_users,
                    &mut current_config.pending_users,
                ) {
                    let _ = channels::save_channel_config(app_handle, super::CONFIG_KEY, &current_config);
                    let _ = app_handle.emit("nostr-status", json!({
                        "kind": "pairing_request",
                        "pubkey": &sender_pk,
                    }));
                    // Don't reply to denied users on public Nostr
                    continue;
                }

                MESSAGE_COUNT.fetch_add(1, Ordering::Relaxed);

                let agent_id = current_config.agent_id.as_deref().unwrap_or("default");
                let ctx = if is_dm {
                    "You are replying to a private Nostr DM (NIP-04 encrypted). \
                     Use plain text. Keep responses concise. \
                     Your reply will be encrypted and sent as a kind-4 DM."
                } else {
                    "You are chatting via Nostr (a decentralized social network). \
                     Use plain text. Keep responses concise. \
                     Your reply will be published as a kind-1 note."
                };

                let response = channels::run_channel_agent(
                    app_handle, "nostr", ctx, &content, &sender_pk, agent_id,
                ).await;

                match response {
                    Ok(reply) if !reply.is_empty() => {
                        if is_dm {
                            // Encrypt and send as kind-4 DM (NIP-04)
                            match nip04_encrypt(secret_key, &sender_pk, &reply) {
                                Ok(encrypted) => {
                                    let tags = json!([["p", &sender_pk]]);
                                    match sign_event(secret_key, pubkey_hex, 4, &tags, &encrypted) {
                                        Ok(dm_event) => {
                                            let publish = json!(["EVENT", dm_event]);
                                            if let Err(e) = ws_tx.send(WsMessage::Text(publish.to_string())).await {
                                                warn!("[nostr] Failed to send DM: {}", e);
                                            }
                                        }
                                        Err(e) => error!("[nostr] Failed to sign DM: {}", e),
                                    }
                                }
                                Err(e) => error!("[nostr] Failed to encrypt DM reply: {}", e),
                            }
                        } else {
                            // Public reply (kind-1)
                            match build_reply_event(secret_key, pubkey_hex, &reply, &event_id, &sender_pk) {
                                Ok(reply_event) => {
                                    let publish = json!(["EVENT", reply_event]);
                                    if let Err(e) = ws_tx.send(WsMessage::Text(publish.to_string())).await {
                                        warn!("[nostr] Failed to publish reply: {}", e);
                                    }
                                }
                                Err(e) => error!("[nostr] Failed to sign reply: {}", e),
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
            if let Ok(fresh) = channels::load_channel_config::<NostrConfig>(app_handle, super::CONFIG_KEY) {
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
