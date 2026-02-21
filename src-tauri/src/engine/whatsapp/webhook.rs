// WhatsApp Bridge — Webhook HTTP Listener
// run_webhook_listener

use crate::atoms::error::EngineResult;
use log::{info, warn};
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Emitter;
use super::messages::handle_inbound_message;

/// Minimal HTTP listener that receives webhooks from Evolution API.
/// Runs on `webhook_port` (default 8086), bound to 127.0.0.1.
pub(crate) async fn run_webhook_listener(
    app_handle: tauri::AppHandle,
    port: u16,
    stop: Arc<AtomicBool>,
) -> EngineResult<()> {
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
