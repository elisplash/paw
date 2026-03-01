// Paw Agent Engine — MCP Server Registry
//
// Manages the lifecycle of all configured MCP servers:
// connect, disconnect, health-check, restart, and tool dispatch.
// Also handles auto-registration of the embedded n8n engine as an MCP server.

use super::client::McpClient;
use super::types::*;
use crate::atoms::types::{FunctionDefinition, ToolDefinition};
use log::info;
use std::collections::HashMap;

/// Well-known server ID for the auto-registered n8n integration engine.
pub const N8N_MCP_SERVER_ID: &str = "n8n";

/// The MCP server registry. Thread-safe via Arc<tokio::sync::Mutex<McpRegistry>>
/// stored in EngineState.
#[derive(Default)]
pub struct McpRegistry {
    /// Connected MCP clients, keyed by server config ID.
    clients: HashMap<String, McpClient>,
}

impl McpRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Connect to an MCP server. Replaces any existing connection with same ID.
    pub async fn connect(&mut self, config: McpServerConfig) -> Result<(), String> {
        let id = config.id.clone();

        // Disconnect existing if present
        if let Some(old) = self.clients.remove(&id) {
            old.shutdown().await;
        }

        let client = McpClient::connect(config).await?;
        self.clients.insert(id, client);
        Ok(())
    }

    /// Disconnect a specific server.
    pub async fn disconnect(&mut self, id: &str) {
        if let Some(client) = self.clients.remove(id) {
            client.shutdown().await;
        }
    }

    /// Disconnect all servers.
    pub async fn disconnect_all(&mut self) {
        let keys: Vec<String> = self.clients.keys().cloned().collect();
        for key in keys {
            if let Some(client) = self.clients.remove(&key) {
                client.shutdown().await;
            }
        }
    }

    /// Get all MCP-provided tools as Paw `ToolDefinition`s.
    /// Tool names are prefixed with `mcp_{server_id}_` to avoid collisions.
    pub fn all_tool_definitions(&self) -> Vec<ToolDefinition> {
        let mut defs = Vec::new();
        for (server_id, client) in &self.clients {
            for tool in &client.tools {
                defs.push(mcp_tool_to_paw_def(server_id, tool));
            }
        }
        defs
    }

    /// Get tool definitions for specific server IDs only.
    pub fn tool_definitions_for(&self, server_ids: &[String]) -> Vec<ToolDefinition> {
        let mut defs = Vec::new();
        for sid in server_ids {
            if let Some(client) = self.clients.get(sid) {
                for tool in &client.tools {
                    defs.push(mcp_tool_to_paw_def(sid, tool));
                }
            }
        }
        defs
    }

    /// Execute an MCP tool call. The `tool_name` should include the
    /// `mcp_{server_id}_` prefix so we can route to the correct server.
    pub async fn execute_tool(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> Option<Result<String, String>> {
        // Parse prefix: mcp_{server_id}_{original_name}
        let stripped = tool_name.strip_prefix("mcp_")?;
        let (server_id, original_name) = find_server_and_tool(stripped, &self.clients)?;

        let client = self.clients.get(server_id)?;
        Some(client.call_tool(original_name, arguments.clone()).await)
    }

    /// Status of all configured servers.
    pub fn status_list(&self) -> Vec<McpServerStatus> {
        self.clients
            .values()
            .map(|c| McpServerStatus {
                id: c.config.id.clone(),
                name: c.config.name.clone(),
                connected: true, // if it's in the map, it's considered connected
                error: None,
                tool_count: c.tools.len(),
            })
            .collect()
    }

    /// Get the list of connected server IDs.
    pub fn connected_ids(&self) -> Vec<String> {
        self.clients.keys().cloned().collect()
    }

    /// Check if a specific server is connected.
    pub fn is_connected(&self, id: &str) -> bool {
        self.clients.contains_key(id)
    }

    /// Refresh tool list for a specific server.
    pub async fn refresh_tools(&mut self, id: &str) -> Result<(), String> {
        let client = self
            .clients
            .get_mut(id)
            .ok_or_else(|| format!("Server '{}' not connected", id))?;
        client.refresh_tools().await
    }

    /// Register the embedded n8n engine as an MCP server.
    ///
    /// n8n's instance-level MCP endpoint is at `{n8n_url}/mcp-server/http`
    /// using the Streamable HTTP transport. Auth requires a dedicated MCP
    /// access token (NOT the N8N_API_KEY). We try:
    ///   1. MCP token (from `retrieve_mcp_token()`) at `/mcp-server/http`
    ///   2. API key as Bearer token at `/mcp-server/http` (some versions)
    ///   3. SSE at `/mcp/sse` (legacy fallback)
    ///
    /// After connecting, all n8n-provided tools appear as `mcp_n8n_{tool_name}`.
    pub async fn register_n8n(
        &mut self,
        n8n_url: &str,
        api_key: &str,
        mcp_token: Option<&str>,
    ) -> Result<usize, String> {
        let base = n8n_url.trim_end_matches('/');
        let mcp_http_url = format!("{}/mcp-server/http", base);

        // ── Strategy 1: Streamable HTTP with MCP token ─────────────────
        if let Some(token) = mcp_token {
            if !token.is_empty() {
                info!(
                    "[mcp] Auto-registering n8n as MCP server at {} (MCP token)",
                    mcp_http_url
                );

                let mut headers = HashMap::new();
                headers.insert("Authorization".to_string(), format!("Bearer {}", token));

                let config = McpServerConfig {
                    id: N8N_MCP_SERVER_ID.to_string(),
                    name: "n8n Integrations".to_string(),
                    transport: McpTransport::StreamableHttp,
                    command: String::new(),
                    args: vec![],
                    env: headers,
                    url: mcp_http_url.clone(),
                    enabled: true,
                };

                match self.connect(config).await {
                    Ok(()) => {
                        let tool_count = self
                            .clients
                            .get(N8N_MCP_SERVER_ID)
                            .map(|c| c.tools.len())
                            .unwrap_or(0);
                        info!(
                            "[mcp] n8n MCP registered via Streamable HTTP (MCP token) — {} tools",
                            tool_count
                        );
                        return Ok(tool_count);
                    }
                    Err(e) => {
                        info!("[mcp] MCP token auth failed: {}", e);
                    }
                }
            }
        }

        // ── Strategy 2: Streamable HTTP with API key ───────────────────
        if !api_key.is_empty() {
            info!("[mcp] Trying Streamable HTTP at {} (API key)", mcp_http_url);

            let mut headers = HashMap::new();
            headers.insert("Authorization".to_string(), format!("Bearer {}", api_key));

            let config = McpServerConfig {
                id: N8N_MCP_SERVER_ID.to_string(),
                name: "n8n Integrations".to_string(),
                transport: McpTransport::StreamableHttp,
                command: String::new(),
                args: vec![],
                env: headers,
                url: mcp_http_url,
                enabled: true,
            };

            match self.connect(config).await {
                Ok(()) => {
                    let tool_count = self
                        .clients
                        .get(N8N_MCP_SERVER_ID)
                        .map(|c| c.tools.len())
                        .unwrap_or(0);
                    info!(
                        "[mcp] n8n MCP registered via Streamable HTTP (API key) — {} tools",
                        tool_count
                    );
                    return Ok(tool_count);
                }
                Err(e) => {
                    info!("[mcp] API key auth at /mcp-server/http failed: {}", e);
                }
            }
        }

        // ── Strategy 3: SSE at /mcp/sse (legacy/older n8n) ─────────────
        let mcp_sse_url = format!("{}/mcp", base);
        info!("[mcp] Trying legacy SSE transport at {}/sse", mcp_sse_url);

        let mut sse_env = HashMap::new();
        if !api_key.is_empty() {
            sse_env.insert("X-N8N-API-KEY".to_string(), api_key.to_string());
            sse_env.insert("Authorization".to_string(), format!("Bearer {}", api_key));
        }
        if let Some(token) = mcp_token {
            if !token.is_empty() {
                sse_env.insert("Authorization".to_string(), format!("Bearer {}", token));
            }
        }

        let sse_config = McpServerConfig {
            id: N8N_MCP_SERVER_ID.to_string(),
            name: "n8n Integrations".to_string(),
            transport: McpTransport::Sse,
            command: String::new(),
            args: vec![],
            env: sse_env,
            url: mcp_sse_url,
            enabled: true,
        };

        self.connect(sse_config).await?;

        let tool_count = self
            .clients
            .get(N8N_MCP_SERVER_ID)
            .map(|c| c.tools.len())
            .unwrap_or(0);

        info!(
            "[mcp] n8n MCP server registered via SSE — {} tools discovered",
            tool_count
        );

        Ok(tool_count)
    }

    /// Check if the n8n MCP server is currently registered.
    pub fn is_n8n_registered(&self) -> bool {
        self.clients.contains_key(N8N_MCP_SERVER_ID)
    }

    /// Disconnect the n8n MCP server.
    pub async fn disconnect_n8n(&mut self) {
        self.disconnect(N8N_MCP_SERVER_ID).await;
    }
}

// ── Conversion helpers ─────────────────────────────────────────────────

/// Convert an MCP tool definition to a Paw ToolDefinition.
/// The tool name is prefixed with `mcp_{server_id}_` to namespace it.
///
/// For n8n tools, applies extra remapping to produce cleaner names.
/// n8n's MCP exposes workflow-level tools (search_workflows, execute_workflow,
/// get_workflow_details) which become:
///   Raw:   `search_workflows`  → `mcp_n8n_search_workflows`
///   Raw:   `execute_workflow`  → `mcp_n8n_execute_workflow`
fn mcp_tool_to_paw_def(server_id: &str, tool: &McpToolDef) -> ToolDefinition {
    let (clean_name, description) = if server_id == N8N_MCP_SERVER_ID {
        // n8n tool names are often PascalCase like "Gmail_SendEmail" or "SendSlackMessage"
        // Remap to snake_case for consistency
        let snake = pascal_to_snake(&tool.name);
        let prefixed = format!("mcp_n8n_{}", snake);
        let desc = tool
            .description
            .as_deref()
            .unwrap_or("(no description)")
            .to_string()
            + " [n8n automation]";
        (prefixed, desc)
    } else {
        let prefixed = format!("mcp_{}_{}", server_id, tool.name);
        let desc = tool
            .description
            .as_deref()
            .unwrap_or("(no description)")
            .to_string()
            + format!(" [MCP: {}]", server_id).as_str();
        (prefixed, desc)
    };

    ToolDefinition {
        tool_type: "function".into(),
        function: FunctionDefinition {
            name: clean_name,
            description,
            parameters: tool.input_schema.clone(),
        },
    }
}

/// Convert PascalCase / camelCase / mixed names to snake_case.
///
/// Examples:
///   "SendSlackMessage"  → "send_slack_message"
///   "Gmail_SendEmail"   → "gmail_send_email"
///   "awsS3Upload"       → "aws_s3_upload"
///   "HTTPRequest"       → "http_request"
///   "already_snake"     → "already_snake"
fn pascal_to_snake(name: &str) -> String {
    let mut result = String::with_capacity(name.len() + 8);
    let chars: Vec<char> = name.chars().collect();

    for (i, &ch) in chars.iter().enumerate() {
        if ch == '_' || ch == '-' {
            if !result.ends_with('_') {
                result.push('_');
            }
            continue;
        }
        if ch.is_uppercase() {
            // Insert underscore before uppercase if:
            // - Not at the start
            // - Previous char was lowercase or digit, OR
            // - Next char is lowercase (handles "HTTPRequest" → "http_request")
            if i > 0 && !result.ends_with('_') {
                let prev = chars[i - 1];
                let next_lower = chars.get(i + 1).is_some_and(|c| c.is_lowercase());
                if prev.is_lowercase()
                    || prev.is_ascii_digit()
                    || (prev.is_uppercase() && next_lower)
                {
                    result.push('_');
                }
            }
            result.push(ch.to_lowercase().next().unwrap_or(ch));
        } else {
            result.push(ch);
        }
    }

    result
}

/// Given a tool name with the `mcp_` prefix stripped (i.e. `{server_id}_{tool_name}`),
/// find the matching server and original tool name.
/// We need this because server IDs themselves may contain underscores.
fn find_server_and_tool<'a>(
    stripped: &'a str,
    clients: &'a HashMap<String, McpClient>,
) -> Option<(&'a str, &'a str)> {
    // Try matching against known server IDs (longest match first for safety)
    let mut ids: Vec<&String> = clients.keys().collect();
    ids.sort_by_key(|b| std::cmp::Reverse(b.len())); // longest first

    for id in ids {
        if let Some(rest) = stripped.strip_prefix(id.as_str()) {
            if let Some(tool_name) = rest.strip_prefix('_') {
                return Some((id.as_str(), tool_name));
            }
        }
    }
    None
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_tool_to_paw_def() {
        let tool = McpToolDef {
            name: "read_file".into(),
            description: Some("Read a file from disk".into()),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"}
                },
                "required": ["path"]
            }),
        };

        let def = mcp_tool_to_paw_def("github", &tool);
        assert_eq!(def.function.name, "mcp_github_read_file");
        assert!(def.function.description.contains("Read a file"));
        assert!(def.function.description.contains("[MCP: github]"));
        assert_eq!(def.tool_type, "function");
    }

    #[test]
    fn test_mcp_tool_n8n_remapping() {
        let tool = McpToolDef {
            name: "Gmail_SendEmail".into(),
            description: Some("Send an email via Gmail".into()),
            input_schema: serde_json::json!({"type": "object"}),
        };
        let def = mcp_tool_to_paw_def("n8n", &tool);
        // n8n MCP uses snake_case tool names (search_workflows, execute_workflow)
        // so pascal_to_snake is a no-op, prefix is just mcp_n8n_
        assert_eq!(def.function.name, "mcp_n8n_gmail_send_email");
        assert!(def.function.description.contains("[n8n automation]"));
    }

    #[test]
    fn test_mcp_tool_no_description() {
        let tool = McpToolDef {
            name: "ping".into(),
            description: None,
            input_schema: serde_json::json!({"type": "object"}),
        };
        let def = mcp_tool_to_paw_def("test", &tool);
        assert!(def.function.description.contains("(no description)"));
    }

    #[test]
    fn test_pascal_to_snake() {
        assert_eq!(pascal_to_snake("SendSlackMessage"), "send_slack_message");
        assert_eq!(pascal_to_snake("Gmail_SendEmail"), "gmail_send_email");
        assert_eq!(pascal_to_snake("awsS3Upload"), "aws_s3_upload");
        assert_eq!(pascal_to_snake("HTTPRequest"), "http_request");
        assert_eq!(pascal_to_snake("already_snake"), "already_snake");
        assert_eq!(pascal_to_snake("simpleWord"), "simple_word");
        assert_eq!(pascal_to_snake("A"), "a");
        assert_eq!(pascal_to_snake("ABCDef"), "abc_def");
    }

    #[test]
    fn test_find_server_and_tool() {
        let stripped = "github_read_file";
        assert_eq!(
            stripped
                .strip_prefix("github")
                .and_then(|r| r.strip_prefix('_')),
            Some("read_file")
        );
    }

    #[test]
    fn test_registry_new_is_empty() {
        let reg = McpRegistry::new();
        assert!(reg.all_tool_definitions().is_empty());
        assert!(reg.status_list().is_empty());
        assert!(reg.connected_ids().is_empty());
    }
}
