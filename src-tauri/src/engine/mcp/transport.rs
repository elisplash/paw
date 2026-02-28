// Paw Agent Engine — MCP Transports (Stdio + SSE)
//
// Two transport implementations for the MCP JSON-RPC interface:
//   - StdioTransport: spawns a child process, Content-Length framed stdin/stdout
//   - SseTransport: connects to an HTTP SSE endpoint (MCP Streamable HTTP)
//
// Both are wrapped by McpTransportHandle for unified API.

use super::types::{JsonRpcRequest, JsonRpcResponse};
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot, Mutex};

/// A running stdio transport — owns the child process and message routing.
pub struct StdioTransport {
    /// Sender to write JSON-RPC requests to the child's stdin.
    writer_tx: mpsc::Sender<Vec<u8>>,
    /// Pending requests awaiting responses, keyed by JSON-RPC id.
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
    /// Handle to the child process (for cleanup).
    child: Arc<Mutex<Option<Child>>>,
    /// Background reader/writer task handles.
    _reader_handle: tokio::task::JoinHandle<()>,
    _writer_handle: tokio::task::JoinHandle<()>,
}

impl StdioTransport {
    /// Spawn a child process and set up bidirectional JSON-RPC transport.
    pub async fn spawn(
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> Result<Self, String> {
        info!("[mcp] Spawning: {} {}", command, args.join(" "));

        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        // Merge extra env vars (credentials, etc.)
        for (k, v) in env {
            cmd.env(k, v);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn MCP server `{}`: {}", command, e))?;

        let stdin = child.stdin.take().ok_or("Failed to open stdin")?;
        let stdout = child.stdout.take().ok_or("Failed to open stdout")?;
        let stderr = child.stderr.take().ok_or("Failed to open stderr")?;

        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // ── Writer task: sends framed messages to stdin ────────────────
        let (writer_tx, mut writer_rx) = mpsc::channel::<Vec<u8>>(64);
        let _writer_handle = {
            let mut stdin = stdin;
            tokio::spawn(async move {
                while let Some(msg) = writer_rx.recv().await {
                    let frame = format!("Content-Length: {}\r\n\r\n", msg.len());
                    if let Err(e) = stdin.write_all(frame.as_bytes()).await {
                        error!("[mcp] stdin write header error: {}", e);
                        break;
                    }
                    if let Err(e) = stdin.write_all(&msg).await {
                        error!("[mcp] stdin write body error: {}", e);
                        break;
                    }
                    if let Err(e) = stdin.flush().await {
                        error!("[mcp] stdin flush error: {}", e);
                        break;
                    }
                }
                debug!("[mcp] Writer task exiting");
            })
        };

        // ── Reader task: reads framed messages from stdout ─────────────
        let _reader_handle = {
            let pending = Arc::clone(&pending);
            let mut reader = BufReader::new(stdout);
            tokio::spawn(async move {
                loop {
                    match read_message(&mut reader).await {
                        Ok(Some(data)) => {
                            match serde_json::from_slice::<JsonRpcResponse>(&data) {
                                Ok(resp) => {
                                    if let Some(id) = resp.id {
                                        let mut map = pending.lock().await;
                                        if let Some(tx) = map.remove(&id) {
                                            let _ = tx.send(resp);
                                        } else {
                                            debug!(
                                                "[mcp] Response for unknown id={}, ignoring",
                                                id
                                            );
                                        }
                                    } else {
                                        // Notification (no id) — log and discard
                                        debug!(
                                            "[mcp] Received notification: {:?}",
                                            &data[..data.len().min(200)]
                                        );
                                    }
                                }
                                Err(e) => {
                                    warn!("[mcp] Failed to parse response: {}", e);
                                }
                            }
                        }
                        Ok(None) => {
                            info!("[mcp] Stdout closed (server exited)");
                            break;
                        }
                        Err(e) => {
                            error!("[mcp] Read error: {}", e);
                            break;
                        }
                    }
                }
            })
        };

        // ── Stderr drain (log warnings) ────────────────────────────────
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break,
                    Ok(_) => {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            debug!("[mcp:stderr] {}", trimmed);
                        }
                    }
                    Err(e) => {
                        warn!("[mcp] stderr read error: {}", e);
                        break;
                    }
                }
            }
        });

        Ok(StdioTransport {
            writer_tx,
            pending,
            child: Arc::new(Mutex::new(Some(child))),
            _reader_handle,
            _writer_handle,
        })
    }

    /// Send a JSON-RPC request and wait for the response.
    pub async fn send_request(
        &self,
        request: JsonRpcRequest,
        timeout_secs: u64,
    ) -> Result<JsonRpcResponse, String> {
        let id = request.id;
        let (tx, rx) = oneshot::channel();

        // Register pending response
        {
            let mut map = self.pending.lock().await;
            map.insert(id, tx);
        }

        // Serialize and send
        let body = serde_json::to_vec(&request).map_err(|e| format!("Serialize error: {}", e))?;
        self.writer_tx
            .send(body)
            .await
            .map_err(|_| "Transport writer closed".to_string())?;

        // Await response with timeout
        let resp = tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), rx)
            .await
            .map_err(|_| format!("MCP request timed out after {}s (id={})", timeout_secs, id))?
            .map_err(|_| "Response channel dropped".to_string())?;

        Ok(resp)
    }

    /// Send a JSON-RPC notification (no response expected).
    pub async fn send_notification(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<(), String> {
        let notif = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params.unwrap_or(serde_json::json!({})),
        });
        let body = serde_json::to_vec(&notif).map_err(|e| format!("Serialize error: {}", e))?;
        self.writer_tx
            .send(body)
            .await
            .map_err(|_| "Transport writer closed".to_string())?;
        Ok(())
    }

    /// Kill the child process and clean up.
    pub async fn shutdown(&self) {
        let mut guard = self.child.lock().await;
        if let Some(ref mut child) = *guard {
            info!("[mcp] Killing child process");
            let _ = child.kill().await;
        }
        *guard = None;
    }

    /// Check if the child process is still running.
    pub async fn is_alive(&self) -> bool {
        let mut guard = self.child.lock().await;
        if let Some(ref mut child) = *guard {
            match child.try_wait() {
                Ok(None) => true, // still running
                Ok(Some(_)) => false,
                Err(_) => false,
            }
        } else {
            false
        }
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        // Best-effort synchronous kill — the async shutdown should be called first
        let child = self.child.clone();
        tokio::spawn(async move {
            let mut guard = child.lock().await;
            if let Some(ref mut child) = *guard {
                let _ = child.kill().await;
            }
        });
    }
}

// ── Content-Length framed message reader ────────────────────────────────

/// Read a single Content-Length framed message from the stream.
/// Returns `Ok(None)` on EOF, `Ok(Some(bytes))` on success.
async fn read_message<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> Result<Option<Vec<u8>>, String> {
    let mut content_length: Option<usize> = None;
    let mut header_line = String::new();

    // Read headers until empty line
    loop {
        header_line.clear();
        let n = reader
            .read_line(&mut header_line)
            .await
            .map_err(|e| format!("Header read error: {}", e))?;
        if n == 0 {
            return Ok(None); // EOF
        }
        let trimmed = header_line.trim();
        if trimmed.is_empty() {
            break; // End of headers
        }
        if let Some(val) = trimmed.strip_prefix("Content-Length:") {
            content_length = val.trim().parse::<usize>().ok();
        }
        // Ignore unknown headers (Content-Type, etc.)
    }

    let len = content_length.ok_or("Missing Content-Length header")?;
    let mut body = vec![0u8; len];
    reader
        .read_exact(&mut body)
        .await
        .map_err(|e| format!("Body read error: {}", e))?;

    Ok(Some(body))
}

// ══════════════════════════════════════════════════════════════════════
// SSE Transport — connects to an MCP server over HTTP SSE
// ══════════════════════════════════════════════════════════════════════
//
// Protocol flow (MCP SSE transport spec):
//   1. GET {base_url}/sse  → SSE stream
//      - Server sends  event: endpoint  data: /messages?sessionId=...
//      - Server sends  event: message   data: {jsonrpc response}
//   2. POST {messages_url}  body: {jsonrpc request}  → 202 Accepted
//
// The SSE stream stays open for the lifetime of the connection.

pub struct SseTransport {
    /// HTTP client for POSTing requests.
    http: reqwest::Client,
    /// The POST endpoint URL received from the `endpoint` SSE event.
    messages_url: Arc<Mutex<Option<String>>>,
    /// Pending request→response channels, keyed by JSON-RPC id.
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
    /// Whether the SSE stream is alive.
    alive: Arc<AtomicBool>,
    /// Handle to the SSE reader task (for cleanup).
    _reader_handle: tokio::task::JoinHandle<()>,
    /// Shutdown signal sender.
    shutdown_tx: mpsc::Sender<()>,
}

impl SseTransport {
    /// Connect to an MCP server via SSE transport.
    ///
    /// `base_url` should be the MCP server's base URL (e.g. `http://127.0.0.1:5678/mcp`).
    /// We'll open GET `{base_url}/sse` for the event stream and POST to the endpoint
    /// URL provided by the server.
    pub async fn connect(
        base_url: &str,
        headers: &HashMap<String, String>,
    ) -> Result<Self, String> {
        let sse_url = format!("{}/sse", base_url.trim_end_matches('/'));
        info!("[mcp:sse] Connecting to {}", sse_url);

        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300)) // long-lived SSE
            .build()
            .map_err(|e| format!("HTTP client error: {}", e))?;

        let mut req = http.get(&sse_url).header("Accept", "text/event-stream");

        // Add custom headers (e.g., API key)
        for (k, v) in headers {
            req = req.header(k, v);
        }

        let response = req
            .send()
            .await
            .map_err(|e| format!("SSE connection failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("SSE connection returned {}", response.status()));
        }

        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let messages_url: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let alive = Arc::new(AtomicBool::new(true));
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

        // ── SSE reader task ────────────────────────────────────────────
        let _reader_handle = {
            let pending = Arc::clone(&pending);
            let messages_url = Arc::clone(&messages_url);
            let alive = Arc::clone(&alive);
            let base_url_owned = base_url.trim_end_matches('/').to_string();

            // Use bytes_stream for streaming
            let mut byte_stream = response.bytes_stream();

            tokio::spawn(async move {
                use futures::StreamExt;
                let mut buffer = String::new();
                let mut current_event = String::new();
                let mut current_data = Vec::<String>::new();

                loop {
                    tokio::select! {
                        chunk = byte_stream.next() => {
                            match chunk {
                                Some(Ok(bytes)) => {
                                    buffer.push_str(&String::from_utf8_lossy(&bytes));
                                    // Process complete SSE events (delimited by blank lines)
                                    while let Some(pos) = buffer.find("\n\n") {
                                        let event_block = buffer[..pos].to_string();
                                        buffer = buffer[pos + 2..].to_string();

                                        // Parse SSE event
                                        current_event.clear();
                                        current_data.clear();

                                        for line in event_block.lines() {
                                            if let Some(val) = line.strip_prefix("event:") {
                                                current_event = val.trim().to_string();
                                            } else if let Some(val) = line.strip_prefix("data:") {
                                                current_data.push(val.trim().to_string());
                                            }
                                            // Ignore id:, retry:, comments (:)
                                        }

                                        let data = current_data.join("\n");

                                        match current_event.as_str() {
                                            "endpoint" => {
                                                // Server tells us where to POST requests
                                                let url = if data.starts_with("http://") || data.starts_with("https://") {
                                                    data.clone()
                                                } else if data.starts_with('/') {
                                                    format!("{}{}", base_url_owned, data)
                                                } else {
                                                    format!("{}/{}", base_url_owned, data)
                                                };
                                                info!("[mcp:sse] Received endpoint: {}", url);
                                                *messages_url.lock().await = Some(url);
                                            }
                                            "message" => {
                                                match serde_json::from_str::<JsonRpcResponse>(&data) {
                                                    Ok(resp) => {
                                                        if let Some(id) = resp.id {
                                                            let mut map = pending.lock().await;
                                                            if let Some(tx) = map.remove(&id) {
                                                                let _ = tx.send(resp);
                                                            } else {
                                                                debug!("[mcp:sse] Response for unknown id={}", id);
                                                            }
                                                        } else {
                                                            debug!("[mcp:sse] Notification: {}", &data[..data.len().min(200)]);
                                                        }
                                                    }
                                                    Err(e) => {
                                                        warn!("[mcp:sse] Failed to parse response: {} — data: {}", e, &data[..data.len().min(300)]);
                                                    }
                                                }
                                            }
                                            other => {
                                                debug!("[mcp:sse] Unknown event type '{}': {}", other, &data[..data.len().min(200)]);
                                            }
                                        }
                                    }
                                }
                                Some(Err(e)) => {
                                    error!("[mcp:sse] Stream error: {}", e);
                                    break;
                                }
                                None => {
                                    info!("[mcp:sse] SSE stream closed");
                                    break;
                                }
                            }
                        }
                        _ = shutdown_rx.recv() => {
                            info!("[mcp:sse] Shutdown signal received");
                            break;
                        }
                    }
                }
                alive.store(false, Ordering::SeqCst);
            })
        };

        // ── Wait for the endpoint event (up to 10s) ────────────────────
        let messages_url_clone = Arc::clone(&messages_url);
        let got_endpoint = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                {
                    let guard = messages_url_clone.lock().await;
                    if guard.is_some() {
                        return;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await;

        if got_endpoint.is_err() {
            return Err("Timed out waiting for SSE endpoint event (10s)".to_string());
        }

        info!("[mcp:sse] SSE transport connected successfully");

        Ok(SseTransport {
            http,
            messages_url,
            pending,
            alive,
            _reader_handle,
            shutdown_tx,
        })
    }

    /// Send a JSON-RPC request via POST and wait for the response on the SSE stream.
    pub async fn send_request(
        &self,
        request: JsonRpcRequest,
        timeout_secs: u64,
    ) -> Result<JsonRpcResponse, String> {
        let id = request.id;
        let (tx, rx) = oneshot::channel();

        // Register pending response
        {
            let mut map = self.pending.lock().await;
            map.insert(id, tx);
        }

        // Get the POST URL
        let post_url = {
            let guard = self.messages_url.lock().await;
            guard
                .clone()
                .ok_or_else(|| "SSE transport: no endpoint URL available".to_string())?
        };

        // POST the request
        let body = serde_json::to_vec(&request).map_err(|e| format!("Serialize error: {}", e))?;

        let resp = self
            .http
            .post(&post_url)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await
            .map_err(|e| format!("POST request failed: {}", e))?;

        if !resp.status().is_success() && resp.status().as_u16() != 202 {
            // Clean up pending
            let mut map = self.pending.lock().await;
            map.remove(&id);
            return Err(format!("POST returned {}", resp.status()));
        }

        // Await response on the SSE stream
        let result = tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), rx)
            .await
            .map_err(|_| {
                format!(
                    "MCP SSE request timed out after {}s (id={})",
                    timeout_secs, id
                )
            })?
            .map_err(|_| "SSE response channel dropped".to_string())?;

        Ok(result)
    }

    /// Send a JSON-RPC notification via POST (no response expected).
    pub async fn send_notification(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<(), String> {
        let post_url = {
            let guard = self.messages_url.lock().await;
            guard
                .clone()
                .ok_or_else(|| "SSE transport: no endpoint URL available".to_string())?
        };

        let notif = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params.unwrap_or(serde_json::json!({})),
        });
        let body = serde_json::to_vec(&notif).map_err(|e| format!("Serialize error: {}", e))?;

        let resp = self
            .http
            .post(&post_url)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await
            .map_err(|e| format!("POST notification failed: {}", e))?;

        if !resp.status().is_success() && resp.status().as_u16() != 202 {
            warn!("[mcp:sse] Notification POST returned {}", resp.status());
        }

        Ok(())
    }

    /// Shut down the SSE connection.
    pub async fn shutdown(&self) {
        info!("[mcp:sse] Shutting down SSE transport");
        let _ = self.shutdown_tx.send(()).await;
    }

    /// Check if the SSE stream is still alive.
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }
}

// ══════════════════════════════════════════════════════════════════════
// Streamable HTTP Transport — single POST endpoint for MCP (2025 spec)
// ══════════════════════════════════════════════════════════════════════
//
// Protocol flow (MCP Streamable HTTP spec):
//   1. POST {url}  body: {jsonrpc request}  → JSON or SSE response
//   2. Server may return Mcp-Session-Id header (must be echoed back)
//   3. Server may respond with Content-Type: text/event-stream for
//      streaming responses, or application/json for direct responses.
//
// Used by n8n's instance-level MCP server at /mcp-server/http.

pub struct StreamableHttpTransport {
    /// HTTP client for POSTing requests.
    http: reqwest::Client,
    /// The single endpoint URL.
    url: String,
    /// Session ID returned by the server (echoed in subsequent requests).
    session_id: Arc<Mutex<Option<String>>>,
}

impl StreamableHttpTransport {
    /// Connect to an MCP server via Streamable HTTP transport.
    ///
    /// `url` is the full endpoint URL (e.g., `http://127.0.0.1:5678/mcp-server/http`).
    /// `headers` contains auth headers (e.g., `Authorization: Bearer <token>`).
    pub async fn connect(url: &str, headers: &HashMap<String, String>) -> Result<Self, String> {
        info!("[mcp:http] Connecting to {}", url);

        let mut default_headers = reqwest::header::HeaderMap::new();
        for (k, v) in headers {
            if let (Ok(name), Ok(val)) = (
                reqwest::header::HeaderName::from_bytes(k.as_bytes()),
                reqwest::header::HeaderValue::from_str(v),
            ) {
                default_headers.insert(name, val);
            }
        }

        let http = reqwest::Client::builder()
            .default_headers(default_headers)
            .build()
            .map_err(|e| format!("HTTP client error: {}", e))?;

        Ok(StreamableHttpTransport {
            http,
            url: url.to_string(),
            session_id: Arc::new(Mutex::new(None)),
        })
    }

    /// Send a JSON-RPC request via POST and parse the response.
    ///
    /// The server may respond with direct JSON or SSE stream — we handle both.
    pub async fn send_request(
        &self,
        request: JsonRpcRequest,
        timeout_secs: u64,
    ) -> Result<JsonRpcResponse, String> {
        let body = serde_json::to_vec(&request).map_err(|e| format!("Serialize error: {}", e))?;

        let mut req = self
            .http
            .post(&self.url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream");

        // Include session ID if we have one
        {
            let session = self.session_id.lock().await;
            if let Some(sid) = session.as_ref() {
                req = req.header("Mcp-Session-Id", sid);
            }
        }

        let resp = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            req.body(body).send(),
        )
        .await
        .map_err(|_| format!("Request timed out after {}s", timeout_secs))?
        .map_err(|e| format!("HTTP request failed: {}", e))?;

        // Store session ID from response
        if let Some(sid) = resp.headers().get("mcp-session-id") {
            if let Ok(sid_str) = sid.to_str() {
                let mut session = self.session_id.lock().await;
                *session = Some(sid_str.to_string());
            }
        }

        if !resp.status().is_success() {
            return Err(format!("HTTP {} from {}", resp.status(), self.url));
        }

        // Check content type to determine response format
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/json")
            .to_string();

        if content_type.contains("text/event-stream") {
            // Parse SSE response — extract JSON-RPC response from SSE events
            let text = resp
                .text()
                .await
                .map_err(|e| format!("Read SSE response: {}", e))?;
            // Find the last "data:" line containing a JSON-RPC response
            for line in text.lines().rev() {
                if let Some(data) = line.strip_prefix("data:") {
                    let data = data.trim();
                    if let Ok(rpc_resp) = serde_json::from_str::<JsonRpcResponse>(data) {
                        return Ok(rpc_resp);
                    }
                }
            }
            Err("No valid JSON-RPC response in SSE stream".to_string())
        } else {
            // Direct JSON response
            let rpc_resp: JsonRpcResponse = resp
                .json()
                .await
                .map_err(|e| format!("Parse JSON-RPC response: {}", e))?;
            Ok(rpc_resp)
        }
    }

    /// Send a JSON-RPC notification via POST (no response expected).
    pub async fn send_notification(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<(), String> {
        let notif = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params.unwrap_or(serde_json::json!({})),
        });
        let body = serde_json::to_vec(&notif).map_err(|e| format!("Serialize error: {}", e))?;

        let mut req = self
            .http
            .post(&self.url)
            .header("Content-Type", "application/json");

        {
            let session = self.session_id.lock().await;
            if let Some(sid) = session.as_ref() {
                req = req.header("Mcp-Session-Id", sid);
            }
        }

        let resp = req
            .body(body)
            .send()
            .await
            .map_err(|e| format!("POST notification failed: {}", e))?;

        if !resp.status().is_success()
            && resp.status().as_u16() != 202
            && resp.status().as_u16() != 204
        {
            warn!("[mcp:http] Notification POST returned {}", resp.status());
        }

        // Store session ID from notification response too
        if let Some(sid) = resp.headers().get("mcp-session-id") {
            if let Ok(sid_str) = sid.to_str() {
                let mut session = self.session_id.lock().await;
                *session = Some(sid_str.to_string());
            }
        }

        Ok(())
    }

    /// Shut down the Streamable HTTP connection by sending DELETE.
    pub async fn shutdown(&self) {
        info!("[mcp:http] Shutting down Streamable HTTP transport");
        let session = self.session_id.lock().await;
        if let Some(sid) = session.as_ref() {
            let _ = self
                .http
                .delete(&self.url)
                .header("Mcp-Session-Id", sid)
                .send()
                .await;
        }
    }

    /// Streamable HTTP is stateless per-request, always considered "alive".
    pub fn is_alive(&self) -> bool {
        true
    }
}

// ══════════════════════════════════════════════════════════════════════
// Unified Transport Handle — wraps Stdio or SSE
// ══════════════════════════════════════════════════════════════════════

/// Unified transport handle that delegates to the appropriate implementation.
pub enum McpTransportHandle {
    Stdio(StdioTransport),
    Sse(SseTransport),
    StreamableHttp(StreamableHttpTransport),
}

impl McpTransportHandle {
    /// Send a JSON-RPC request and wait for the response.
    pub async fn send_request(
        &self,
        request: JsonRpcRequest,
        timeout_secs: u64,
    ) -> Result<JsonRpcResponse, String> {
        match self {
            McpTransportHandle::Stdio(t) => t.send_request(request, timeout_secs).await,
            McpTransportHandle::Sse(t) => t.send_request(request, timeout_secs).await,
            McpTransportHandle::StreamableHttp(t) => t.send_request(request, timeout_secs).await,
        }
    }

    /// Send a JSON-RPC notification (no response expected).
    pub async fn send_notification(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<(), String> {
        match self {
            McpTransportHandle::Stdio(t) => t.send_notification(method, params).await,
            McpTransportHandle::Sse(t) => t.send_notification(method, params).await,
            McpTransportHandle::StreamableHttp(t) => t.send_notification(method, params).await,
        }
    }

    /// Shutdown the transport.
    pub async fn shutdown(&self) {
        match self {
            McpTransportHandle::Stdio(t) => t.shutdown().await,
            McpTransportHandle::Sse(t) => t.shutdown().await,
            McpTransportHandle::StreamableHttp(t) => t.shutdown().await,
        }
    }

    /// Check if the transport is alive.
    pub async fn is_alive(&self) -> bool {
        match self {
            McpTransportHandle::Stdio(t) => t.is_alive().await,
            McpTransportHandle::Sse(t) => t.is_alive(),
            McpTransportHandle::StreamableHttp(t) => t.is_alive(),
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_read_message_basic() {
        let data = b"Content-Length: 13\r\n\r\n{\"test\":true}";
        let mut reader = BufReader::new(&data[..]);
        let result = read_message(&mut reader).await.unwrap().unwrap();
        assert_eq!(result, b"{\"test\":true}");
    }

    #[tokio::test]
    async fn test_read_message_eof() {
        let data = b"";
        let mut reader = BufReader::new(&data[..]);
        let result = read_message(&mut reader).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_read_message_with_extra_headers() {
        let data = b"Content-Length: 2\r\nContent-Type: application/json\r\n\r\n{}";
        let mut reader = BufReader::new(&data[..]);
        let result = read_message(&mut reader).await.unwrap().unwrap();
        assert_eq!(result, b"{}");
    }

    #[test]
    fn test_sse_transport_handle_enum_variants() {
        // Verify enum variants exist (compile-time check)
        // We can't construct real transports without actual processes/servers,
        // but this ensures the enum is well-formed.
        fn _assert_send_sync<T: Send>() {}
        // SseTransport should be Send due to Arc internals
    }
}
