// Pawz Agent Engine — Native Rust AI agent runtime
// Direct AI API calls, in-process tool execution, and Tauri IPC
// for zero-network-hop communication.

pub mod agent_loop;
pub mod http;
pub mod paths;
pub mod pricing;
pub mod providers;
pub mod sessions;
pub mod state;
pub mod tools;
pub mod types;
// commands module moved to crate::commands::channels — see src/commands/channels.rs
pub mod channels;
pub mod chat;
pub mod compaction;
pub mod dex;
pub mod discord;
pub mod engram;
pub mod events;
pub mod injection;
pub mod irc;
pub mod matrix;
pub mod mattermost;
pub mod mcp;
pub mod memory;
pub mod n8n_engine;
pub mod nextcloud;
pub mod nostr;
pub mod orchestrator;
pub mod routing;
pub mod sandbox;
pub mod skills;
pub mod slack;
pub mod sol_dex;
pub mod swarm;
pub mod tasks;
pub mod telegram;
pub mod tool_index;
pub mod twitch;
pub mod web;
pub mod webchat;
pub mod webhook;
pub mod whatsapp;
