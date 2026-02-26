// Paw Agent Engine — MCP (Model Context Protocol) Client
//
// Phase E: Dynamic tool discovery from external MCP servers.
// Lets agents use tools from npm MCP packages (e.g., @modelcontextprotocol/server-github)
// without any Rust code changes.
//
// Architecture:
//   types.rs     — MCP protocol types + config structs
//   transport.rs — stdio + SSE transports + unified McpTransportHandle
//   client.rs    — JSON-RPC initialize/tools-list/tools-call (transport-agnostic)
//   registry.rs  — multi-server lifecycle + tool dispatch + n8n auto-registration

pub mod client;
pub mod registry;
pub mod transport;
pub mod types;

// Re-export the main public types
pub use registry::McpRegistry;
pub use types::{McpServerConfig, McpServerStatus, McpTransport};
