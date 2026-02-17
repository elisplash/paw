// Paw Agent Engine — Multi-Agent Orchestrator
// Boss agent decomposes goals into sub-tasks, delegates to specialized sub-agents,
// monitors progress, and synthesizes results. All HIL security policies apply.

use crate::engine::types::*;
use crate::engine::providers::AnyProvider;
use crate::engine::commands::{EngineState, PendingApprovals};
use crate::engine::sessions::SessionStore;
use crate::engine::skills;
use log::{info, warn, error};
use tauri::{Emitter, Manager};

/// Orchestrator-specific tools that only the boss agent gets
pub fn boss_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "delegate_task".into(),
                description: "Delegate a sub-task to a specialized sub-agent on this project. The sub-agent will work on the task and report back.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "agent_id": {
                            "type": "string",
                            "description": "The agent_id of the sub-agent to delegate to (must be assigned to this project)"
                        },
                        "task_description": {
                            "type": "string",
                            "description": "Clear, specific description of what the sub-agent should do"
                        },
                        "context": {
                            "type": "string",
                            "description": "Additional context, requirements, or constraints for the sub-task"
                        }
                    },
                    "required": ["agent_id", "task_description"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "check_agent_status".into(),
                description: "Check the current status and progress of all sub-agents on this project.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "send_agent_message".into(),
                description: "Send a message to a specific sub-agent or broadcast to all agents on this project.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "to_agent": {
                            "type": "string",
                            "description": "The agent_id to send to, or 'all' for broadcast"
                        },
                        "message": {
                            "type": "string",
                            "description": "The message content"
                        }
                    },
                    "required": ["to_agent", "message"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "project_complete".into(),
                description: "Mark the project as completed with a final summary. Call this when all sub-tasks are done and the project goal has been achieved.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "summary": {
                            "type": "string",
                            "description": "Final summary of what was accomplished"
                        },
                        "status": {
                            "type": "string",
                            "enum": ["completed", "failed"],
                            "description": "Final project status"
                        }
                    },
                    "required": ["summary", "status"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "create_sub_agent".into(),
                description: "Create and register a new sub-agent in the current project. The agent will be added to the database and available for task delegation immediately.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "A unique name/id for the agent (e.g. 'code-cat', 'research-owl'). Use lowercase with hyphens."
                        },
                        "role": {
                            "type": "string",
                            "enum": ["worker", "boss"],
                            "description": "The agent's role. Usually 'worker' for sub-agents."
                        },
                        "specialty": {
                            "type": "string",
                            "enum": ["coder", "researcher", "designer", "communicator", "security", "general"],
                            "description": "The agent's area of expertise"
                        },
                        "system_prompt": {
                            "type": "string",
                            "description": "Custom system prompt / personality instructions for this agent"
                        },
                        "capabilities": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "List of tool names this agent should have access to (e.g. ['exec', 'fetch', 'web_search']). Leave empty for all default tools."
                        },
                        "model": {
                            "type": "string",
                            "description": "Optional model override for this agent (e.g. 'gemini-2.5-flash'). Leave empty to use project defaults."
                        }
                    },
                    "required": ["name", "role", "specialty", "system_prompt"]
                }),
            },
        },
    ]
}

/// Worker-specific tools that sub-agents get
pub fn worker_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "report_progress".into(),
                description: "Report your progress back to the boss agent. Call this when you have updates, results, or encounter issues.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "status": {
                            "type": "string",
                            "enum": ["working", "done", "error", "blocked"],
                            "description": "Current status of your work"
                        },
                        "message": {
                            "type": "string",
                            "description": "Description of progress, results, or issues"
                        },
                        "output": {
                            "type": "string",
                            "description": "Any output or deliverables from your work"
                        }
                    },
                    "required": ["status", "message"]
                }),
            },
        },
    ]
}

/// Execute an orchestrator-specific tool call for the boss agent.
/// Returns Ok(output) or Err(error_message).
pub async fn execute_boss_tool(
    tool_call: &ToolCall,
    app_handle: &tauri::AppHandle,
    project_id: &str,
) -> Option<Result<String, String>> {
    let name = &tool_call.function.name;
    let args: serde_json::Value = serde_json::from_str(&tool_call.function.arguments)
        .unwrap_or(serde_json::json!({}));

    match name.as_str() {
        "delegate_task" => {
            let agent_id = args["agent_id"].as_str().unwrap_or("").to_string();
            let task_desc = args["task_description"].as_str().unwrap_or("").to_string();
            let context = args["context"].as_str().unwrap_or("").to_string();

            if agent_id.is_empty() || task_desc.is_empty() {
                return Some(Err("delegate_task requires agent_id and task_description".into()));
            }

            // Record the delegation message
            let store = get_store(app_handle);
            if let Some(ref store) = store {
                let msg = ProjectMessage {
                    id: uuid::Uuid::new_v4().to_string(),
                    project_id: project_id.to_string(),
                    from_agent: "boss".into(),
                    to_agent: Some(agent_id.clone()),
                    kind: "delegation".into(),
                    content: task_desc.clone(),
                    metadata: if context.is_empty() { None } else { Some(serde_json::json!({"context": context}).to_string()) },
                    created_at: chrono::Utc::now().to_rfc3339(),
                };
                store.add_project_message(&msg).ok();

                // Update agent status
                store.update_project_agent_status(project_id, &agent_id, "working", Some(&task_desc)).ok();
            }

            // Emit event for UI update
            app_handle.emit("project-event", serde_json::json!({
                "kind": "delegation",
                "project_id": project_id,
                "agent_id": agent_id,
                "task": task_desc,
            })).ok();

            // Spawn the sub-agent asynchronously
            let app = app_handle.clone();
            let pid = project_id.to_string();
            let aid = agent_id.clone();
            let task = task_desc.clone();
            let ctx = context.clone();

            tauri::async_runtime::spawn(async move {
                if let Err(e) = run_sub_agent(&app, &pid, &aid, &task, &ctx).await {
                    error!("[orchestrator] Sub-agent {} failed: {}", aid, e);
                    if let Some(store) = get_store(&app) {
                        store.update_project_agent_status(&pid, &aid, "error", Some(&e)).ok();
                        let msg = ProjectMessage {
                            id: uuid::Uuid::new_v4().to_string(),
                            project_id: pid.clone(),
                            from_agent: aid.clone(),
                            to_agent: Some("boss".into()),
                            kind: "error".into(),
                            content: format!("Agent {} failed: {}", aid, e),
                            metadata: None,
                            created_at: chrono::Utc::now().to_rfc3339(),
                        };
                        store.add_project_message(&msg).ok();
                    }
                }
            });

            Some(Ok(format!("Task delegated to agent '{}'. They are now working on: {}", agent_id, task_desc)))
        }

        "check_agent_status" => {
            let store = get_store(app_handle);
            match store {
                Some(store) => {
                    match store.get_project_agents(project_id) {
                        Ok(agents) => {
                            let msgs = store.get_project_messages(project_id, 20).unwrap_or_default();
                            let mut status_lines: Vec<String> = Vec::new();

                            for a in &agents {
                                let recent: Vec<&ProjectMessage> = msgs.iter()
                                    .filter(|m| m.from_agent == a.agent_id)
                                    .collect();
                                let last_msg = recent.last()
                                    .map(|m| format!(" | Last: [{}] {}", m.kind, &m.content[..m.content.len().min(100)]))
                                    .unwrap_or_default();

                                status_lines.push(format!(
                                    "- {} ({}): status={}, task={}{}",
                                    a.agent_id, a.specialty, a.status,
                                    a.current_task.as_deref().unwrap_or("none"),
                                    last_msg
                                ));
                            }

                            Some(Ok(format!("Agent Status:\n{}", status_lines.join("\n"))))
                        }
                        Err(e) => Some(Err(e))
                    }
                }
                None => Some(Err("Could not access engine store".into()))
            }
        }

        "send_agent_message" => {
            let to = args["to_agent"].as_str().unwrap_or("").to_string();
            let message = args["message"].as_str().unwrap_or("").to_string();

            if to.is_empty() || message.is_empty() {
                return Some(Err("send_agent_message requires to_agent and message".into()));
            }

            let store = get_store(app_handle);
            if let Some(ref store) = store {
                let msg = ProjectMessage {
                    id: uuid::Uuid::new_v4().to_string(),
                    project_id: project_id.to_string(),
                    from_agent: "boss".into(),
                    to_agent: if to == "all" { None } else { Some(to.clone()) },
                    kind: "message".into(),
                    content: message.clone(),
                    metadata: None,
                    created_at: chrono::Utc::now().to_rfc3339(),
                };
                store.add_project_message(&msg).ok();
            }

            app_handle.emit("project-event", serde_json::json!({
                "kind": "message",
                "project_id": project_id,
                "from": "boss",
                "to": to,
                "content": message,
            })).ok();

            Some(Ok(format!("Message sent to {}", to)))
        }

        "project_complete" => {
            let summary = args["summary"].as_str().unwrap_or("").to_string();
            let status = args["status"].as_str().unwrap_or("completed").to_string();

            let store = get_store(app_handle);
            if let Some(ref store) = store {
                // Update project status
                if let Ok(projects) = store.list_projects() {
                    if let Some(mut proj) = projects.into_iter().find(|p| p.id == project_id) {
                        proj.status = status.clone();
                        store.update_project(&proj).ok();
                    }
                }

                let msg = ProjectMessage {
                    id: uuid::Uuid::new_v4().to_string(),
                    project_id: project_id.to_string(),
                    from_agent: "boss".into(),
                    to_agent: None,
                    kind: "result".into(),
                    content: summary.clone(),
                    metadata: Some(serde_json::json!({"final_status": status}).to_string()),
                    created_at: chrono::Utc::now().to_rfc3339(),
                };
                store.add_project_message(&msg).ok();
            }

            app_handle.emit("project-event", serde_json::json!({
                "kind": "project_complete",
                "project_id": project_id,
                "status": status,
                "summary": summary,
            })).ok();

            Some(Ok(format!("Project marked as {}. Summary: {}", status, summary)))
        }

        "create_sub_agent" => {
            let name = args["name"].as_str().unwrap_or("").to_string();
            let role = args["role"].as_str().unwrap_or("worker").to_string();
            let specialty = args["specialty"].as_str().unwrap_or("general").to_string();
            let system_prompt = args["system_prompt"].as_str().unwrap_or("").to_string();
            let capabilities: Vec<String> = args["capabilities"].as_array()
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            let model = args["model"].as_str().map(|s| s.to_string()).filter(|s| !s.is_empty());

            if name.is_empty() {
                return Some(Err("create_sub_agent requires a 'name' argument".into()));
            }
            if system_prompt.is_empty() {
                return Some(Err("create_sub_agent requires a 'system_prompt' argument".into()));
            }

            // Generate a slug-style agent_id from the name
            let agent_id = name.to_lowercase()
                .chars()
                .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' })
                .collect::<String>();

            let store = get_store(app_handle);
            match store {
                Some(store) => {
                    // Check if agent already exists in this project
                    if let Ok(existing) = store.get_project_agents(project_id) {
                        if existing.iter().any(|a| a.agent_id == agent_id) {
                            return Some(Err(format!("Agent '{}' already exists in this project", agent_id)));
                        }
                    }

                    let agent = ProjectAgent {
                        agent_id: agent_id.clone(),
                        role: role.clone(),
                        specialty: specialty.clone(),
                        status: "idle".into(),
                        current_task: None,
                        model: model.clone(),
                        system_prompt: Some(system_prompt.clone()),
                        capabilities: capabilities.clone(),
                    };

                    match store.add_project_agent(project_id, &agent) {
                        Ok(()) => {
                            // Also create an IDENTITY.md agent file so compose_agent_context picks it up
                            let identity_content = format!(
                                "# {}\n\n## Identity\nAgent ID: {}\nRole: {}\nSpecialty: {}\n\n## Personality & Instructions\n{}\n",
                                name, agent_id, role, specialty, system_prompt
                            );
                            store.set_agent_file(&agent_id, "IDENTITY.md", &identity_content).ok();

                            // Record creation message in project log
                            let msg = ProjectMessage {
                                id: uuid::Uuid::new_v4().to_string(),
                                project_id: project_id.to_string(),
                                from_agent: "boss".into(),
                                to_agent: Some(agent_id.clone()),
                                kind: "message".into(),
                                content: format!("Created new agent '{}' (role={}, specialty={})", agent_id, role, specialty),
                                metadata: Some(serde_json::json!({
                                    "action": "create_sub_agent",
                                    "capabilities": capabilities,
                                    "model": model,
                                }).to_string()),
                                created_at: chrono::Utc::now().to_rfc3339(),
                            };
                            store.add_project_message(&msg).ok();

                            // Emit event for UI
                            app_handle.emit("project-event", serde_json::json!({
                                "kind": "agent_created",
                                "project_id": project_id,
                                "agent_id": agent_id,
                                "role": role,
                                "specialty": specialty,
                            })).ok();

                            Some(Ok(format!(
                                "Successfully created sub-agent '{}' (role={}, specialty={}). You can now delegate tasks to this agent using delegate_task with agent_id='{}'.",
                                agent_id, role, specialty, agent_id
                            )))
                        }
                        Err(e) => Some(Err(format!("Failed to create agent: {}", e)))
                    }
                }
                None => Some(Err("Could not access engine store".into()))
            }
        }

        _ => None, // Not an orchestrator tool
    }
}

/// Execute a worker-specific tool (report_progress).
pub async fn execute_worker_tool(
    tool_call: &ToolCall,
    app_handle: &tauri::AppHandle,
    project_id: &str,
    agent_id: &str,
) -> Option<Result<String, String>> {
    let name = &tool_call.function.name;
    let args: serde_json::Value = serde_json::from_str(&tool_call.function.arguments)
        .unwrap_or(serde_json::json!({}));

    match name.as_str() {
        "report_progress" => {
            let status = args["status"].as_str().unwrap_or("working").to_string();
            let message = args["message"].as_str().unwrap_or("").to_string();
            let output = args["output"].as_str().unwrap_or("").to_string();

            let store = get_store(app_handle);
            if let Some(ref store) = store {
                // Update agent status
                let db_status = match status.as_str() {
                    "done" => "done",
                    "error" | "blocked" => "error",
                    _ => "working",
                };
                store.update_project_agent_status(project_id, agent_id, db_status, Some(&message)).ok();

                // Record progress message
                let mut content = message.clone();
                if !output.is_empty() {
                    content = format!("{}\n\nOutput:\n{}", content, output);
                }

                let msg = ProjectMessage {
                    id: uuid::Uuid::new_v4().to_string(),
                    project_id: project_id.to_string(),
                    from_agent: agent_id.to_string(),
                    to_agent: Some("boss".into()),
                    kind: "progress".into(),
                    content,
                    metadata: Some(serde_json::json!({"status": status}).to_string()),
                    created_at: chrono::Utc::now().to_rfc3339(),
                };
                store.add_project_message(&msg).ok();
            }

            app_handle.emit("project-event", serde_json::json!({
                "kind": "progress",
                "project_id": project_id,
                "agent_id": agent_id,
                "status": status,
                "message": message,
            })).ok();

            Some(Ok("Progress reported to boss agent.".into()))
        }
        _ => None,
    }
}

/// Run the full orchestrator flow for a project.
/// The boss agent gets a special system prompt + delegation tools,
/// and orchestrates sub-agents to achieve the project goal.
pub async fn run_project(
    app_handle: &tauri::AppHandle,
    project_id: &str,
) -> Result<String, String> {
    let state = app_handle.state::<EngineState>();
    let run_id = uuid::Uuid::new_v4().to_string();

    // Load project
    let projects = state.store.list_projects()?;
    let project = projects.into_iter().find(|p| p.id == project_id)
        .ok_or_else(|| format!("Project not found: {}", project_id))?;

    if project.agents.is_empty() {
        return Err("Project has no agents assigned. Add at least a boss agent.".into());
    }

    info!("[orchestrator] Starting project '{}' with {} agents, boss='{}'",
        project.title, project.agents.len(), project.boss_agent);

    // Update project status to running
    {
        let mut p = project.clone();
        p.status = "running".into();
        state.store.update_project(&p)?;
    }

    // Emit project started
    app_handle.emit("project-event", serde_json::json!({
        "kind": "project_started",
        "project_id": project_id,
    })).ok();

    // Record initial message
    let init_msg = ProjectMessage {
        id: uuid::Uuid::new_v4().to_string(),
        project_id: project_id.to_string(),
        from_agent: "system".into(),
        to_agent: None,
        kind: "message".into(),
        content: format!("Project '{}' started. Goal: {}", project.title, project.goal),
        metadata: None,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    state.store.add_project_message(&init_msg)?;

    // Get provider config — use model routing for boss agent
    let (provider_config, model) = {
        let cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;
        let default_model = cfg.default_model.clone().unwrap_or_else(|| "gpt-4o".to_string());

        // Find the boss agent's ProjectAgent entry to get specialty
        let boss_entry = project.agents.iter().find(|a| a.role == "boss");
        let boss_specialty = boss_entry.map(|a| a.specialty.as_str()).unwrap_or("general");

        // Resolve model via routing: per-agent override > boss_model > default
        let model = if let Some(agent_model) = boss_entry.and_then(|a| a.model.as_deref()).filter(|m| !m.is_empty()) {
            agent_model.to_string()
        } else {
            cfg.model_routing.resolve(&project.boss_agent, "boss", boss_specialty, &default_model)
        };

        info!("[orchestrator] Boss agent '{}' using model '{}'", project.boss_agent, model);

        let provider = resolve_provider_for_model(&cfg, &model);
        match provider {
            Some(p) => (p, model),
            None => return Err("No AI provider configured".into()),
        }
    };

    let (base_system_prompt, max_rounds, tool_timeout) = {
        let cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;
        (
            cfg.default_system_prompt.clone(),
            cfg.max_tool_rounds,
            cfg.tool_timeout_secs,
        )
    };

    // Build agent roster description
    let agent_roster: Vec<String> = project.agents.iter()
        .filter(|a| a.role != "boss")
        .map(|a| format!("- **{}** (specialty: {}): {}", a.agent_id, a.specialty, a.status))
        .collect();

    // Boss agent system prompt
    let boss_soul = state.store.compose_agent_context(&project.boss_agent).unwrap_or(None);
    let skill_instructions = skills::get_enabled_skill_instructions(&state.store).unwrap_or_default();

    let mut sys_parts: Vec<String> = Vec::new();
    if let Some(sp) = &base_system_prompt { sys_parts.push(sp.clone()); }
    if let Some(soul) = boss_soul { sys_parts.push(soul); }
    if !skill_instructions.is_empty() { sys_parts.push(skill_instructions.clone()); }

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
    let enabled_ids: Vec<String> = skills::builtin_skills().iter()
        .filter(|s| state.store.is_skill_enabled(&s.id).unwrap_or(false))
        .map(|s| s.id.clone())
        .collect();
    if !enabled_ids.is_empty() {
        all_tools.extend(ToolDefinition::skill_tools(&enabled_ids));
    }
    all_tools.extend(boss_tools());

    // Create boss session
    let session_id = format!("eng-project-{}-boss", project_id);
    if state.store.get_session(&session_id).ok().flatten().is_none() {
        state.store.create_session(&session_id, &model, None)?;
    }

    // User message = project goal
    let user_msg = StoredMessage {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.clone(),
        role: "user".into(),
        content: format!("Execute this project:\n\nTitle: {}\nGoal: {}", project.title, project.goal),
        tool_calls_json: None,
        tool_call_id: None,
        name: None,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    state.store.add_message(&user_msg)?;

    let mut messages = state.store.load_conversation(&session_id, Some(&boss_system_prompt))?;
    let provider = AnyProvider::from_config(&provider_config);
    let pending = state.pending_approvals.clone();
    let pid = project_id.to_string();

    // Run the boss agent loop with custom tool handling
    // We use the standard agent loop but intercept orchestrator tools in the tool_executor
    let result = run_boss_agent_loop(
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
    ).await;

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

    app_handle.emit("project-event", serde_json::json!({
        "kind": "project_finished",
        "project_id": project_id,
        "success": result.is_ok(),
    })).ok();

    result
}

/// Boss agent loop — like run_agent_turn but intercepts orchestrator-specific tools.
async fn run_boss_agent_loop(
    app_handle: &tauri::AppHandle,
    provider: &AnyProvider,
    model: &str,
    messages: &mut Vec<Message>,
    tools: &[ToolDefinition],
    session_id: &str,
    run_id: &str,
    max_rounds: u32,
    pending_approvals: &PendingApprovals,
    tool_timeout_secs: u64,
    project_id: &str,
) -> Result<String, String> {
    let mut round = 0;
    let mut final_text = String::new();

    let orchestrator_tool_names = ["delegate_task", "check_agent_status", "send_agent_message", "project_complete", "create_sub_agent"];
    // All built-in tools skip HIL — the agent has full access
    let safe_tools = [
        // Core tools
        "exec", "fetch", "read_file", "write_file",
        "list_directory", "append_file", "delete_file",
        // Web tools
        "web_search", "web_read", "web_screenshot", "web_browse",
        // Soul / persona tools
        "soul_read", "soul_write", "soul_list",
        // Memory tools
        "memory_store", "memory_search",
        // Self-awareness
        "self_info",
        // Skill tools
        "email_send", "email_read",
        "slack_send", "slack_read",
        "github_api", "rest_api_call", "webhook_send", "image_generate",
        // Orchestrator tools
        "delegate_task", "check_agent_status", "send_agent_message", "project_complete",
    ];

    loop {
        round += 1;
        if round > max_rounds {
            warn!("[orchestrator] Max rounds ({}) reached", max_rounds);
            return Ok(final_text);
        }

        info!("[orchestrator] Boss round {}/{} project={}", round, max_rounds, project_id);

        // Call the AI model
        let chunks = provider.chat_stream(messages, tools, model, None).await?;

        // Assemble response
        let mut text_accum = String::new();
        let mut tool_call_map: std::collections::HashMap<usize, (String, String, String, Option<String>, Vec<ThoughtPart>)> = std::collections::HashMap::new();
        let mut has_tool_calls = false;

        // Extract confirmed model from API response
        let confirmed_model: Option<String> = chunks.iter().find_map(|c| c.model.clone());

        for chunk in &chunks {
            if let Some(dt) = &chunk.delta_text {
                text_accum.push_str(dt);
                let _ = app_handle.emit("engine-event", EngineEvent::Delta {
                    session_id: session_id.to_string(),
                    run_id: run_id.to_string(),
                    text: dt.clone(),
                });
            }
            for tc_delta in &chunk.tool_calls {
                has_tool_calls = true;
                let entry = tool_call_map.entry(tc_delta.index)
                    .or_insert_with(|| (String::new(), String::new(), String::new(), None, Vec::new()));
                if let Some(id) = &tc_delta.id { entry.0 = id.clone(); }
                if let Some(name) = &tc_delta.function_name { entry.1 = name.clone(); }
                if let Some(args_delta) = &tc_delta.arguments_delta { entry.2.push_str(args_delta); }
                if tc_delta.thought_signature.is_some() { entry.3 = tc_delta.thought_signature.clone(); }
            }
            if !chunk.thought_parts.is_empty() {
                let first_idx = chunk.tool_calls.first().map(|tc| tc.index).unwrap_or(0);
                let entry = tool_call_map.entry(first_idx)
                    .or_insert_with(|| (String::new(), String::new(), String::new(), None, Vec::new()));
                entry.4.extend(chunk.thought_parts.clone());
            }
        }

        if !has_tool_calls || tool_call_map.is_empty() {
            final_text = text_accum.clone();
            messages.push(Message {
                role: Role::Assistant,
                content: MessageContent::Text(text_accum),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
            let _ = app_handle.emit("engine-event", EngineEvent::Complete {
                session_id: session_id.to_string(),
                run_id: run_id.to_string(),
                text: final_text.clone(),
                tool_calls_count: 0,
                usage: None,
                model: confirmed_model.clone(),
            });
            return Ok(final_text);
        }

        // Build tool calls
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut sorted_indices: Vec<usize> = tool_call_map.keys().cloned().collect();
        sorted_indices.sort();
        for idx in sorted_indices {
            let (id, name, arguments, thought_sig, thoughts) = tool_call_map.get(&idx).unwrap();
            let call_id = if id.is_empty() { format!("call_{}", uuid::Uuid::new_v4()) } else { id.clone() };
            tool_calls.push(ToolCall {
                id: call_id,
                call_type: "function".into(),
                function: FunctionCall { name: name.clone(), arguments: arguments.clone() },
                thought_signature: thought_sig.clone(),
                thought_parts: thoughts.clone(),
            });
        }

        messages.push(Message {
            role: Role::Assistant,
            content: MessageContent::Text(text_accum),
            tool_calls: Some(tool_calls.clone()),
            tool_call_id: None,
            name: None,
        });

        // Execute tool calls
        for tc in &tool_calls {
            info!("[orchestrator] Boss tool call: {} id={}", tc.function.name, tc.id);

            // Check if orchestrator tool — handle directly, no HIL needed
            if orchestrator_tool_names.contains(&tc.function.name.as_str()) {
                let result = execute_boss_tool(tc, app_handle, project_id).await;
                let output = match result {
                    Some(Ok(text)) => text,
                    Some(Err(e)) => format!("Error: {}", e),
                    None => "Unknown orchestrator tool".into(),
                };

                let _ = app_handle.emit("engine-event", EngineEvent::ToolResultEvent {
                    session_id: session_id.to_string(),
                    run_id: run_id.to_string(),
                    tool_call_id: tc.id.clone(),
                    output: output.clone(),
                    success: true,
                });

                messages.push(Message {
                    role: Role::Tool,
                    content: MessageContent::Text(output),
                    tool_calls: None,
                    tool_call_id: Some(tc.id.clone()),
                    name: Some(tc.function.name.clone()),
                });
                continue;
            }

            // Standard tools — use HIL
            let skip_hil = safe_tools.contains(&tc.function.name.as_str());
            let approved = if skip_hil {
                true
            } else {
                let (approval_tx, approval_rx) = tokio::sync::oneshot::channel::<bool>();
                {
                    let mut map = pending_approvals.lock().unwrap();
                    map.insert(tc.id.clone(), approval_tx);
                }
                let _ = app_handle.emit("engine-event", EngineEvent::ToolRequest {
                    session_id: session_id.to_string(),
                    run_id: run_id.to_string(),
                    tool_call: tc.clone(),
                });
                match tokio::time::timeout(
                    std::time::Duration::from_secs(tool_timeout_secs),
                    approval_rx,
                ).await {
                    Ok(Ok(allowed)) => allowed,
                    _ => {
                        let mut map = pending_approvals.lock().unwrap();
                        map.remove(&tc.id);
                        false
                    }
                }
            };

            if !approved {
                messages.push(Message {
                    role: Role::Tool,
                    content: MessageContent::Text("Tool execution denied by user.".into()),
                    tool_calls: None,
                    tool_call_id: Some(tc.id.clone()),
                    name: Some(tc.function.name.clone()),
                });
                continue;
            }

            let result = crate::engine::tool_executor::execute_tool(tc, app_handle).await;
            let _ = app_handle.emit("engine-event", EngineEvent::ToolResultEvent {
                session_id: session_id.to_string(),
                run_id: run_id.to_string(),
                tool_call_id: tc.id.clone(),
                output: result.output.clone(),
                success: result.success,
            });
            messages.push(Message {
                role: Role::Tool,
                content: MessageContent::Text(result.output),
                tool_calls: None,
                tool_call_id: Some(tc.id.clone()),
                name: Some(tc.function.name.clone()),
            });
        }

        // If project_complete was called, we can stop the loop
        // (Check last message for project_complete result)
        let should_stop = tool_calls.iter().any(|tc| tc.function.name == "project_complete");
        if should_stop {
            info!("[orchestrator] Boss called project_complete, ending loop");
            return Ok(final_text);
        }
    }
}

/// Run a sub-agent on a delegated task within a project.
async fn run_sub_agent(
    app_handle: &tauri::AppHandle,
    project_id: &str,
    agent_id: &str,
    task_description: &str,
    context: &str,
) -> Result<String, String> {
    let state = app_handle.state::<EngineState>();

    // Get provider — use model routing for worker agents
    let (provider_config, model) = {
        let cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;
        let default_model = cfg.default_model.clone().unwrap_or_else(|| "gpt-4o".to_string());

        // Look up this agent in the project to get specialty and per-agent model override
        let agent_entry = state.store.get_project_agents(project_id).ok()
            .and_then(|agents| agents.into_iter().find(|a| a.agent_id == agent_id));
        let specialty = agent_entry.as_ref().map(|a| a.specialty.as_str()).unwrap_or("general");

        // Resolve model: per-agent field > model_routing > default
        let model = if let Some(agent_model) = agent_entry.as_ref().and_then(|a| a.model.as_deref()).filter(|m| !m.is_empty()) {
            agent_model.to_string()
        } else {
            cfg.model_routing.resolve(agent_id, "worker", specialty, &default_model)
        };

        info!("[orchestrator] Worker agent '{}' (specialty={}) using model '{}'", agent_id, specialty, model);

        let provider = resolve_provider_for_model(&cfg, &model);
        match provider {
            Some(p) => (p, model),
            None => return Err("No AI provider configured".into()),
        }
    };

    let (base_system_prompt, max_rounds, tool_timeout) = {
        let cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;
        (
            cfg.default_system_prompt.clone(),
            cfg.max_tool_rounds,
            cfg.tool_timeout_secs,
        )
    };

    // Build system prompt for sub-agent
    let agent_soul = state.store.compose_agent_context(agent_id).unwrap_or(None);
    let skill_instructions = skills::get_enabled_skill_instructions(&state.store).unwrap_or_default();

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

    // Create per-agent session
    let session_id = format!("eng-project-{}-{}", project_id, agent_id);
    let run_id = uuid::Uuid::new_v4().to_string();

    if state.store.get_session(&session_id).ok().flatten().is_none() {
        state.store.create_session(&session_id, &model, None)?;
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

    // Run the worker agent loop with report_progress interception
    let result = run_worker_agent_loop(
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
                store.update_project_agent_status(project_id, agent_id, "error", Some(err)).ok();
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

/// Worker agent loop — like run_agent_turn but intercepts report_progress.
async fn run_worker_agent_loop(
    app_handle: &tauri::AppHandle,
    provider: &AnyProvider,
    model: &str,
    messages: &mut Vec<Message>,
    tools: &[ToolDefinition],
    session_id: &str,
    run_id: &str,
    max_rounds: u32,
    pending_approvals: &PendingApprovals,
    tool_timeout_secs: u64,
    project_id: &str,
    agent_id: &str,
) -> Result<String, String> {
    let mut round = 0;
    let mut final_text = String::new();

    // All built-in tools skip HIL — the agent has full access
    let safe_tools = [
        // Core tools
        "exec", "fetch", "read_file", "write_file",
        "list_directory", "append_file", "delete_file",
        // Web tools
        "web_search", "web_read", "web_screenshot", "web_browse",
        // Soul / persona tools
        "soul_read", "soul_write", "soul_list",
        // Memory tools
        "memory_store", "memory_search",
        // Self-awareness
        "self_info",
        // Skill tools
        "email_send", "email_read",
        "slack_send", "slack_read",
        "github_api", "rest_api_call", "webhook_send", "image_generate",
        // Worker tool
        "report_progress",
    ];

    loop {
        round += 1;
        if round > max_rounds {
            warn!("[orchestrator] Worker {} max rounds reached", agent_id);
            return Ok(final_text);
        }

        info!("[orchestrator] Worker {} round {}/{} project={}", agent_id, round, max_rounds, project_id);

        let chunks = provider.chat_stream(messages, tools, model, None).await?;

        let mut text_accum = String::new();
        let mut tool_call_map: std::collections::HashMap<usize, (String, String, String, Option<String>, Vec<ThoughtPart>)> = std::collections::HashMap::new();
        let mut has_tool_calls = false;

        // Extract confirmed model from API response
        let _confirmed_model: Option<String> = chunks.iter().find_map(|c| c.model.clone());

        for chunk in &chunks {
            if let Some(dt) = &chunk.delta_text {
                text_accum.push_str(dt);
                let _ = app_handle.emit("engine-event", EngineEvent::Delta {
                    session_id: session_id.to_string(),
                    run_id: run_id.to_string(),
                    text: dt.clone(),
                });
            }
            for tc_delta in &chunk.tool_calls {
                has_tool_calls = true;
                let entry = tool_call_map.entry(tc_delta.index)
                    .or_insert_with(|| (String::new(), String::new(), String::new(), None, Vec::new()));
                if let Some(id) = &tc_delta.id { entry.0 = id.clone(); }
                if let Some(name) = &tc_delta.function_name { entry.1 = name.clone(); }
                if let Some(args_delta) = &tc_delta.arguments_delta { entry.2.push_str(args_delta); }
                if tc_delta.thought_signature.is_some() { entry.3 = tc_delta.thought_signature.clone(); }
            }
            if !chunk.thought_parts.is_empty() {
                let first_idx = chunk.tool_calls.first().map(|tc| tc.index).unwrap_or(0);
                let entry = tool_call_map.entry(first_idx)
                    .or_insert_with(|| (String::new(), String::new(), String::new(), None, Vec::new()));
                entry.4.extend(chunk.thought_parts.clone());
            }
        }

        if !has_tool_calls || tool_call_map.is_empty() {
            final_text = text_accum.clone();
            messages.push(Message {
                role: Role::Assistant,
                content: MessageContent::Text(text_accum),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
            return Ok(final_text);
        }

        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut sorted_indices: Vec<usize> = tool_call_map.keys().cloned().collect();
        sorted_indices.sort();
        for idx in sorted_indices {
            let (id, name, arguments, thought_sig, thoughts) = tool_call_map.get(&idx).unwrap();
            let call_id = if id.is_empty() { format!("call_{}", uuid::Uuid::new_v4()) } else { id.clone() };
            tool_calls.push(ToolCall {
                id: call_id,
                call_type: "function".into(),
                function: FunctionCall { name: name.clone(), arguments: arguments.clone() },
                thought_signature: thought_sig.clone(),
                thought_parts: thoughts.clone(),
            });
        }

        messages.push(Message {
            role: Role::Assistant,
            content: MessageContent::Text(text_accum),
            tool_calls: Some(tool_calls.clone()),
            tool_call_id: None,
            name: None,
        });

        let mut worker_done = false;

        for tc in &tool_calls {
            // Intercept report_progress
            if tc.function.name == "report_progress" {
                let result = execute_worker_tool(tc, app_handle, project_id, agent_id).await;
                let output = match result {
                    Some(Ok(text)) => text,
                    Some(Err(e)) => format!("Error: {}", e),
                    None => "Unknown tool".into(),
                };

                messages.push(Message {
                    role: Role::Tool,
                    content: MessageContent::Text(output),
                    tool_calls: None,
                    tool_call_id: Some(tc.id.clone()),
                    name: Some(tc.function.name.clone()),
                });

                // Check if worker reported done
                let args: serde_json::Value = serde_json::from_str(&tc.function.arguments).unwrap_or_default();
                if args["status"].as_str() == Some("done") {
                    worker_done = true;
                }
                continue;
            }

            // Standard tools with HIL
            let skip_hil = safe_tools.contains(&tc.function.name.as_str());
            let approved = if skip_hil {
                true
            } else {
                let (approval_tx, approval_rx) = tokio::sync::oneshot::channel::<bool>();
                {
                    let mut map = pending_approvals.lock().unwrap();
                    map.insert(tc.id.clone(), approval_tx);
                }
                let _ = app_handle.emit("engine-event", EngineEvent::ToolRequest {
                    session_id: session_id.to_string(),
                    run_id: run_id.to_string(),
                    tool_call: tc.clone(),
                });
                match tokio::time::timeout(
                    std::time::Duration::from_secs(tool_timeout_secs),
                    approval_rx,
                ).await {
                    Ok(Ok(allowed)) => allowed,
                    _ => {
                        let mut map = pending_approvals.lock().unwrap();
                        map.remove(&tc.id);
                        false
                    }
                }
            };

            if !approved {
                messages.push(Message {
                    role: Role::Tool,
                    content: MessageContent::Text("Tool execution denied by user.".into()),
                    tool_calls: None,
                    tool_call_id: Some(tc.id.clone()),
                    name: Some(tc.function.name.clone()),
                });
                continue;
            }

            let result = crate::engine::tool_executor::execute_tool(tc, app_handle).await;
            messages.push(Message {
                role: Role::Tool,
                content: MessageContent::Text(result.output),
                tool_calls: None,
                tool_call_id: Some(tc.id.clone()),
                name: Some(tc.function.name.clone()),
            });
        }

        if worker_done {
            info!("[orchestrator] Worker {} reported done", agent_id);
            return Ok(final_text);
        }
    }
}

/// Resolve a provider config for a given model string.
/// Uses smart prefix matching (gemini → Google, claude → Anthropic, etc.)
/// Falls back to default provider, then first provider.
fn resolve_provider_for_model(cfg: &EngineConfig, model: &str) -> Option<ProviderConfig> {
    // Match model prefix to provider kind
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

/// Helper to get a SessionStore from app_handle.
fn get_store(_app_handle: &tauri::AppHandle) -> Option<SessionStore> {
    crate::engine::sessions::SessionStore::open().ok()
}
