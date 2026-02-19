// Paw Command Modules — Systems Layer
//
// Each sub-module is a thin Tauri command wrapper.
// Heavy logic lives in engine/ organisms; these modules
// only deserialise, delegate, and serialise.

pub mod state;
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
