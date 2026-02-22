// Paw Agent Engine — MCP (Model Context Protocol) Client
//
// Phase E: Dynamic tool discovery from external MCP servers.
// Lets agents use tools from npm MCP packages (e.g., @modelcontextprotocol/server-github)
// without any Rust code changes.
//
// Architecture:
//   types.rs     — MCP protocol types + config structs
//   transport.rs — stdio process spawning + Content-Length framing
//   client.rs    — JSON-RPC initialize/tools-list/tools-call
//   registry.rs  — multi-server lifecycle + tool dispatch

pub mod types;
pub mod transport;
pub mod client;
pub mod registry;

// Re-export the main public types
pub use registry::McpRegistry;
pub use types::{McpServerConfig, McpServerStatus, McpTransport};
