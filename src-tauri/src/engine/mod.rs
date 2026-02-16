// Paw Agent Engine — Native Rust AI agent runtime
// Replaces the OpenClaw WebSocket gateway with direct AI API calls,
// in-process tool execution, and Tauri IPC for zero-network-hop communication.

pub mod types;
pub mod providers;
pub mod agent_loop;
pub mod tool_executor;
pub mod sessions;
pub mod commands;
pub mod memory;
pub mod skills;
