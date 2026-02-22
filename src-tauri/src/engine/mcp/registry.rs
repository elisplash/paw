// Paw Agent Engine — MCP Server Registry
//
// Manages the lifecycle of all configured MCP servers:
// connect, disconnect, health-check, restart, and tool dispatch.

use super::client::McpClient;
use super::types::*;
use crate::atoms::types::{FunctionDefinition, ToolDefinition};
use log::info;
use std::collections::HashMap;

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
}

// ── Conversion helpers ─────────────────────────────────────────────────

/// Convert an MCP tool definition to a Paw ToolDefinition.
/// The tool name is prefixed with `mcp_{server_id}_` to namespace it.
fn mcp_tool_to_paw_def(server_id: &str, tool: &McpToolDef) -> ToolDefinition {
    let prefixed_name = format!("mcp_{}_{}", server_id, tool.name);
    let server_tag = format!(" [MCP: {}]", server_id);
    let description = tool
        .description
        .as_deref()
        .unwrap_or("(no description)")
        .to_string()
        + &server_tag;

    ToolDefinition {
        tool_type: "function".into(),
        function: FunctionDefinition {
            name: prefixed_name,
            description,
            parameters: tool.input_schema.clone(),
        },
    }
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
    ids.sort_by(|a, b| b.len().cmp(&a.len())); // longest first

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
    fn test_find_server_and_tool() {
        // We can't create real McpClient instances without actually spawning processes,
        // so we test the parsing logic directly.

        // Test the prefix parsing logic manually instead
        let stripped = "github_read_file";
        // If "github" is a known server ID, should return ("github", "read_file")

        // Direct test of the prefix stripping logic
        assert_eq!(
            stripped.strip_prefix("github").and_then(|r| r.strip_prefix('_')),
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
