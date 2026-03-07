// Pawz Agent Engine — Native Rust AI agent runtime
// Direct AI API calls, in-process tool execution, and Tauri IPC
// for zero-network-hop communication.

pub mod agent_loop;
pub mod audit;
pub mod binary_ipc;
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
pub mod constrained;
pub mod dex;
pub mod discord;
pub mod engram;
pub mod events;
pub mod forge;
pub mod injection;
pub mod irc;
pub mod key_vault;
pub mod matrix;
pub mod mattermost;
pub mod mcp;
pub mod memory;
pub mod n8n_engine;
pub mod nextcloud;
pub mod nostr;
pub mod oauth;
pub mod orchestrator;
pub mod plan;
pub mod routing;
pub mod sandbox;
pub mod skills;
pub mod slack;
pub mod sol_dex;
pub mod speculative;
pub mod swarm;
pub mod tasks;
pub mod telegram;
pub mod telemetry;
pub mod tool_index;
pub mod tool_registry;
pub mod twitch;
pub mod util;
pub mod web;
pub mod webchat;
pub mod webhook;
pub mod whatsapp;
