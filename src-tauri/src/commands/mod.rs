// Paw Command Modules — Systems Layer
//
// Each sub-module is a thin Tauri command wrapper.
// Heavy logic lives in engine/ organisms; these modules
// only deserialise, delegate, and serialise.

pub mod action_log;
pub mod agent;
pub mod audit;
pub mod automations;
pub mod browser;
pub mod canvas;
pub mod channels;
pub mod chat;
pub mod config;
pub mod dashboard_tabs;
pub mod dashboards;
pub mod export;
pub mod flows;
pub mod guardrails;
pub mod health_monitor;
pub mod integrations;
pub mod mail;
pub mod mcp;
pub mod memory;
pub mod n8n;
pub mod oauth;
pub mod ollama;
pub mod project;
pub mod queries;
pub mod skill_wizard;
pub mod skills;
pub mod squad;
pub mod state;
pub mod tailscale;
pub mod task;
pub mod telemetry;
pub mod tool_bridge;
pub mod trade;
pub mod tts;
pub mod utility;
pub mod webhook;
