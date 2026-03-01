// Paw Agent Engine — Multi-Agent Orchestrator
//
// Boss agent decomposes goals into sub-tasks, delegates to specialized sub-agents,
// monitors progress, and synthesizes results. All HIL security policies apply.
//
// Module layout:
//   tools.rs      — boss_tools() / worker_tools() definitions
//   handlers.rs   — execute_boss_tool / execute_worker_tool + handler fns
//   agent_loop.rs — unified streaming loop (boss & worker, parameterized)
//   sub_agent.rs  — run_sub_agent() setup + resolve_provider_for_model()

mod agent_loop;
mod handlers;
pub(crate) mod sub_agent;
pub mod tools;

use crate::engine::providers::AnyProvider;
use crate::engine::skills;
use crate::engine::state::EngineState;
use crate::engine::types::*;
use log::{info, warn};
use tauri::{Emitter, Manager};

use crate::atoms::error::EngineResult;
use agent_loop::{run_orchestrator_loop, AgentRole};
use sub_agent::resolve_provider_for_model;
use tools::boss_tools;

// ── Public API ─────────────────────────────────────────────────────────

/// Run the full orchestrator flow for a project.
/// The boss agent gets a special system prompt + delegation tools,
/// and orchestrates sub-agents to achieve the project goal.
pub async fn run_project(app_handle: &tauri::AppHandle, project_id: &str) -> EngineResult<String> {
    let state = app_handle.state::<EngineState>();
    let run_id = uuid::Uuid::new_v4().to_string();

    // Load project
    let projects = state.store.list_projects()?;
    let project = projects
        .into_iter()
        .find(|p| p.id == project_id)
        .ok_or_else(|| format!("Project not found: {}", project_id))?;

    if project.agents.is_empty() {
        return Err("Project has no agents assigned. Add at least a boss agent.".into());
    }

    info!(
        "[orchestrator] Starting project '{}' with {} agents, boss='{}'",
        project.title,
        project.agents.len(),
        project.boss_agent
    );

    // Update project status to running
    {
        let mut p = project.clone();
        p.status = "running".into();
        state.store.update_project(&p)?;
    }

    // Emit project started
    app_handle
        .emit(
            "project-event",
            serde_json::json!({
                "kind": "project_started",
                "project_id": project_id,
            }),
        )
        .ok();

    // Record initial message
    let init_msg = ProjectMessage {
        id: uuid::Uuid::new_v4().to_string(),
        project_id: project_id.to_string(),
        from_agent: "system".into(),
        to_agent: None,
        kind: "message".into(),
        content: format!(
            "Project '{}' started. Goal: {}",
            project.title, project.goal
        ),
        metadata: None,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    state.store.add_project_message(&init_msg)?;

    // Get provider config — use model routing for boss agent
    let (provider_config, model) = {
        let cfg = state.config.lock();
        let default_model = cfg
            .default_model
            .clone()
            .unwrap_or_else(|| "gpt-4o".to_string());

        let boss_entry = project.agents.iter().find(|a| a.role == "boss");
        let boss_specialty = boss_entry
            .map(|a| a.specialty.as_str())
            .unwrap_or("general");

        let model = if let Some(agent_model) = boss_entry
            .and_then(|a| a.model.as_deref())
            .filter(|m| !m.is_empty())
        {
            agent_model.to_string()
        } else {
            cfg.model_routing
                .resolve(&project.boss_agent, "boss", boss_specialty, &default_model)
        };

        info!(
            "[orchestrator] Boss agent '{}' using model '{}'",
            project.boss_agent, model
        );

        let provider = resolve_provider_for_model(&cfg, &model);
        match provider {
            Some(p) => (p, model),
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

    // Build agent roster description
    let agent_roster: Vec<String> = project
        .agents
        .iter()
        .filter(|a| a.role != "boss")
        .map(|a| {
            format!(
                "- **{}** (specialty: {}): {}",
                a.agent_id, a.specialty, a.status
            )
        })
        .collect();

    // Boss agent system prompt
    let boss_soul = state
        .store
        .compose_agent_context(&project.boss_agent)
        .unwrap_or(None);
    let skill_instructions =
        skills::get_enabled_skill_instructions(&state.store, &project.boss_agent)
            .unwrap_or_default();

    let mut sys_parts: Vec<String> = Vec::new();
    if let Some(sp) = &base_system_prompt {
        sys_parts.push(sp.clone());
    }
    if let Some(soul) = boss_soul {
        sys_parts.push(soul);
    }
    if !skill_instructions.is_empty() {
        sys_parts.push(skill_instructions.clone());
    }

    // §17 Pre-recall: inject relevant memories into orchestrator context
    {
        let emb_client = state.embedding_client();
        match crate::engine::engram::bridge::search(
            &state.store,
            &project.goal,
            10,
            0.2,
            emb_client.as_ref(),
            Some(&project.boss_agent),
        )
        .await
        {
            Ok(memories) if !memories.is_empty() => {
                let mut memory_block = String::from(
                    "## Relevant Memories\nPrior knowledge that may help with this project:\n",
                );
                for m in &memories {
                    memory_block.push_str(&format!("- [{}] {}\n", m.category, m.content));
                }
                sys_parts.push(memory_block);
                info!(
                    "[orchestrator] Pre-recalled {} memories for project '{}'",
                    memories.len(),
                    project.title
                );
            }
            Ok(_) => {}
            Err(e) => warn!("[orchestrator] Memory pre-recall failed: {}", e),
        }
    }

    sys_parts.push(format!(
        r#"## Orchestrator Mode

You are the **Boss Agent** orchestrating project "{}".

### Project Goal
{}

### Your Team
{}

### How to Work
1. Analyze the project goal and break it into concrete sub-tasks.
2. Use `delegate_task` to assign sub-tasks to your team members based on their specialty.
3. Use `check_agent_status` to monitor progress.
4. Use `send_agent_message` to provide guidance or corrections.
5. When all sub-tasks are complete, use `project_complete` to finalize.

### Rules
- Delegate work — don't try to do everything yourself.
- Be specific when delegating — give clear instructions.
- Monitor progress and adjust if agents get stuck.
- You can also use standard tools (exec, read_file, write_file, web_search, etc.) for coordination tasks.
- Always call `project_complete` when done."#,
        project.title,
        project.goal,
        if agent_roster.is_empty() { "No sub-agents assigned. You'll work solo.".into() } else { agent_roster.join("\n") }
    ));

    let boss_system_prompt = sys_parts.join("\n\n---\n\n");

    // Build tools: builtins + skill tools + orchestrator boss tools
    let mut all_tools = ToolDefinition::builtins();
    let enabled_ids: Vec<String> = skills::builtin_skills()
        .iter()
        .filter(|s| state.store.is_skill_enabled(&s.id).unwrap_or(false))
        .map(|s| s.id.clone())
        .collect();
    if !enabled_ids.is_empty() {
        all_tools.extend(ToolDefinition::skill_tools(&enabled_ids));
    }
    all_tools.extend(boss_tools());
    // Add tools from connected MCP servers
    all_tools.extend(ToolDefinition::mcp_tools(app_handle));

    // Create boss session
    let session_id = format!("eng-project-{}-boss", project_id);
    if state
        .store
        .get_session(&session_id)
        .ok()
        .flatten()
        .is_none()
    {
        state.store.create_session(
            &session_id,
            &model,
            None,
            Some(&format!("project-boss-{}", project_id)),
        )?;
    }

    // User message = project goal
    let user_msg = StoredMessage {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.clone(),
        role: "user".into(),
        content: format!(
            "Execute this project:\n\nTitle: {}\nGoal: {}",
            project.title, project.goal
        ),
        tool_calls_json: None,
        tool_call_id: None,
        name: None,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    state.store.add_message(&user_msg)?;

    let mut messages =
        state
            .store
            .load_conversation(&session_id, Some(&boss_system_prompt), None, None)?;
    let provider = AnyProvider::from_config(&provider_config);
    let pending = state.pending_approvals.clone();
    let pid = project_id.to_string();

    // Run the boss agent loop
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
        &project.boss_agent,
        AgentRole::Boss,
    )
    .await;

    // Save final response
    match &result {
        Ok(text) => {
            let msg_id = uuid::Uuid::new_v4().to_string();
            let stored = StoredMessage {
                id: msg_id,
                session_id: session_id.clone(),
                role: "assistant".into(),
                content: text.clone(),
                tool_calls_json: None,
                tool_call_id: None,
                name: None,
                created_at: chrono::Utc::now().to_rfc3339(),
            };
            state.store.add_message(&stored).ok();

            // §17 Post-capture: store project outcome in Engram memory
            if !text.is_empty() {
                let summary = if text.len() > 4000 {
                    &text[..4000]
                } else {
                    text.as_str()
                };
                let content = format!(
                    "Project '{}' completed. Goal: {}. Outcome: {}",
                    project.title, project.goal, summary
                );
                let emb_client = state.embedding_client();
                match crate::engine::engram::bridge::store_auto_capture(
                    &state.store,
                    &content,
                    "task_result",
                    emb_client.as_ref(),
                    Some(&project.boss_agent),
                    Some(&session_id),
                    None,
                    None,
                )
                .await
                {
                    Ok(Some(id)) => info!(
                        "[orchestrator] Project outcome stored in Engram (id={})",
                        &id[..id.len().min(8)]
                    ),
                    Ok(None) => {}
                    Err(e) => warn!("[orchestrator] Failed to store project outcome: {}", e),
                }
            }
        }
        Err(err) => {
            let mut p = project.clone();
            p.status = "failed".into();
            state.store.update_project(&p).ok();

            let msg = ProjectMessage {
                id: uuid::Uuid::new_v4().to_string(),
                project_id: pid.clone(),
                from_agent: "system".into(),
                to_agent: None,
                kind: "error".into(),
                content: format!("Project failed: {}", err),
                metadata: None,
                created_at: chrono::Utc::now().to_rfc3339(),
            };
            state.store.add_project_message(&msg).ok();
        }
    }

    app_handle
        .emit(
            "project-event",
            serde_json::json!({
                "kind": "project_finished",
                "project_id": project_id,
                "success": result.is_ok(),
            }),
        )
        .ok();

    result
}
