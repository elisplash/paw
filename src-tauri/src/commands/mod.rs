// Paw Command Modules — Systems Layer
//
// Each sub-module is a thin Tauri command wrapper.
// Heavy logic lives in engine/ organisms; these modules
// only deserialise, delegate, and serialise.

pub mod chat;
pub mod agent;
pub mod memory;
pub mod skills;
pub mod project;
