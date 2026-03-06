// Paw Agent Engine — MCP (Model Context Protocol) Types
//
// Protocol types for the MCP JSON-RPC interface.
// Spec: https://spec.modelcontextprotocol.io/

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Server Configuration (persisted) ───────────────────────────────────

/// User-configured MCP server definition — stored in DB.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Unique identifier (user-chosen or auto-generated).
    pub id: String,
    /// Human-readable display name.
    pub name: String,
    /// Transport type.
    #[serde(default)]
    pub transport: McpTransport,
    /// Command to spawn (stdio transport).
    #[serde(default)]
    pub command: String,
    /// Arguments for the command.
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables passed to the child process.
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// URL for SSE transport (ignored for stdio).
    #[serde(default)]
    pub url: String,
    /// Whether this server is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum McpTransport {
    #[default]
    Stdio,
    Sse,
    /// MCP Streamable HTTP transport — single POST endpoint.
    /// Used by n8n's instance-level MCP server (`/mcp-server/http`).
    #[serde(alias = "streamable_http", alias = "http")]
    StreamableHttp,
}

// ── JSON-RPC 2.0 Framing ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    pub fn new(id: u64, method: &str, params: Option<serde_json::Value>) -> Self {
        JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id,
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

// ── MCP Protocol Messages ──────────────────────────────────────────────

/// Client capabilities sent during `initialize`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpClientCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roots: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling: Option<serde_json::Value>,
}

/// Parameters for the `initialize` request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    pub protocol_version: String,
    pub capabilities: McpClientCapabilities,
    pub client_info: McpClientInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpClientInfo {
    pub name: String,
    pub version: String,
}

/// Result of a successful `initialize` response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: String,
    #[serde(default)]
    pub capabilities: McpServerCapabilities,
    #[serde(default)]
    pub server_info: Option<McpServerInfo>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpServerCapabilities {
    #[serde(default)]
    pub tools: Option<serde_json::Value>,
    #[serde(default)]
    pub resources: Option<serde_json::Value>,
    #[serde(default)]
    pub prompts: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
}

// ── tools/list ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsListResult {
    pub tools: Vec<McpToolDef>,
}

/// A single tool exposed by an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolDef {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// JSON Schema describing the tool's input.
    #[serde(default = "default_empty_object")]
    pub input_schema: serde_json::Value,
}

fn default_empty_object() -> serde_json::Value {
    serde_json::json!({"type": "object", "properties": {}})
}

// ── tools/call ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallParams {
    pub name: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub content: Vec<McpContent>,
    #[serde(default, rename = "isError")]
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum McpContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
    #[serde(rename = "resource")]
    Resource { resource: serde_json::Value },
}

// ── Runtime state (not persisted) ──────────────────────────────────────

/// Runtime status of a connected MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerStatus {
    pub id: String,
    pub name: String,
    pub connected: bool,
    pub error: Option<String>,
    pub tool_count: usize,
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_config_defaults() {
        let json = r#"{"id":"test","name":"Test","command":"echo"}"#;
        let cfg: McpServerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.transport, McpTransport::Stdio);
        assert!(cfg.enabled);
        assert!(cfg.args.is_empty());
        assert!(cfg.env.is_empty());
    }

    #[test]
    fn test_transport_serde() {
        let t = McpTransport::Stdio;
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(json, "\"stdio\"");
        let t2: McpTransport = serde_json::from_str("\"sse\"").unwrap();
        assert_eq!(t2, McpTransport::Sse);
    }

    #[test]
    fn test_jsonrpc_request_serde() {
        let req = JsonRpcRequest::new(1, "tools/list", None);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"method\":\"tools/list\""));
        assert!(!json.contains("\"params\"")); // skip_serializing_if None
    }

    #[test]
    fn test_jsonrpc_response_success() {
        let json = r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert!(resp.error.is_none());
        assert!(resp.result.is_some());
    }

    #[test]
    fn test_jsonrpc_response_error() {
        let json =
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32601);
    }

    #[test]
    fn test_mcp_tool_def_serde() {
        let json = r#"{"name":"read_file","description":"Read a file","inputSchema":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}}"#;
        let tool: McpToolDef = serde_json::from_str(json).unwrap();
        assert_eq!(tool.name, "read_file");
        assert_eq!(tool.description.as_deref(), Some("Read a file"));
        assert!(tool.input_schema["properties"]["path"].is_object());
    }

    #[test]
    fn test_tool_call_result_text() {
        let json = r#"{"content":[{"type":"text","text":"Hello world"}],"isError":false}"#;
        let result: ToolCallResult = serde_json::from_str(json).unwrap();
        assert!(!result.is_error);
        assert_eq!(result.content.len(), 1);
        match &result.content[0] {
            McpContent::Text { text } => assert_eq!(text, "Hello world"),
            _ => panic!("Expected Text content"),
        }
    }

    #[test]
    fn test_initialize_params() {
        let params = InitializeParams {
            protocol_version: "2024-11-05".into(),
            capabilities: McpClientCapabilities::default(),
            client_info: McpClientInfo {
                name: "OpenPawz".into(),
                version: "0.1.0".into(),
            },
        };
        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("protocolVersion"));
        assert!(json.contains("clientInfo"));
    }

    // ══════════════════════════════════════════════════════════════════
    // MCP Protocol Contract Tests
    //
    // These verify our types correctly round-trip through the MCP
    // JSON-RPC protocol.  If the MCP spec changes or n8n emits
    // different shapes, these tests catch it.
    // ══════════════════════════════════════════════════════════════════

    #[test]
    fn initialize_result_parses_real_n8n_response() {
        // Real n8n MCP initialize response
        let json = r#"{
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {"listChanged": true}
            },
            "serverInfo": {
                "name": "n8n",
                "version": "1.82.1"
            }
        }"#;
        let result: InitializeResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.protocol_version, "2024-11-05");
        assert!(result.capabilities.tools.is_some());
        assert!(result.server_info.is_some());
        assert_eq!(result.server_info.as_ref().unwrap().name, "n8n");
        assert_eq!(
            result.server_info.as_ref().unwrap().version.as_deref(),
            Some("1.82.1")
        );
    }

    #[test]
    fn initialize_result_handles_minimal_server() {
        // Some MCP servers return minimal capabilities
        let json = r#"{
            "protocolVersion": "2024-11-05",
            "capabilities": {}
        }"#;
        let result: InitializeResult = serde_json::from_str(json).unwrap();
        assert!(result.capabilities.tools.is_none());
        assert!(result.server_info.is_none());
    }

    #[test]
    fn tools_list_parses_real_n8n_tools() {
        // Real n8n MCP tools/list response with actual tool shapes
        let json = r#"{
            "tools": [
                {
                    "name": "gmail_send_email",
                    "description": "Send an email using Gmail",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "to": {"type": "string", "description": "Recipient"},
                            "subject": {"type": "string"},
                            "body": {"type": "string"}
                        },
                        "required": ["to", "subject", "body"]
                    }
                },
                {
                    "name": "slack_post_message",
                    "description": "Post a message to Slack",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "channel": {"type": "string"},
                            "text": {"type": "string"}
                        },
                        "required": ["channel", "text"]
                    }
                },
                {
                    "name": "minimal_tool"
                }
            ]
        }"#;
        let result: ToolsListResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.tools.len(), 3);
        assert_eq!(result.tools[0].name, "gmail_send_email");
        assert_eq!(
            result.tools[0].description.as_deref(),
            Some("Send an email using Gmail")
        );
        assert!(result.tools[0].input_schema["required"].is_array());

        // Minimal tool should still parse
        assert_eq!(result.tools[2].name, "minimal_tool");
        assert!(result.tools[2].description.is_none());
    }

    #[test]
    fn tools_list_handles_empty() {
        let json = r#"{"tools": []}"#;
        let result: ToolsListResult = serde_json::from_str(json).unwrap();
        assert!(result.tools.is_empty());
    }

    #[test]
    fn tool_call_result_parses_success() {
        let json = r#"{
            "content": [
                {"type": "text", "text": "Email sent successfully to user@example.com"}
            ],
            "isError": false
        }"#;
        let result: ToolCallResult = serde_json::from_str(json).unwrap();
        assert!(!result.is_error);
        assert_eq!(result.content.len(), 1);
        match &result.content[0] {
            McpContent::Text { text } => {
                assert!(text.contains("Email sent"));
            }
            _ => panic!("Expected Text content"),
        }
    }

    #[test]
    fn tool_call_result_parses_error() {
        let json = r#"{
            "content": [
                {"type": "text", "text": "Authentication failed: invalid token"}
            ],
            "isError": true
        }"#;
        let result: ToolCallResult = serde_json::from_str(json).unwrap();
        assert!(result.is_error);
        match &result.content[0] {
            McpContent::Text { text } => assert!(text.contains("Authentication failed")),
            _ => panic!("Expected Text content"),
        }
    }

    #[test]
    fn tool_call_result_parses_mixed_content() {
        let json = r#"{
            "content": [
                {"type": "text", "text": "Here is the chart:"},
                {"type": "image", "data": "iVBORw0KGgo...", "mimeType": "image/png"},
                {"type": "text", "text": "Generated from workflow data"}
            ],
            "isError": false
        }"#;
        let result: ToolCallResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.content.len(), 3);

        // First and third should be text
        match &result.content[0] {
            McpContent::Text { text } => assert!(text.contains("chart")),
            _ => panic!("Expected Text"),
        }

        // Second should be image
        match &result.content[1] {
            McpContent::Image { data, mime_type } => {
                assert!(data.starts_with("iVBOR"));
                assert_eq!(mime_type, "image/png");
            }
            _ => panic!("Expected Image"),
        }
    }

    #[test]
    fn tool_call_result_handles_resource_content() {
        let json = r#"{
            "content": [
                {"type": "resource", "resource": {"uri": "file:///tmp/output.csv", "mimeType": "text/csv"}}
            ],
            "isError": false
        }"#;
        let result: ToolCallResult = serde_json::from_str(json).unwrap();
        match &result.content[0] {
            McpContent::Resource { resource } => {
                assert_eq!(resource["uri"].as_str().unwrap(), "file:///tmp/output.csv");
            }
            _ => panic!("Expected Resource content"),
        }
    }

    #[test]
    fn tool_call_result_is_error_defaults_false() {
        let json = r#"{
            "content": [{"type": "text", "text": "ok"}]
        }"#;
        let result: ToolCallResult = serde_json::from_str(json).unwrap();
        assert!(!result.is_error); // serde default
    }

    #[test]
    fn jsonrpc_request_with_params() {
        let params = serde_json::json!({
            "name": "gmail_send_email",
            "arguments": {"to": "test@example.com", "subject": "Hi"}
        });
        let req = JsonRpcRequest::new(42, "tools/call", Some(params.clone()));
        let json = serde_json::to_string(&req).unwrap();

        // Must contain all required JSON-RPC fields
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"id\":42"));
        assert!(json.contains("\"method\":\"tools/call\""));
        assert!(json.contains("\"params\""));

        // Round-trip
        let parsed: JsonRpcRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, 42);
        assert_eq!(parsed.method, "tools/call");
        assert_eq!(parsed.params.unwrap()["name"], "gmail_send_email");
    }

    #[test]
    fn jsonrpc_response_with_result_and_no_error() {
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 3,
            "result": {
                "tools": [{"name": "test_tool", "inputSchema": {"type": "object"}}]
            }
        }"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.id, Some(3));
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools[0]["name"], "test_tool");
    }

    #[test]
    fn jsonrpc_error_method_not_found() {
        // MCP spec: -32601 means method not found (server doesn't support tools)
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 5,
            "error": {"code": -32601, "message": "Method not found"}
        }"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32601);
    }

    #[test]
    fn jsonrpc_error_with_data() {
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 6,
            "error": {
                "code": -32000,
                "message": "Internal error",
                "data": {"details": "workflow execution failed", "nodeId": "n1"}
            }
        }"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32000);
        assert!(err.data.is_some());
        assert_eq!(err.data.unwrap()["details"], "workflow execution failed");
    }

    #[test]
    fn streamable_http_transport_alias() {
        // Verify all transport name aliases deserialize correctly
        let cases = vec![
            ("\"streamablehttp\"", McpTransport::StreamableHttp),
            ("\"streamable_http\"", McpTransport::StreamableHttp),
            ("\"http\"", McpTransport::StreamableHttp),
            ("\"stdio\"", McpTransport::Stdio),
            ("\"sse\"", McpTransport::Sse),
        ];
        for (input, expected) in cases {
            let transport: McpTransport = serde_json::from_str(input).unwrap();
            assert_eq!(transport, expected, "Failed to parse: {}", input);
        }
    }

    #[test]
    fn tool_200_cap_test() {
        // Verify MAX_TOOLS_PER_SERVER behavior isn't embedded in types
        // but our ToolsListResult can handle large lists
        let mut tools = Vec::new();
        for i in 0..250 {
            tools.push(McpToolDef {
                name: format!("tool_{}", i),
                description: Some(format!("Tool number {}", i)),
                input_schema: serde_json::json!({"type": "object"}),
            });
        }
        let result = ToolsListResult { tools };

        // Types should accept any count — capping happens in client logic
        assert_eq!(result.tools.len(), 250);

        // Simulate the client cap
        let capped: Vec<McpToolDef> = result.tools.into_iter().take(200).collect();
        assert_eq!(capped.len(), 200);
        assert_eq!(capped[199].name, "tool_199");
    }

    #[test]
    fn mcp_server_config_full_round_trip() {
        let config = McpServerConfig {
            id: "n8n-mcp".into(),
            name: "n8n Engine".into(),
            transport: McpTransport::StreamableHttp,
            command: String::new(),
            args: vec![],
            env: std::collections::HashMap::from([(
                "Authorization".into(),
                "Bearer eyJ...".into(),
            )]),
            url: "http://localhost:5678/mcp-server/http".into(),
            enabled: true,
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: McpServerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "n8n-mcp");
        assert_eq!(parsed.transport, McpTransport::StreamableHttp);
        assert_eq!(parsed.url, "http://localhost:5678/mcp-server/http");
        assert!(parsed.env.contains_key("Authorization"));
    }
}
