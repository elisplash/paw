// Paw Agent Engine — Orchestrator Sub-Agent Runner
//
// Sets up and runs a worker sub-agent within a project.
// Builds its system prompt, tool set, capability filter, and session,
// then delegates to the unified `run_orchestrator_loop`.

use crate::engine::providers::AnyProvider;
use crate::engine::state::EngineState;
use crate::engine::skills;
use crate::engine::types::*;
use log::info;
use tauri::{Emitter, Manager};

use super::agent_loop::{run_orchestrator_loop, AgentRole};
use super::handlers::get_store;
use super::tools::worker_tools;
use crate::atoms::error::EngineResult;

/// Resolve a provider config for a given model string.
/// Uses smart prefix matching (gemini → Google, claude → Anthropic, etc.)
/// Falls back to default provider, then first provider.
pub(crate) fn resolve_provider_for_model(cfg: &EngineConfig, model: &str) -> Option<ProviderConfig> {
    let model = crate::engine::state::normalize_model_name(model);
    let provider = if model.starts_with("claude") || model.starts_with("anthropic") {
        cfg.providers.iter().find(|p| p.kind == ProviderKind::Anthropic).cloned()
    } else if model.starts_with("gemini") || model.starts_with("google") {
        cfg.providers.iter().find(|p| p.kind == ProviderKind::Google).cloned()
    } else if model.starts_with("gpt") || model.starts_with("o1") || model.starts_with("o3") || model.starts_with("o4") {
        cfg.providers.iter().find(|p| p.kind == ProviderKind::OpenAI).cloned()
    } else if model.contains('/') {
        // OpenRouter-style model IDs (e.g., meta-llama/llama-3.1-405b)
        cfg.providers.iter().find(|p| p.kind == ProviderKind::OpenRouter).cloned()
    } else if model.contains(':') {
        // Ollama-style model IDs (e.g., llama3.1:8b)
        cfg.providers.iter().find(|p| p.kind == ProviderKind::Ollama).cloned()
    } else {
        None
    };

    provider
        .or_else(|| {
            cfg.default_provider.as_ref()
                .and_then(|dp| cfg.providers.iter().find(|p| p.id == *dp).cloned())
        })
        .or_else(|| cfg.providers.first().cloned())
}

/// Run a sub-agent on a delegated task within a project.
pub(crate) async fn run_sub_agent(
    app_handle: &tauri::AppHandle,
    project_id: &str,
    agent_id: &str,
    task_description: &str,
    context: &str,
) -> EngineResult<String> {
    let state = app_handle.state::<EngineState>();

    // Get provider — use model routing for worker agents
    let (provider_config, model, agent_capabilities) = {
        let cfg = state.config.lock();
        let default_model = cfg.default_model.clone().unwrap_or_else(|| "gpt-4o".to_string());

        // Look up this agent in the project to get specialty and per-agent model override
        let agent_entry = state.store.get_project_agents(project_id).ok()
            .and_then(|agents| agents.into_iter().find(|a| a.agent_id == agent_id));
        let specialty = agent_entry.as_ref().map(|a| a.specialty.as_str()).unwrap_or("general");
        let capabilities = agent_entry.as_ref().map(|a| a.capabilities.clone()).unwrap_or_default();

        // Resolve model: per-agent field > model_routing > default
        let model = if let Some(agent_model) = agent_entry.as_ref().and_then(|a| a.model.as_deref()).filter(|m| !m.is_empty()) {
            agent_model.to_string()
        } else {
            cfg.model_routing.resolve(agent_id, "worker", specialty, &default_model)
        };

        info!("[orchestrator] Worker agent '{}' (specialty={}) using model '{}'", agent_id, specialty, model);

        let provider = resolve_provider_for_model(&cfg, &model);
        match provider {
            Some(p) => (p, model, capabilities),
            None => return Err("No AI provider configured".into()),
        }
    };

    let (base_system_prompt, max_rounds, tool_timeout) = {
        let cfg = state.config.lock();
        (
            cfg.default_system_prompt.clone(),
            cfg.max_tool_rounds,
            cfg.tool_timeout_secs,
        )
    };

    // Build system prompt for sub-agent
    let agent_soul = state.store.compose_agent_context(agent_id).unwrap_or(None);
    let skill_instructions = skills::get_enabled_skill_instructions(&state.store, agent_id).unwrap_or_default();

    let mut sys_parts: Vec<String> = Vec::new();
    if let Some(sp) = &base_system_prompt { sys_parts.push(sp.clone()); }
    if let Some(soul) = agent_soul { sys_parts.push(soul); }
    if !skill_instructions.is_empty() { sys_parts.push(skill_instructions); }

    sys_parts.push(format!(
        r#"## Sub-Agent Mode

You are agent '{}', working as part of a multi-agent project team.
Your boss agent has delegated this task to you.

### Your Task
{}
{}

### Instructions
- Focus on completing your assigned task thoroughly.
- Use `report_progress` to update the boss on your progress.
- Call `report_progress` with status "done" when finished.
- If you get stuck, report with status "blocked" and explain why.
- You have access to standard tools (exec, read_file, write_file, web_search, etc.)."#,
        agent_id,
        task_description,
        if context.is_empty() { String::new() } else { format!("\n### Additional Context\n{}", context) }
    ));

    let full_system_prompt = sys_parts.join("\n\n---\n\n");

    // Build tools: builtins + skills + worker tools
    let mut all_tools = ToolDefinition::builtins();
    let enabled_ids: Vec<String> = skills::builtin_skills().iter()
        .filter(|s| state.store.is_skill_enabled(&s.id).unwrap_or(false))
        .map(|s| s.id.clone())
        .collect();
    if !enabled_ids.is_empty() {
        all_tools.extend(ToolDefinition::skill_tools(&enabled_ids));
    }
    all_tools.extend(worker_tools());
    // Add tools from connected MCP servers
    all_tools.extend(ToolDefinition::mcp_tools(app_handle));

    // Apply per-agent tool capabilities filter
    if !agent_capabilities.is_empty() {
        let before = all_tools.len();
        all_tools.retain(|tool| agent_capabilities.contains(&tool.function.name));
        // Always keep worker control tools regardless of policy
        for wt in worker_tools() {
            if !all_tools.iter().any(|t| t.function.name == wt.function.name) {
                all_tools.push(wt);
            }
        }
        info!("[orchestrator] Capabilities filter for '{}': {} → {} tools (capabilities: {:?})",
            agent_id, before, all_tools.len(), agent_capabilities);
    }

    // Create per-agent session
    let session_id = format!("eng-project-{}-{}", project_id, agent_id);
    let run_id = uuid::Uuid::new_v4().to_string();

    if state.store.get_session(&session_id).ok().flatten().is_none() {
        state.store.create_session(&session_id, &model, None, Some(agent_id))?;
    }

    // Add the task as user message
    let user_msg = StoredMessage {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.clone(),
        role: "user".into(),
        content: format!("Your assigned task: {}\n\n{}", task_description,
            if context.is_empty() { "" } else { context }),
        tool_calls_json: None,
        tool_call_id: None,
        name: None,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    state.store.add_message(&user_msg)?;

    let mut messages = state.store.load_conversation(&session_id, Some(&full_system_prompt))?;
    let provider = AnyProvider::from_config(&provider_config);
    let pending = state.pending_approvals.clone();
    let pid = project_id.to_string();
    let aid = agent_id.to_string();

    // Run the unified agent loop as a Worker
    let result = run_orchestrator_loop(
        app_handle,
        &provider,
        &model,
        &mut messages,
        &all_tools,
        &session_id,
        &run_id,
        max_rounds,
        &pending,
        tool_timeout,
        &pid,
        &aid,
        AgentRole::Worker { agent_id: &aid },
    ).await;

    // Record result
    let store = get_store(app_handle);
    match &result {
        Ok(text) => {
            if let Some(ref store) = store {
                store.update_project_agent_status(project_id, agent_id, "done", None).ok();
                let msg = ProjectMessage {
                    id: uuid::Uuid::new_v4().to_string(),
                    project_id: project_id.to_string(),
                    from_agent: agent_id.to_string(),
                    to_agent: Some("boss".into()),
                    kind: "result".into(),
                    content: format!("Task completed: {}", &text[..text.len().min(500)]),
                    metadata: None,
                    created_at: chrono::Utc::now().to_rfc3339(),
                };
                store.add_project_message(&msg).ok();
            }
        }
        Err(err) => {
            if let Some(ref store) = store {
                let err_str = err.to_string();
                store.update_project_agent_status(project_id, agent_id, "error", Some(&err_str)).ok();
            }
        }
    }

    app_handle.emit("project-event", serde_json::json!({
        "kind": "agent_finished",
        "project_id": project_id,
        "agent_id": agent_id,
        "success": result.is_ok(),
    })).ok();

    result
}
