// Paw Command Modules — Systems Layer
//
// Each sub-module is a thin Tauri command wrapper.
// Heavy logic lives in engine/ organisms; these modules
// only deserialise, delegate, and serialise.

pub mod state;
pub mod channels;
pub mod chat;
pub mod agent;
pub mod memory;
pub mod skills;
pub mod project;
pub mod config;
pub mod trade;
pub mod task;
pub mod tts;
pub mod mail;
pub mod utility;
pub mod browser;
pub mod tailscale;
pub mod webhook;
pub mod mcp;
pub mod skill_wizard;
pub mod squad;
