// Paw Agent Engine — Orchestrator Tool Handlers
//
// Boss tool handlers: delegate_task, check_agent_status, send_agent_message,
// project_complete, create_sub_agent.
// Worker tool handler: report_progress (execute_worker_tool).

use crate::engine::types::*;
use crate::engine::sessions::SessionStore;
use log::error;
use tauri::Emitter;

use super::sub_agent::run_sub_agent;
use crate::atoms::error::EngineResult;

// ── Helpers ────────────────────────────────────────────────────────────

/// Helper to get a SessionStore from app_handle.
pub(crate) fn get_store(_app_handle: &tauri::AppHandle) -> Option<SessionStore> {
    crate::engine::sessions::SessionStore::open().ok()
}

// ── Boss tool dispatcher ───────────────────────────────────────────────

/// Execute an orchestrator-specific tool call for the boss agent.
/// Returns `Some(Ok/Err)` for orchestrator tools, `None` for unknown tools.
pub async fn execute_boss_tool(
    tool_call: &ToolCall,
    app_handle: &tauri::AppHandle,
    project_id: &str,
) -> Option<Result<String, String>> {
    let name = &tool_call.function.name;
    let args: serde_json::Value = serde_json::from_str(&tool_call.function.arguments)
        .unwrap_or(serde_json::json!({}));

    match name.as_str() {
        "delegate_task" => Some(handle_delegate_task(&args, app_handle, project_id).map_err(|e| e.to_string())),
        "check_agent_status" => Some(handle_check_agent_status(app_handle, project_id).map_err(|e| e.to_string())),
        "send_agent_message" => Some(handle_send_agent_message(&args, app_handle, project_id).map_err(|e| e.to_string())),
        "project_complete" => Some(handle_project_complete(&args, app_handle, project_id).map_err(|e| e.to_string())),
        "create_sub_agent" => Some(handle_create_sub_agent(&args, app_handle, project_id).map_err(|e| e.to_string())),
        _ => None,
    }
}

// ── Worker tool dispatcher ─────────────────────────────────────────────

/// Execute a worker-specific tool (report_progress).
/// Returns `Some(Ok/Err)` for worker tools, `None` for unknown tools.
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
                let db_status = match status.as_str() {
                    "done" => "done",
                    "error" | "blocked" => "error",
                    _ => "working",
                };
                store.update_project_agent_status(project_id, agent_id, db_status, Some(&message)).ok();

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

// ── Boss tool handlers ─────────────────────────────────────────────────

fn handle_delegate_task(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    project_id: &str,
) -> EngineResult<String> {
    let agent_id = args["agent_id"].as_str().unwrap_or("").to_string();
    let task_desc = args["task_description"].as_str().unwrap_or("").to_string();
    let context = args["context"].as_str().unwrap_or("").to_string();

    if agent_id.is_empty() || task_desc.is_empty() {
        return Err("delegate_task requires agent_id and task_description".into());
    }

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
        store.update_project_agent_status(project_id, &agent_id, "working", Some(&task_desc)).ok();
    }

    app_handle.emit("project-event", serde_json::json!({
        "kind": "delegation",
        "project_id": project_id,
        "agent_id": agent_id,
        "task": task_desc,
    })).ok();

    let app = app_handle.clone();
    let pid = project_id.to_string();
    let aid = agent_id.clone();
    let task = task_desc.clone();
    let ctx = context.clone();

    tauri::async_runtime::spawn(async move {
        if let Err(e) = run_sub_agent(&app, &pid, &aid, &task, &ctx).await {
            error!("[orchestrator] Sub-agent {} failed: {}", aid, e);
            if let Some(store) = get_store(&app) {
                store.update_project_agent_status(&pid, &aid, "error", Some(&e.to_string())).ok();
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

    Ok(format!("Task delegated to agent '{}'. They are now working on: {}", agent_id, task_desc))
}

fn handle_check_agent_status(
    app_handle: &tauri::AppHandle,
    project_id: &str,
) -> EngineResult<String> {
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

                    Ok(format!("Agent Status:\n{}", status_lines.join("\n")))
                }
                Err(e) => Err(e)
            }
        }
        None => Err("Could not access engine store".into())
    }
}

fn handle_send_agent_message(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    project_id: &str,
) -> EngineResult<String> {
    let to = args["to_agent"].as_str().unwrap_or("").to_string();
    let message = args["message"].as_str().unwrap_or("").to_string();

    if to.is_empty() || message.is_empty() {
        return Err("send_agent_message requires to_agent and message".into());
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

    Ok(format!("Message sent to {}", to))
}

fn handle_project_complete(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    project_id: &str,
) -> EngineResult<String> {
    let summary = args["summary"].as_str().unwrap_or("").to_string();
    let status = args["status"].as_str().unwrap_or("completed").to_string();

    let store = get_store(app_handle);
    if let Some(ref store) = store {
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

    Ok(format!("Project marked as {}. Summary: {}", status, summary))
}

fn handle_create_sub_agent(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    project_id: &str,
) -> EngineResult<String> {
    let name = args["name"].as_str().unwrap_or("").to_string();
    let role = args["role"].as_str().unwrap_or("worker").to_string();
    let specialty = args["specialty"].as_str().unwrap_or("general").to_string();
    let system_prompt = args["system_prompt"].as_str().unwrap_or("").to_string();
    let capabilities: Vec<String> = args["capabilities"].as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    let model = args["model"].as_str().map(|s| s.to_string()).filter(|s| !s.is_empty());

    if name.is_empty() {
        return Err("create_sub_agent requires a 'name' argument".into());
    }
    if system_prompt.is_empty() {
        return Err("create_sub_agent requires a 'system_prompt' argument".into());
    }

    let agent_id = name.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' })
        .collect::<String>();

    let store = get_store(app_handle);
    match store {
        Some(store) => {
            if let Ok(existing) = store.get_project_agents(project_id) {
                if existing.iter().any(|a| a.agent_id == agent_id) {
                    return Err(format!("Agent '{}' already exists in this project", agent_id).into());
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
                    let identity_content = format!(
                        "# {}\n\n## Identity\nAgent ID: {}\nRole: {}\nSpecialty: {}\n\n## Personality & Instructions\n{}\n",
                        name, agent_id, role, specialty, system_prompt
                    );
                    store.set_agent_file(&agent_id, "IDENTITY.md", &identity_content).ok();

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

                    app_handle.emit("project-event", serde_json::json!({
                        "kind": "agent_created",
                        "project_id": project_id,
                        "agent_id": agent_id,
                        "role": role,
                        "specialty": specialty,
                    })).ok();

                    Ok(format!(
                        "Successfully created sub-agent '{}' (role={}, specialty={}). You can now delegate tasks to this agent using delegate_task with agent_id='{}'.",
                        agent_id, role, specialty, agent_id
                    ))
                }
                Err(e) => Err(e.into())
            }
        }
        None => Err("Could not access engine store".into())
    }
}
