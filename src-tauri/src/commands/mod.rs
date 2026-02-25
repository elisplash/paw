// Paw Command Modules — Systems Layer
//
// Each sub-module is a thin Tauri command wrapper.
// Heavy logic lives in engine/ organisms; these modules
// only deserialise, delegate, and serialise.

pub mod action_log;
pub mod agent;
pub mod automations;
pub mod browser;
pub mod channels;
pub mod chat;
pub mod config;
pub mod mail;
pub mod mcp;
pub mod memory;
pub mod n8n;
pub mod project;
pub mod guardrails;
pub mod queries;
pub mod skill_wizard;
pub mod skills;
pub mod squad;
pub mod state;
pub mod tailscale;
pub mod task;
pub mod tool_bridge;
pub mod trade;
pub mod tts;
pub mod utility;
pub mod webhook;
