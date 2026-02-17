// Pawz Agent Engine — Native Rust AI agent runtime
// Direct AI API calls, in-process tool execution, and Tauri IPC
// for zero-network-hop communication.

pub mod types;
pub mod providers;
pub mod agent_loop;
pub mod tool_executor;
pub mod sessions;
pub mod commands;
pub mod memory;
pub mod skills;
pub mod web;
pub mod injection;
pub mod compaction;
pub mod telegram;
pub mod channels;
pub mod orchestrator;
pub mod discord;
pub mod irc;
pub mod slack;
pub mod matrix;
pub mod mattermost;
pub mod nextcloud;
pub mod nostr;
pub mod twitch;
pub mod webchat;
