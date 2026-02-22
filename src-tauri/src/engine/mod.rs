// Pawz Agent Engine — Native Rust AI agent runtime
// Direct AI API calls, in-process tool execution, and Tauri IPC
// for zero-network-hop communication.

pub mod types;
pub mod http;
pub mod state;
pub mod tools;
pub mod pricing;
pub mod providers;
pub mod agent_loop;
pub mod sessions;
// commands module moved to crate::commands::channels — see src/commands/channels.rs
pub mod chat;
pub mod memory;
pub mod skills;
pub mod web;
pub mod dex;
pub mod sol_dex;
pub mod injection;
pub mod compaction;
pub mod routing;
pub mod sandbox;
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
pub mod whatsapp;
pub mod webhook;
pub mod mcp;
pub mod events;
pub mod swarm;
