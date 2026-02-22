// Paw Agent Engine — MCP Stdio Transport
//
// Spawns a child process, communicates via JSON-RPC over stdin/stdout
// using Content-Length framed messages (same framing as LSP).

use super::types::{JsonRpcRequest, JsonRpcResponse};
use log::{debug, error, info, warn};
use std::collections::HashMap;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot, Mutex};
use std::sync::Arc;

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

        let mut child = cmd.spawn().map_err(|e| {
            format!(
                "Failed to spawn MCP server `{}`: {}",
                command, e
            )
        })?;

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
        let body =
            serde_json::to_vec(&notif).map_err(|e| format!("Serialize error: {}", e))?;
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
                Ok(None) => true,  // still running
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
        let data =
            b"Content-Length: 2\r\nContent-Type: application/json\r\n\r\n{}";
        let mut reader = BufReader::new(&data[..]);
        let result = read_message(&mut reader).await.unwrap().unwrap();
        assert_eq!(result, b"{}");
    }
}
