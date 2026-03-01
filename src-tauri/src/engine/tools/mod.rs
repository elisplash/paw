// Paw Agent Engine — Tool Registry & Dispatcher
// Each tool group is a self-contained module with definitions + executor.
// This replaces both the old tools.rs builtins()/skill_tools()
// AND the old tool_executor.rs execute_tool() match.

#![allow(clippy::too_many_lines)]

use crate::atoms::error::EngineResult;
use crate::atoms::types::*;
use crate::engine::skills;
use crate::engine::state::EngineState;
use log::info;
use tauri::Manager;

pub mod agent_comms;
pub mod agents;
pub mod coinbase;
pub mod dex;
pub mod discord;
pub mod exec;
pub mod fetch;
pub mod filesystem;
pub mod integrations;
pub mod memory;
pub mod n8n;
pub mod request_tools;
pub mod skill_output;
pub mod skill_storage;
pub mod skills_tools;
pub mod solana;
pub mod soul;
pub mod squads;
pub mod tasks;
pub mod telegram;
pub mod web;
pub mod worker_delegate;

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
        tools.extend(agent_comms::definitions());
        tools.extend(squads::definitions());
        tools.extend(request_tools::definitions());
        tools.extend(n8n::definitions());
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
                "telegram" => tools.extend(telegram::definitions()),
                "rest_api" => tools.extend(integrations::definitions_for("rest_api")),
                "webhook" => tools.extend(integrations::definitions_for("webhook")),
                "image_gen" => tools.extend(integrations::definitions_for("image_gen")),
                "discord" => tools.extend(discord::definitions()),
                "coinbase" => tools.extend(coinbase::definitions()),
                "solana_dex" => tools.extend(solana::definitions()),
                "dex" => tools.extend(dex::definitions()),
                _ => {}
            }
        }
        tools
    }
}

// ── Main executor ──────────────────────────────────────────────────────────

/// Execute a single tool call and return the result.
pub async fn execute_tool(
    tool_call: &crate::engine::types::ToolCall,
    app_handle: &tauri::AppHandle,
    agent_id: &str,
) -> ToolResult {
    let name = &tool_call.function.name;
    let args_str = &tool_call.function.arguments;

    info!(
        "[engine] Executing tool: {} agent={} args={}",
        name,
        agent_id,
        &args_str[..args_str.len().min(200)]
    );

    let args: serde_json::Value = match serde_json::from_str(args_str) {
        Ok(v) => v,
        Err(parse_err) => {
            log::warn!(
                "[engine] Malformed tool args for '{}' — JSON parse failed: {}. Args: {}",
                name,
                parse_err,
                &args_str[..args_str.len().min(300)]
            );
            serde_json::json!({})
        }
    };

    // fetch & exec: When a worker model is configured, delegate these to the
    // worker (Foreman) so the main model doesn't spend API tokens on
    // data-fetching rounds. The worker is typically a cheaper model.
    if (name == "fetch" || name == "exec") && worker_delegate::has_worker(app_handle) {
        info!("[engine] Delegating {} to Foreman (worker model)", name);
        if let Some(worker_result) =
            worker_delegate::delegate_to_worker(tool_call, app_handle, agent_id).await
        {
            return worker_result;
        }
        // Worker delegation failed — fall through to direct execution
        info!(
            "[engine] Worker delegation failed, executing {} directly",
            name
        );
    }

    // Try each module in order — first Some(result) wins.
    let result = None
        .or(exec::execute(name, &args, app_handle, agent_id).await)
        .or(fetch::execute(name, &args, app_handle).await)
        .or(filesystem::execute(name, &args, agent_id).await)
        .or(soul::execute(name, &args, app_handle, agent_id).await)
        .or(memory::execute(name, &args, app_handle, agent_id).await)
        .or(web::execute(name, &args, app_handle).await)
        .or(tasks::execute(name, &args, app_handle, agent_id).await)
        .or(agents::execute(name, &args, app_handle, agent_id).await)
        .or(skills_tools::execute(name, &args, app_handle, agent_id).await)
        .or(skill_output::execute(name, &args, app_handle, agent_id).await)
        .or(skill_storage::execute(name, &args, app_handle, agent_id).await)
        .or(agent_comms::execute(name, &args, app_handle, agent_id).await)
        .or(squads::execute(name, &args, app_handle, agent_id).await)
        .or(request_tools::execute(name, &args, app_handle, agent_id).await)
        .or(telegram::execute(name, &args, app_handle).await)
        .or(integrations::execute(name, &args, app_handle).await)
        .or(n8n::execute(name, &args, app_handle).await)
        .or(coinbase::execute(name, &args, app_handle).await)
        .or(solana::execute(name, &args, app_handle).await)
        .or(dex::execute(name, &args, app_handle).await)
        .or(discord::execute(name, &args, app_handle).await);

    // Try MCP tools (prefixed with `mcp_`) if no built-in handled it.
    // When a worker_model is configured, delegate MCP calls to the local
    // Ollama worker instead of executing directly — zero API cost.
    let result = match result {
        Some(r) => r,
        None if name.starts_with("mcp_") => {
            // Try worker delegation first (local Ollama model)
            if let Some(worker_result) =
                worker_delegate::delegate_to_worker(tool_call, app_handle, agent_id).await
            {
                if worker_result.success {
                    Ok(worker_result.output)
                } else {
                    Err(worker_result.output)
                }
            } else {
                // No worker configured — fall back to direct MCP execution
                info!("[engine] No worker model configured, executing MCP tool directly");
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
/// Each agent gets its own isolated workspace under the Paw data root.
pub fn agent_workspace(agent_id: &str) -> std::path::PathBuf {
    crate::engine::paths::agent_workspace_dir(agent_id)
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
    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    // Look up default_enabled from builtin definition
    let defs = skills::builtin_skills();
    let default_enabled = defs
        .iter()
        .find(|d| d.id == skill_id)
        .map(|d| d.default_enabled)
        .unwrap_or(false);
    let enabled = state
        .store
        .get_skill_enabled_state(skill_id)?
        .unwrap_or(default_enabled);

    if !enabled {
        return Err(format!(
            "Skill '{}' is not enabled. Ask the user to enable it in Skills.",
            skill_id
        )
        .into());
    }

    let creds = skills::get_skill_credentials(&state.store, skill_id)?;

    if let Some(def) = defs.iter().find(|d| d.id == skill_id) {
        let missing: Vec<&str> = def
            .required_credentials
            .iter()
            .filter(|c| c.required && !creds.contains_key(&c.key))
            .map(|c| c.key.as_str())
            .collect();
        if !missing.is_empty() {
            return Err(format!(
                "Skill '{}' is missing required credentials: {}. Ask the user to configure them in Skills.",
                skill_id, missing.join(", ")
            ).into());
        }
    }

    Ok(creds)
}
