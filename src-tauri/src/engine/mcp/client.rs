// Paw Agent Engine — MCP Client
//
// Manages the connection to a single MCP server.
// Handles initialize handshake, tools/list, tools/call.
// Supports both Stdio and SSE transports via McpTransportHandle.

use super::transport::McpTransportHandle;
use super::types::*;
use log::info;
use std::sync::atomic::{AtomicU64, Ordering};

/// MCP protocol version we advertise.
const PROTOCOL_VERSION: &str = "2024-11-05";
/// Default timeout for JSON-RPC requests (seconds).
const DEFAULT_TIMEOUT: u64 = 30;
/// Timeout for tool calls — tools can be slow (seconds).
const TOOL_CALL_TIMEOUT: u64 = 120;

/// A connected MCP client for a single server.
pub struct McpClient {
    /// The server config this client was created from.
    pub config: McpServerConfig,
    /// Underlying transport (Stdio or SSE).
    transport: McpTransportHandle,
    /// Monotonically increasing request ID.
    next_id: AtomicU64,
    /// Server's declared capabilities (from initialize response).
    pub server_info: Option<McpServerInfo>,
    /// Cached tools from the last `tools/list` call.
    pub tools: Vec<McpToolDef>,
}

impl McpClient {
    /// Connect to the MCP server using the appropriate transport,
    /// perform the initialize handshake, and fetch the initial tool list.
    pub async fn connect(config: McpServerConfig) -> Result<Self, String> {
        info!(
            "[mcp] Connecting to server '{}' via {:?}",
            config.name, config.transport
        );

        let transport = match config.transport {
            McpTransport::Stdio => {
                use super::transport::StdioTransport;
                let stdio =
                    StdioTransport::spawn(&config.command, &config.args, &config.env).await?;
                McpTransportHandle::Stdio(stdio)
            }
            McpTransport::Sse => {
                use super::transport::SseTransport;
                if config.url.is_empty() {
                    return Err("SSE transport requires a URL".to_string());
                }
                // Pass env vars as headers (e.g., API keys)
                let sse = SseTransport::connect(&config.url, &config.env).await?;
                McpTransportHandle::Sse(sse)
            }
            McpTransport::StreamableHttp => {
                use super::transport::StreamableHttpTransport;
                if config.url.is_empty() {
                    return Err("Streamable HTTP transport requires a URL".to_string());
                }
                // Pass env vars as headers (e.g., Authorization: Bearer <token>)
                let http = StreamableHttpTransport::connect(&config.url, &config.env).await?;
                McpTransportHandle::StreamableHttp(http)
            }
        };

        let mut client = McpClient {
            config,
            transport,
            next_id: AtomicU64::new(1),
            server_info: None,
            tools: vec![],
        };

        // ── Initialize handshake ───────────────────────────────────────
        client.initialize().await?;

        // ── Fetch tool list ────────────────────────────────────────────
        client.refresh_tools().await?;

        Ok(client)
    }

    /// MCP `initialize` handshake.
    async fn initialize(&mut self) -> Result<(), String> {
        let params = InitializeParams {
            protocol_version: PROTOCOL_VERSION.into(),
            capabilities: McpClientCapabilities::default(),
            client_info: McpClientInfo {
                name: "OpenPawz".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
        };

        let req = JsonRpcRequest::new(
            self.next_id(),
            "initialize",
            Some(serde_json::to_value(&params).map_err(|e| e.to_string())?),
        );

        let resp = self.transport.send_request(req, DEFAULT_TIMEOUT).await?;

        if let Some(err) = resp.error {
            return Err(format!(
                "MCP initialize failed: {} (code={})",
                err.message, err.code
            ));
        }

        if let Some(result) = resp.result {
            let init: InitializeResult =
                serde_json::from_value(result).map_err(|e| format!("Parse init result: {}", e))?;
            info!(
                "[mcp] Server '{}' initialized (protocol={})",
                self.config.name, init.protocol_version
            );
            self.server_info = init.server_info;
        }

        // Send `initialized` notification (required by spec)
        self.transport
            .send_notification("notifications/initialized", None)
            .await?;

        Ok(())
    }

    /// Fetch (or refresh) the tool list from the server.
    pub async fn refresh_tools(&mut self) -> Result<(), String> {
        let req = JsonRpcRequest::new(self.next_id(), "tools/list", None);
        let resp = self.transport.send_request(req, DEFAULT_TIMEOUT).await?;

        if let Some(err) = resp.error {
            // Server may not support tools — that's OK
            if err.code == -32601 {
                info!("[mcp] Server '{}' does not expose tools", self.config.name);
                self.tools = vec![];
                return Ok(());
            }
            return Err(format!(
                "tools/list failed: {} (code={})",
                err.message, err.code
            ));
        }

        if let Some(result) = resp.result {
            let list: ToolsListResult =
                serde_json::from_value(result).map_err(|e| format!("Parse tools/list: {}", e))?;
            info!(
                "[mcp] Server '{}' exposes {} tools",
                self.config.name,
                list.tools.len()
            );
            self.tools = list.tools;
        } else {
            self.tools = vec![];
        }

        Ok(())
    }

    /// Call a tool on this MCP server.
    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<String, String> {
        let params = ToolCallParams {
            name: tool_name.into(),
            arguments,
        };

        let req = JsonRpcRequest::new(
            self.next_id(),
            "tools/call",
            Some(serde_json::to_value(&params).map_err(|e| e.to_string())?),
        );

        let resp = self.transport.send_request(req, TOOL_CALL_TIMEOUT).await?;

        if let Some(err) = resp.error {
            return Err(format!(
                "tools/call '{}' failed: {} (code={})",
                tool_name, err.message, err.code
            ));
        }

        let result_val = resp
            .result
            .ok_or_else(|| format!("tools/call '{}': empty result", tool_name))?;

        let tool_result: ToolCallResult = serde_json::from_value(result_val)
            .map_err(|e| format!("Parse tools/call result: {}", e))?;

        if tool_result.is_error {
            let error_text = extract_text_content(&tool_result.content);
            return Err(error_text);
        }

        Ok(extract_text_content(&tool_result.content))
    }

    /// Check if the underlying transport is still alive.
    pub async fn is_alive(&self) -> bool {
        self.transport.is_alive().await
    }

    /// Gracefully shut down the server.
    pub async fn shutdown(&self) {
        info!("[mcp] Shutting down server '{}'", self.config.name);
        self.transport.shutdown().await;
    }

    /// Get the next request ID.
    fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }
}

// ── Helpers ────────────────────────────────────────────────────────────

/// Extract text content from MCP content blocks, concatenated.
fn extract_text_content(content: &[McpContent]) -> String {
    content
        .iter()
        .filter_map(|c| match c {
            McpContent::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_text_content_single() {
        let content = vec![McpContent::Text {
            text: "Hello".into(),
        }];
        assert_eq!(extract_text_content(&content), "Hello");
    }

    #[test]
    fn test_extract_text_content_multi() {
        let content = vec![
            McpContent::Text {
                text: "Line 1".into(),
            },
            McpContent::Image {
                data: "base64...".into(),
                mime_type: "image/png".into(),
            },
            McpContent::Text {
                text: "Line 2".into(),
            },
        ];
        assert_eq!(extract_text_content(&content), "Line 1\nLine 2");
    }

    #[test]
    fn test_extract_text_content_empty() {
        let content: Vec<McpContent> = vec![];
        assert_eq!(extract_text_content(&content), "");
    }
}
