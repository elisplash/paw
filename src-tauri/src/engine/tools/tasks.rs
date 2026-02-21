// Paw Agent Engine — Task management tools

use crate::atoms::types::*;
use crate::engine::state::EngineState;
use log::info;
use tauri::Emitter;
use tauri::Manager;
use crate::atoms::error::EngineResult;

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "create_task".into(),
                description: "Create a new task or scheduled automation for an agent. Tasks appear on the Tasks board. Add a cron_schedule (cron syntax) to make it run automatically.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "title": { "type": "string", "description": "Short task title" },
                        "description": { "type": "string", "description": "Detailed task instructions for the agent" },
                        "priority": { "type": "string", "enum": ["low", "medium", "high", "urgent"], "description": "Task priority (default: medium)" },
                        "agent_id": { "type": "string", "description": "Agent to assign the task to (default: 'default')" },
                        "cron_schedule": { "type": "string", "description": "Cron schedule for recurring tasks (e.g. '0 9 * * 1-5' for weekdays at 9am). Omit for one-shot tasks." }
                    },
                    "required": ["title", "description"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "list_tasks".into(),
                description: "List tasks on the board. Filter by status and/or show only scheduled automations.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "status_filter": { "type": "string", "description": "Filter by status: 'inbox', 'assigned', 'running', 'done', 'failed'" },
                        "cron_only": { "type": "boolean", "description": "If true, only show scheduled (cron) tasks" }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "manage_task".into(),
                description: "Update, delete, pause, enable, or trigger a task.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "task_id": { "type": "string", "description": "ID of the task to manage" },
                        "action": { "type": "string", "enum": ["update", "delete", "run_now", "pause", "enable"], "description": "Action to perform" },
                        "title": { "type": "string", "description": "New title (for action=update)" },
                        "description": { "type": "string", "description": "New description (for action=update)" },
                        "priority": { "type": "string", "description": "New priority (for action=update)" },
                        "status": { "type": "string", "description": "New status (for action=update)" },
                        "cron_schedule": { "type": "string", "description": "New cron schedule (for action=update)" },
                        "agent_id": { "type": "string", "description": "Re-assign to agent (for action=update)" }
                    },
                    "required": ["task_id", "action"]
                }),
            },
        },
    ]
}

pub async fn execute(
    name: &str,
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    _agent_id: &str,
) -> Option<Result<String, String>> {
    Some(match name {
        "create_task"  => execute_create_task(args, app_handle).await.map_err(|e| e.to_string()),
        "list_tasks"   => execute_list_tasks(args, app_handle).await.map_err(|e| e.to_string()),
        "manage_task"  => execute_manage_task(args, app_handle).await.map_err(|e| e.to_string()),
        _ => return None,
    })
}

async fn execute_create_task(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
) -> EngineResult<String> {
    let title = args["title"].as_str().ok_or("create_task: missing 'title'")?;
    let description = args["description"].as_str().ok_or("create_task: missing 'description'")?;
    let priority = args["priority"].as_str().unwrap_or("medium").to_string();
    let agent_id = args["agent_id"].as_str().unwrap_or("default").to_string();
    let cron_schedule = args["cron_schedule"].as_str().map(String::from);

    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let cron_enabled = cron_schedule.is_some();
    let next_run_at = if cron_enabled { Some(now.clone()) } else { None };

    let task = crate::engine::types::Task {
        id: id.clone(),
        title: title.to_string(),
        description: description.to_string(),
        status: if cron_enabled { "assigned".into() } else { "inbox".into() },
        priority: priority.clone(),
        assigned_agent: Some(agent_id.clone()),
        assigned_agents: vec![crate::engine::types::TaskAgent {
            agent_id: agent_id.clone(),
            role: "lead".into(),
        }],
        session_id: None,
        cron_schedule: cron_schedule.clone(),
        cron_enabled,
        last_run_at: None,
        next_run_at,
        created_at: now.clone(),
        updated_at: now,
        model: None,
    };

    state.store.create_task(&task)?;

    let aid = uuid::Uuid::new_v4().to_string();
    state.store.add_task_activity(&aid, &id, "created", None,
        &format!("Task created via chat: {}", title)).ok();

    info!("[engine] create_task tool: '{}' agent={} cron={:?}", title, agent_id, cron_schedule);
    app_handle.emit("task-updated", serde_json::json!({ "task_id": id })).ok();

    let schedule_info = if let Some(ref s) = cron_schedule {
        format!("\n- **Schedule**: {} (will run automatically via heartbeat)", s)
    } else {
        "\n- **Type**: One-shot task (run manually from Tasks board)".into()
    };

    Ok(format!(
        "Task created successfully!\n\n\
        - **ID**: {}\n\
        - **Title**: {}\n\
        - **Priority**: {}\n\
        - **Agent**: {}{}",
        id, title, priority, agent_id, schedule_info
    ))
}

async fn execute_list_tasks(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
) -> EngineResult<String> {
    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    let status_filter = args["status_filter"].as_str();
    let cron_only = args["cron_only"].as_bool().unwrap_or(false);

    let tasks = state.store.list_tasks()?;

    let filtered: Vec<_> = tasks.into_iter()
        .filter(|t| {
            if let Some(sf) = status_filter {
                if t.status != sf { return false; }
            }
            if cron_only && t.cron_schedule.is_none() { return false; }
            true
        })
        .collect();

    if filtered.is_empty() {
        return Ok("No tasks found matching the criteria.".into());
    }

    let mut output = format!("Found {} task(s):\n\n", filtered.len());
    for t in &filtered {
        let schedule = t.cron_schedule.as_deref().unwrap_or("none");
        let enabled = if t.cron_enabled { "enabled" } else { "paused" };
        let agent = t.assigned_agent.as_deref().unwrap_or("unassigned");
        let next = t.next_run_at.as_deref().unwrap_or("-");
        output.push_str(&format!(
            "---\n**{}** (ID: `{}`)\n- Status: {} | Priority: {}\n- Agent: {} | Schedule: {} ({})\n- Next run: {}\n- Description: {}\n\n",
            t.title, t.id, t.status, t.priority, agent, schedule, enabled, next,
            if t.description.len() > 150 { format!("{}...", &t.description[..150]) } else { t.description.clone() }
        ));
    }

    Ok(output)
}

async fn execute_manage_task(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
) -> EngineResult<String> {
    let task_id = args["task_id"].as_str().ok_or("manage_task: missing 'task_id'")?.to_string();
    let action = args["action"].as_str().ok_or("manage_task: missing 'action'")?;

    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    match action {
        "delete" => {
            state.store.delete_task(&task_id)?;
            info!("[engine] manage_task: deleted {}", task_id);
            app_handle.emit("task-updated", serde_json::json!({ "task_id": task_id })).ok();
            Ok(format!("Task {} deleted.", task_id))
        }
        "run_now" => {
            let tasks = state.store.list_tasks()?;
            if let Some(mut task) = tasks.into_iter().find(|t| t.id == task_id) {
                task.cron_enabled = true;
                task.next_run_at = Some(chrono::Utc::now().to_rfc3339());
                task.status = "assigned".to_string();
                state.store.update_task(&task)?;
                let aid = uuid::Uuid::new_v4().to_string();
                state.store.add_task_activity(&aid, &task_id, "cron_triggered", None,
                    "Manually triggered via chat — will run on next heartbeat cycle").ok();
                app_handle.emit("task-updated", serde_json::json!({ "task_id": task_id })).ok();
                Ok(format!("Task '{}' queued for immediate execution. It will run within the next 60-second heartbeat cycle.", task.title))
            } else {
                Err(format!("Task not found: {}", task_id).into())
            }
        }
        "pause" => {
            let tasks = state.store.list_tasks()?;
            if let Some(mut task) = tasks.into_iter().find(|t| t.id == task_id) {
                task.cron_enabled = false;
                state.store.update_task(&task)?;
                app_handle.emit("task-updated", serde_json::json!({ "task_id": task_id })).ok();
                Ok(format!("Automation '{}' paused.", task.title))
            } else {
                Err(format!("Task not found: {}", task_id).into())
            }
        }
        "enable" => {
            let tasks = state.store.list_tasks()?;
            if let Some(mut task) = tasks.into_iter().find(|t| t.id == task_id) {
                task.cron_enabled = true;
                if task.next_run_at.is_none() {
                    task.next_run_at = Some(chrono::Utc::now().to_rfc3339());
                }
                state.store.update_task(&task)?;
                app_handle.emit("task-updated", serde_json::json!({ "task_id": task_id })).ok();
                Ok(format!("Automation '{}' enabled. Will run on next heartbeat.", task.title))
            } else {
                Err(format!("Task not found: {}", task_id).into())
            }
        }
        "update" => {
            let tasks = state.store.list_tasks()?;
            if let Some(mut task) = tasks.into_iter().find(|t| t.id == task_id) {
                if let Some(t) = args["title"].as_str()       { task.title = t.to_string(); }
                if let Some(d) = args["description"].as_str() { task.description = d.to_string(); }
                if let Some(p) = args["priority"].as_str()    { task.priority = p.to_string(); }
                if let Some(s) = args["status"].as_str()      { task.status = s.to_string(); }
                if let Some(s) = args["cron_schedule"].as_str() {
                    task.cron_schedule = Some(s.to_string());
                    task.cron_enabled = true;
                    task.next_run_at = Some(chrono::Utc::now().to_rfc3339());
                }
                if let Some(a) = args["agent_id"].as_str() {
                    task.assigned_agent = Some(a.to_string());
                    task.assigned_agents = vec![crate::engine::types::TaskAgent {
                        agent_id: a.to_string(),
                        role: "lead".into(),
                    }];
                }
                task.updated_at = chrono::Utc::now().to_rfc3339();
                state.store.update_task(&task)?;
                app_handle.emit("task-updated", serde_json::json!({ "task_id": task_id })).ok();
                Ok(format!("Task '{}' updated.", task.title))
            } else {
                Err(format!("Task not found: {}", task_id).into())
            }
        }
        _ => Err(format!("Unknown action: {}. Use: update, delete, run_now, pause, enable", action).into()),
    }
}
