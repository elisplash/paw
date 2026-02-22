// Paw Agent Engine — Tool Registry & Dispatcher
// Each tool group is a self-contained module with definitions + executor.
// This replaces both the old tools.rs builtins()/skill_tools()
// AND the old tool_executor.rs execute_tool() match.

#![allow(clippy::too_many_lines)]

use crate::atoms::types::*;
use crate::engine::state::EngineState;
use crate::engine::skills;
use log::info;
use tauri::Manager;
use crate::atoms::error::EngineResult;

pub mod exec;
pub mod fetch;
pub mod filesystem;
pub mod soul;
pub mod memory;
pub mod web;
pub mod email;
pub mod telegram;
pub mod slack;
pub mod github;
pub mod integrations;
pub mod tasks;
pub mod agents;
pub mod skills_tools;
pub mod coinbase;
pub mod dex;
pub mod solana;
pub mod skill_output;
pub mod skill_storage;

// ── ToolDefinition helpers (keep backward-compatible API for all callers) ───

impl ToolDefinition {
    /// Return the default set of built-in tools.
    pub fn builtins() -> Vec<Self> {
        let mut tools = Vec::new();
        tools.extend(exec::definitions());
        tools.extend(fetch::definitions());
        tools.extend(filesystem::definitions());
        tools.extend(soul::definitions());
        tools.extend(memory::definitions());
        tools.extend(web::definitions());
        tools.extend(tasks::definitions());
        tools.extend(agents::definitions());
        tools.extend(skills_tools::definitions());
        tools.extend(skill_output::definitions());
        tools.extend(skill_storage::definitions());
        tools
    }

    /// Return tools exposed by all connected MCP servers.
    /// Call this after builtins + skill_tools to merge dynamic tools.
    pub fn mcp_tools(app_handle: &tauri::AppHandle) -> Vec<Self> {
        if let Some(state) = app_handle.try_state::<EngineState>() {
            // Use try_lock to avoid blocking — if locked, return empty
            // (tools will be available on next request)
            match state.mcp_registry.try_lock() {
                Ok(reg) => reg.all_tool_definitions(),
                Err(_) => vec![],
            }
        } else {
            vec![]
        }
    }

    /// Return tools for enabled skills.
    pub fn skill_tools(enabled_skill_ids: &[String]) -> Vec<Self> {
        let mut tools = Vec::new();
        for id in enabled_skill_ids {
            match id.as_str() {
                "email"      => tools.extend(email::definitions()),
                "slack"      => tools.extend(slack::definitions()),
                "telegram"   => tools.extend(telegram::definitions()),
                "github"     => tools.extend(github::definitions()),
                "rest_api"   => tools.extend(integrations::definitions_for("rest_api")),
                "webhook"    => tools.extend(integrations::definitions_for("webhook")),
                "image_gen"  => tools.extend(integrations::definitions_for("image_gen")),
                "coinbase"   => tools.extend(coinbase::definitions()),
                "dex"        => tools.extend(dex::definitions()),
                "solana_dex" => tools.extend(solana::definitions()),
                _ => {}
            }
        }
        tools
    }
}

// ── Main executor ──────────────────────────────────────────────────────────

/// Execute a single tool call and return the result.
pub async fn execute_tool(tool_call: &crate::engine::types::ToolCall, app_handle: &tauri::AppHandle, agent_id: &str) -> ToolResult {
    let name = &tool_call.function.name;
    let args_str = &tool_call.function.arguments;

    info!("[engine] Executing tool: {} agent={} args={}", name, agent_id, &args_str[..args_str.len().min(200)]);

    let args: serde_json::Value = serde_json::from_str(args_str).unwrap_or(serde_json::json!({}));

    // Try each module in order — first Some(result) wins.
    let result = None
        .or(exec::execute(name, &args, app_handle, agent_id).await)
        .or(fetch::execute(name, &args, app_handle).await)
        .or(filesystem::execute(name, &args, agent_id).await)
        .or(soul::execute(name, &args, app_handle, agent_id).await)
        .or(memory::execute(name, &args, app_handle).await)
        .or(web::execute(name, &args, app_handle).await)
        .or(tasks::execute(name, &args, app_handle, agent_id).await)
        .or(agents::execute(name, &args, app_handle, agent_id).await)
        .or(skills_tools::execute(name, &args, app_handle, agent_id).await)
        .or(skill_output::execute(name, &args, app_handle, agent_id).await)
        .or(skill_storage::execute(name, &args, app_handle, agent_id).await)
        .or(email::execute(name, &args, app_handle).await)
        .or(telegram::execute(name, &args, app_handle).await)
        .or(slack::execute(name, &args, app_handle).await)
        .or(github::execute(name, &args, app_handle).await)
        .or(integrations::execute(name, &args, app_handle).await)
        .or(coinbase::execute(name, &args, app_handle).await)
        .or(dex::execute(name, &args, app_handle).await)
        .or(solana::execute(name, &args, app_handle).await);

    // Try MCP tools (prefixed with `mcp_`) if no built-in handled it
    // NOTE: holds the tokio::sync::Mutex for the duration of the tool call.
    // This is safe (tokio mutex is await-safe) but limits concurrency.
    // TODO(perf): extract client Arc and drop lock before awaiting call_tool.
    let result = match result {
        Some(r) => r,
        None if name.starts_with("mcp_") => {
            if let Some(state) = app_handle.try_state::<EngineState>() {
                let reg = state.mcp_registry.lock().await;
                match reg.execute_tool(name, &args).await {
                    Some(r) => r,
                    None => Err(format!("Unknown tool: {}", name)),
                }
            } else {
                Err(format!("Unknown tool: {}", name))
            }
        }
        None => Err(format!("Unknown tool: {}", name)),
    };

    match result {
        Ok(output) => ToolResult {
            tool_call_id: tool_call.id.clone(),
            output,
            success: true,
        },
        Err(err) => ToolResult {
            tool_call_id: tool_call.id.clone(),
            output: format!("Error: {}", err),
            success: false,
        },
    }
}

// ── Workspace helpers ──────────────────────────────────────────────────────

/// Get the per-agent workspace directory path.
/// Each agent gets its own isolated workspace at ~/.paw/workspaces/{agent_id}/
pub fn agent_workspace(agent_id: &str) -> std::path::PathBuf {
    let base = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    base.join(".paw").join("workspaces").join(agent_id)
}

/// Ensure the agent's workspace directory exists.
pub fn ensure_workspace(agent_id: &str) -> EngineResult<std::path::PathBuf> {
    let ws = agent_workspace(agent_id);
    std::fs::create_dir_all(&ws)
        .map_err(|e| format!("Failed to create workspace for agent '{}': {}", agent_id, e))?;
    Ok(ws)
}

// ── Shared credential helper (used by skill modules) ──────────────────────

/// Check that a skill is enabled and return its decrypted credentials.
pub fn get_skill_creds(
    skill_id: &str,
    app_handle: &tauri::AppHandle,
) -> EngineResult<std::collections::HashMap<String, String>> {
    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    if !state.store.is_skill_enabled(skill_id)? {
        return Err(format!("Skill '{}' is not enabled. Ask the user to enable it in Settings → Skills.", skill_id).into());
    }

    let creds = skills::get_skill_credentials(&state.store, skill_id)?;

    let defs = skills::builtin_skills();
    if let Some(def) = defs.iter().find(|d| d.id == skill_id) {
        let missing: Vec<&str> = def.required_credentials.iter()
            .filter(|c| c.required && !creds.contains_key(&c.key))
            .map(|c| c.key.as_str())
            .collect();
        if !missing.is_empty() {
            return Err(format!(
                "Skill '{}' is missing required credentials: {}. Ask the user to configure them in Settings → Skills.",
                skill_id, missing.join(", ")
            ).into());
        }
    }

    Ok(creds)
}
