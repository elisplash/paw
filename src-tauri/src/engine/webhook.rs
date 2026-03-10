// Paw Agent Engine — Generic Inbound Webhook Server
//
// Phase D: lets external systems (Zapier, n8n, GitHub Actions, cron, curl)
// POST to a local HTTP endpoint and trigger an agent run.
//
// Architecture: raw `tokio::net::TcpListener` (same pattern as webchat/whatsapp),
// no framework dependency. Routes:
//   POST /webhook/:agent_id                  — run agent with JSON body as message
//   POST /webhook/:agent_id/tool/:tool_name  — (future) direct tool execution
//   GET  /webhook/health                     — liveness probe
//
// Auth: bearer token checked on every request (except /health).
// Rate limiting: token-bucket per source IP.
// Response: synchronous agent text reply in JSON body.

use crate::atoms::error::{EngineError, EngineResult};
use crate::engine::channels;
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

// ── Webhook Config ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub enabled: bool,
    /// Address to bind — "127.0.0.1" (localhost) or "0.0.0.0" (all interfaces)
    #[serde(default = "default_bind")]
    pub bind_address: String,
    /// Port to listen on
    #[serde(default = "default_port")]
    pub port: u16,
    /// Bearer token for authentication. Auto-generated if empty.
    pub auth_token: String,
    /// Default agent ID for requests that don't specify one in the URL
    #[serde(default)]
    pub default_agent_id: String,
    /// Max requests per IP per minute (0 = unlimited)
    #[serde(default = "default_rate_limit")]
    pub rate_limit_per_minute: u32,
    /// Allow dangerous tools (same as Phase C channel policy)
    #[serde(default)]
    pub allow_dangerous_tools: bool,
}

fn default_bind() -> String {
    "127.0.0.1".into()
}
fn default_port() -> u16 {
    3940
}
fn default_rate_limit() -> u32 {
    60
}

impl Default for WebhookConfig {
    fn default() -> Self {
        let token: String = uuid::Uuid::new_v4().to_string().replace('-', "");
        WebhookConfig {
            enabled: false,
            bind_address: default_bind(),
            port: default_port(),
            auth_token: token,
            default_agent_id: "default".into(),
            rate_limit_per_minute: default_rate_limit(),
            allow_dangerous_tools: false,
        }
    }
}

// ── Webhook Request / Response Types ───────────────────────────────────

#[derive(Debug, Deserialize)]
struct WebhookRequest {
    /// The message to send to the agent
    message: String,
    /// Optional: override the agent_id from the URL
    #[serde(default)]
    agent_id: Option<String>,
    /// Optional: extra context injected into agent system prompt
    #[serde(default)]
    context: Option<String>,
    /// Optional: user identifier for session isolation
    #[serde(default = "default_user_id")]
    user_id: String,
}

fn default_user_id() -> String {
    "webhook".into()
}

#[derive(Debug, Serialize)]
struct WebhookResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    response: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_id: Option<String>,
}

// ── Global State ───────────────────────────────────────────────────────

static BRIDGE_RUNNING: AtomicBool = AtomicBool::new(false);
static REQUEST_COUNT: AtomicU64 = AtomicU64::new(0);
static STOP_SIGNAL: std::sync::OnceLock<Arc<AtomicBool>> = std::sync::OnceLock::new();

fn get_stop_signal() -> Arc<AtomicBool> {
    STOP_SIGNAL
        .get_or_init(|| Arc::new(AtomicBool::new(false)))
        .clone()
}

const CONFIG_KEY: &str = "webhook_config";

// ── Rate Limiter ───────────────────────────────────────────────────────

struct RateLimiter {
    /// IP → (count, window_start)
    buckets: parking_lot::Mutex<HashMap<String, (u32, Instant)>>,
    limit: u32,
}

impl RateLimiter {
    fn new(limit: u32) -> Self {
        RateLimiter {
            buckets: parking_lot::Mutex::new(HashMap::new()),
            limit,
        }
    }

    /// Returns true if the request is allowed, false if rate-limited.
    fn check(&self, ip: &str) -> bool {
        if self.limit == 0 {
            return true;
        } // unlimited
        let mut map = self.buckets.lock();
        let now = Instant::now();
        let entry = map.entry(ip.to_string()).or_insert((0, now));
        // Reset window if >60s elapsed
        if now.duration_since(entry.1).as_secs() >= 60 {
            *entry = (0, now);
        }
        if entry.0 >= self.limit {
            false
        } else {
            entry.0 += 1;
            true
        }
    }
}

// ── Public API ─────────────────────────────────────────────────────────

pub fn start_bridge(app_handle: tauri::AppHandle) -> EngineResult<()> {
    if BRIDGE_RUNNING.load(Ordering::Relaxed) {
        return Err(EngineError::Config("Webhook server already running".into()));
    }

    let config: WebhookConfig = channels::load_channel_config(&app_handle, CONFIG_KEY)?;
    if !config.enabled {
        return Err(EngineError::Config(
            "Webhook server is disabled — enable it in settings first".into(),
        ));
    }

    let stop = get_stop_signal();
    stop.store(false, Ordering::Relaxed);
    BRIDGE_RUNNING.store(true, Ordering::Relaxed);

    tauri::async_runtime::spawn(async move {
        if let Err(e) = run_server(app_handle.clone(), config).await {
            error!("[webhook] Server crashed: {}", e);
            let _ = app_handle.emit(
                "webhook-status",
                json!({
                    "kind": "error",
                    "message": e.to_string(),
                }),
            );
        }
        BRIDGE_RUNNING.store(false, Ordering::Relaxed);
    });

    Ok(())
}

pub fn stop_bridge() {
    let stop = get_stop_signal();
    stop.store(true, Ordering::Relaxed);
    BRIDGE_RUNNING.store(false, Ordering::Relaxed);
    info!("[webhook] Stop signal sent");
}

pub fn get_status(app_handle: &tauri::AppHandle) -> channels::ChannelStatus {
    let config: WebhookConfig =
        channels::load_channel_config(app_handle, CONFIG_KEY).unwrap_or_default();
    channels::ChannelStatus {
        running: BRIDGE_RUNNING.load(Ordering::Relaxed),
        connected: BRIDGE_RUNNING.load(Ordering::Relaxed) && config.enabled,
        bot_name: Some("Webhook Server".into()),
        bot_id: None,
        message_count: REQUEST_COUNT.load(Ordering::Relaxed),
        allowed_users: vec![],
        pending_users: vec![],
        dm_policy: String::new(),
    }
}

// ── HTTP Server ────────────────────────────────────────────────────────

async fn run_server(app_handle: tauri::AppHandle, config: WebhookConfig) -> EngineResult<()> {
    let stop = get_stop_signal();
    let addr = format!("{}:{}", config.bind_address, config.port);

    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("Bind {}:{} failed: {}", config.bind_address, config.port, e))?;

    if config.bind_address != "127.0.0.1" && config.bind_address != "localhost" {
        warn!("[webhook] Binding to {} — ensure auth_token is strong and consider TLS via Tailscale Funnel", config.bind_address);
    }

    info!("[webhook] Listening on http://{}", addr);
    let _ = app_handle.emit(
        "webhook-status",
        json!({
            "kind": "connected",
            "address": &addr,
        }),
    );

    let config = Arc::new(config);
    let rate_limiter = Arc::new(RateLimiter::new(config.rate_limit_per_minute));

    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }

        let accept =
            tokio::time::timeout(std::time::Duration::from_secs(1), listener.accept()).await;

        match accept {
            Ok(Ok((stream, peer))) => {
                let app = app_handle.clone();
                let cfg = config.clone();
                let rl = rate_limiter.clone();
                tokio::spawn(async move {
                    let peer_ip = peer.ip().to_string();
                    if let Err(e) = handle_request(stream, &peer_ip, app, cfg, rl).await {
                        warn!("[webhook] Request error from {}: {}", peer_ip, e);
                    }
                });
            }
            Ok(Err(e)) => {
                warn!("[webhook] Accept error: {}", e);
            }
            Err(_) => { /* timeout — loop to check stop signal */ }
        }
    }

    info!("[webhook] Server stopped");
    let _ = app_handle.emit("webhook-status", json!({ "kind": "disconnected" }));
    Ok(())
}
// ── SSE Chat Handler ──────────────────────────────────────────────

/// Handle `POST /chat/stream` — streams every EngineEvent (delta, tool_request,
/// tool_result, complete, etc.) as SSE to the caller.
/// This is the primary endpoint for the Pawz VS Code extension.
/// Set `allow_dangerous_tools = true` in webhook config to give the agent
/// the full tool set (read_file, write_file, exec, etc.).
async fn handle_sse_chat(
    stream: &mut tokio::net::TcpStream,
    app_handle: &tauri::AppHandle,
    config: Arc<WebhookConfig>,
    body_str: &str,
) -> EngineResult<()> {
    let req: WebhookRequest = match serde_json::from_str(body_str) {
        Ok(r) => r,
        Err(e) => {
            send_json(
                stream,
                400,
                &WebhookResponse {
                    ok: false,
                    response: None,
                    error: Some(format!("Invalid JSON body: {}", e)),
                    agent_id: None,
                },
            )
            .await?;
            return Ok(());
        }
    };

    let agent_id = req
        .agent_id
        .as_deref()
        .unwrap_or(&config.default_agent_id)
        .to_string();

    // Must match the session_id format used by run_channel_agent:
    // format!("eng-{}-{}-{}", channel_prefix, agent_id, user_id)
    let session_id = format!("eng-webhook-{}-{}", agent_id, req.user_id);
    let session_filter = format!("\"session_id\":\"{}\"", session_id);

    // Subscribe to engine event broadcast BEFORE spawning the agent so no
    // events are missed during the startup window.
    let mut rx = match app_handle.try_state::<crate::engine::state::EngineState>() {
        Some(es) => es.sse_events.subscribe(),
        None => {
            send_json(
                stream,
                503,
                &WebhookResponse {
                    ok: false,
                    response: None,
                    error: Some("Engine not initialised".into()),
                    agent_id: None,
                },
            )
            .await?;
            return Ok(());
        }
    };

    // Commit to SSE — send headers now, no turning back.
    let sse_headers = "HTTP/1.1 200 OK\r\n\
        Content-Type: text/event-stream\r\n\
        Cache-Control: no-cache\r\n\
        Connection: keep-alive\r\n\
        Access-Control-Allow-Origin: *\r\n\
        Access-Control-Allow-Headers: Authorization, Content-Type\r\n\
        \r\n";
    stream
        .write_all(sse_headers.as_bytes())
        .await
        .map_err(|e| format!("SSE header write failed: {}", e))?;

    // Spawn the agent turn in the background.
    let app2 = app_handle.clone();
    let message = req.message.clone();
    let user_id = req.user_id.clone();
    let context = req.context.clone().unwrap_or_else(|| {
        "You are responding via the Pawz VS Code extension. \
         The user is working in their code editor. \
         You have full access to the workspace via read_file, write_file, exec, and all built-in tools. \
         File paths are absolute or relative to the workspace root provided in the conversation context."
            .into()
    });
    let aid = agent_id.clone();
    let allow_dangerous = config.allow_dangerous_tools;
    tokio::spawn(async move {
        let _ = channels::run_channel_agent(
            &app2,
            "webhook",
            &context,
            &message,
            &user_id,
            &aid,
            allow_dangerous,
        )
        .await;
    });

    // Stream events to the SSE client.
    // The agent loop always emits Complete or Error as its final event —
    // that's our termination signal. A keepalive comment keeps the
    // connection alive during long-running tool chains.
    let keepalive_interval = Duration::from_secs(15);
    let max_duration = Duration::from_secs(300); // 5-minute hard cap
    let t0 = Instant::now();

    loop {
        if t0.elapsed() > max_duration {
            let _ = stream
                .write_all(
                    b"data: {\"kind\":\"error\",\"message\":\"Request timeout\"}\
                    \n\n",
                )
                .await;
            break;
        }

        match tokio::time::timeout(keepalive_interval, rx.recv()).await {
            Ok(Ok(payload)) => {
                if !payload.contains(&session_filter) {
                    continue; // event belongs to a different session
                }
                let frame = format!("data: {}\n\n", payload);
                if stream.write_all(frame.as_bytes()).await.is_err() {
                    break; // client disconnected
                }
                let terminal = payload.contains("\"kind\":\"complete\"")
                    || payload.contains("\"kind\":\"error\"");
                if terminal {
                    break;
                }
            }
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(n))) => {
                warn!(
                    "[webhook-sse] Broadcast lagged {} events — client too slow, skipping",
                    n
                );
                // Continue — future events are still readable
            }
            Ok(Err(_)) => break, // broadcast channel closed
            Err(_) => {
                // Keepalive interval hit — send SSE comment to keep connection alive
                if stream.write_all(b": keepalive\n\n").await.is_err() {
                    break;
                }
            }
        }
    }

    REQUEST_COUNT.fetch_add(1, Ordering::Relaxed);
    Ok(())
}
// ── Request Handler ────────────────────────────────────────────────────

async fn handle_request(
    mut stream: tokio::net::TcpStream,
    peer_ip: &str,
    app_handle: tauri::AppHandle,
    config: Arc<WebhookConfig>,
    rate_limiter: Arc<RateLimiter>,
) -> EngineResult<()> {
    // Read the full HTTP request (up to 64KB)
    let mut buf = vec![0u8; 65536];
    let n = stream
        .read(&mut buf)
        .await
        .map_err(|e| format!("Read error: {}", e))?;
    if n == 0 {
        return Ok(());
    }
    let raw = String::from_utf8_lossy(&buf[..n]).to_string();

    // Parse first line: "METHOD /path HTTP/1.x"
    let first_line = raw.lines().next().unwrap_or("");
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    let (method, path) = if parts.len() >= 2 {
        (parts[0], parts[1])
    } else {
        send_json(
            &mut stream,
            400,
            &WebhookResponse {
                ok: false,
                response: None,
                error: Some("Malformed request".into()),
                agent_id: None,
            },
        )
        .await?;
        return Ok(());
    };

    // ── Health check (no auth required) ─────────────────────────────
    if method == "GET" && path == "/webhook/health" {
        let body = json!({ "ok": true, "running": true }).to_string();
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(), body
        );
        stream
            .write_all(resp.as_bytes())
            .await
            .map_err(|e| format!("Write error: {}", e))?;
        return Ok(());
    }

    // ── CORS preflight ──────────────────────────────────────────────
    if method == "OPTIONS" {
        let resp = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: http://localhost:1420\r\nAccess-Control-Allow-Methods: POST, GET, OPTIONS\r\nAccess-Control-Allow-Headers: Authorization, Content-Type\r\nVary: Origin\r\nConnection: close\r\n\r\n";
        stream
            .write_all(resp.as_bytes())
            .await
            .map_err(|e| format!("Write error: {}", e))?;
        return Ok(());
    }

    // ── Auth check ──────────────────────────────────────────────────
    let auth_ok = raw.lines().any(|line| {
        let lower = line.to_lowercase();
        if lower.starts_with("authorization:") {
            let value = line["authorization:".len()..].trim();
            // Accept "Bearer <token>" or raw "<token>"
            let token = value
                .strip_prefix("Bearer ")
                .or_else(|| value.strip_prefix("bearer "))
                .unwrap_or(value);
            token == config.auth_token
        } else {
            false
        }
    });
    if !auth_ok {
        send_json(
            &mut stream,
            401,
            &WebhookResponse {
                ok: false,
                response: None,
                error: Some("Unauthorized — provide Authorization: Bearer <token>".into()),
                agent_id: None,
            },
        )
        .await?;
        return Ok(());
    }

    // ── Rate limiting ───────────────────────────────────────────────
    if !rate_limiter.check(peer_ip) {
        send_json(
            &mut stream,
            429,
            &WebhookResponse {
                ok: false,
                response: None,
                error: Some("Rate limit exceeded — try again later".into()),
                agent_id: None,
            },
        )
        .await?;
        return Ok(());
    }

    // ── Route: POST /chat/stream — SSE (VS Code extension / trusted local clients) ──
    if method == "POST" && path == "/chat/stream" {
        let body_str = raw.split("\r\n\r\n").nth(1).unwrap_or("");
        return handle_sse_chat(&mut stream, &app_handle, config.clone(), body_str).await;
    }

    // ── Route: POST /webhook/:agent_id ──────────────────────────────
    if method == "POST" && path.starts_with("/webhook/") {
        let segments: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        // segments: ["webhook", agent_id] or ["webhook", agent_id, "tool", tool_name]
        let url_agent_id = segments.get(1).unwrap_or(&"");

        if segments.len() >= 4 && segments[2] == "tool" {
            // POST /webhook/:agent_id/tool/:tool_name — future: direct tool execution
            send_json(
                &mut stream,
                501,
                &WebhookResponse {
                    ok: false,
                    response: None,
                    error: Some("Direct tool execution not yet implemented".into()),
                    agent_id: None,
                },
            )
            .await?;
            return Ok(());
        }

        // Parse JSON body
        let body_str = raw.split("\r\n\r\n").nth(1).unwrap_or("");
        let webhook_req: WebhookRequest = match serde_json::from_str(body_str) {
            Ok(r) => r,
            Err(e) => {
                send_json(
                    &mut stream,
                    400,
                    &WebhookResponse {
                        ok: false,
                        response: None,
                        error: Some(format!("Invalid JSON body: {}", e)),
                        agent_id: None,
                    },
                )
                .await?;
                return Ok(());
            }
        };

        // Resolve agent ID: body override > URL segment > config default
        let agent_id = webhook_req
            .agent_id
            .as_deref()
            .unwrap_or(if url_agent_id.is_empty() {
                &config.default_agent_id
            } else {
                url_agent_id
            });

        let context = webhook_req.context.as_deref().unwrap_or(
            "You are responding to an automated webhook request. Keep responses concise and structured. \
             Use JSON formatting if the caller is likely a machine."
        );

        info!(
            "[webhook] POST from {} → agent={} user={} msg_len={}",
            peer_ip,
            agent_id,
            webhook_req.user_id,
            webhook_req.message.len()
        );

        REQUEST_COUNT.fetch_add(1, Ordering::Relaxed);

        // Emit activity event for frontend
        let _ = app_handle.emit(
            "webhook-activity",
            json!({
                "peer": peer_ip,
                "agent_id": agent_id,
                "user_id": webhook_req.user_id,
                "message_preview": if webhook_req.message.len() > 80 {
                    format!("{}…", &webhook_req.message[..80])
                } else {
                    webhook_req.message.clone()
                },
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }),
        );

        // Run agent
        let result = channels::run_channel_agent(
            &app_handle,
            "webhook",
            context,
            &webhook_req.message,
            &webhook_req.user_id,
            agent_id,
            config.allow_dangerous_tools,
        )
        .await;

        match result {
            Ok(text) => {
                send_json(
                    &mut stream,
                    200,
                    &WebhookResponse {
                        ok: true,
                        response: Some(text),
                        error: None,
                        agent_id: Some(agent_id.to_string()),
                    },
                )
                .await?;
            }
            Err(e) => {
                error!("[webhook] Agent error: {}", e);
                send_json(
                    &mut stream,
                    500,
                    &WebhookResponse {
                        ok: false,
                        response: None,
                        error: Some(format!("Agent error: {}", e)),
                        agent_id: Some(agent_id.to_string()),
                    },
                )
                .await?;
            }
        }
        return Ok(());
    }

    // ── 404 for anything else ───────────────────────────────────────
    send_json(
        &mut stream,
        404,
        &WebhookResponse {
            ok: false,
            response: None,
            error: Some(format!("Not found: {} {}", method, path)),
            agent_id: None,
        },
    )
    .await?;
    Ok(())
}

// ── Helpers ────────────────────────────────────────────────────────────

async fn send_json(
    stream: &mut tokio::net::TcpStream,
    status: u16,
    body: &WebhookResponse,
) -> EngineResult<()> {
    let status_text = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        _ => "Unknown",
    };
    let json = serde_json::to_string(body).map_err(|e| format!("Serialize error: {}", e))?;
    let resp = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: http://localhost:1420\r\nVary: Origin\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status, status_text, json.len(), json
    );
    stream
        .write_all(resp.as_bytes())
        .await
        .map_err(|e| format!("Write error: {}", e))?;
    Ok(())
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webhook_config_default() {
        let config = WebhookConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.bind_address, "127.0.0.1");
        assert_eq!(config.port, 3940);
        assert!(!config.auth_token.is_empty());
        assert_eq!(config.default_agent_id, "default");
        assert_eq!(config.rate_limit_per_minute, 60);
        assert!(!config.allow_dangerous_tools);
    }

    #[test]
    fn test_rate_limiter_allows_within_limit() {
        let rl = RateLimiter::new(3);
        assert!(rl.check("1.2.3.4"));
        assert!(rl.check("1.2.3.4"));
        assert!(rl.check("1.2.3.4"));
        assert!(!rl.check("1.2.3.4")); // 4th request blocked
    }

    #[test]
    fn test_rate_limiter_unlimited() {
        let rl = RateLimiter::new(0);
        for _ in 0..1000 {
            assert!(rl.check("1.2.3.4"));
        }
    }

    #[test]
    fn test_rate_limiter_separate_ips() {
        let rl = RateLimiter::new(2);
        assert!(rl.check("1.1.1.1"));
        assert!(rl.check("1.1.1.1"));
        assert!(!rl.check("1.1.1.1")); // blocked
        assert!(rl.check("2.2.2.2")); // different IP, allowed
        assert!(rl.check("2.2.2.2"));
        assert!(!rl.check("2.2.2.2")); // blocked
    }

    #[test]
    fn test_webhook_request_deserialize() {
        let json = r#"{"message": "hello", "user_id": "test-user"}"#;
        let req: WebhookRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.message, "hello");
        assert_eq!(req.user_id, "test-user");
        assert!(req.agent_id.is_none());
        assert!(req.context.is_none());
    }

    #[test]
    fn test_webhook_request_minimal() {
        let json = r#"{"message": "ping"}"#;
        let req: WebhookRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.message, "ping");
        assert_eq!(req.user_id, "webhook"); // default
    }

    #[test]
    fn test_webhook_response_serialize() {
        let resp = WebhookResponse {
            ok: true,
            response: Some("Hello!".into()),
            error: None,
            agent_id: Some("default".into()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"ok\":true"));
        assert!(json.contains("\"response\":\"Hello!\""));
        assert!(!json.contains("\"error\"")); // None fields skipped
    }
}
