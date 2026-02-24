// commands/task.rs — Thin Tauri command wrappers for task operations.
//
// Business logic lives in engine/tasks.rs. This file only does:
//   1. Extract Tauri State<> from the managed state
//   2. Delegate to the engine layer
//   3. Map errors to String for the IPC boundary

use crate::engine::state::EngineState;
use crate::engine::tasks;
use crate::engine::types::*;
use log::info;
use tauri::State;

// ── Task Commands ──────────────────────────────────────────────────────

#[tauri::command]
pub fn engine_tasks_list(
    state: State<'_, EngineState>,
) -> Result<Vec<Task>, String> {
    state.store.list_tasks().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_task_create(
    state: State<'_, EngineState>,
    task: Task,
) -> Result<(), String> {
    info!("[engine] Creating task: {} ({})", task.title, task.id);
    state.store.create_task(&task)?;
    let aid = uuid::Uuid::new_v4().to_string();
    state.store.add_task_activity(&aid, &task.id, "created", None, &format!("Task created: {}", task.title))?;
    Ok(())
}

#[tauri::command]
pub fn engine_task_update(
    state: State<'_, EngineState>,
    task: Task,
) -> Result<(), String> {
    info!("[engine] Updating task: {} status={}", task.id, task.status);
    state.store.update_task(&task).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_task_delete(
    state: State<'_, EngineState>,
    task_id: String,
) -> Result<(), String> {
    info!("[engine] Deleting task: {}", task_id);
    state.store.delete_task(&task_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_task_move(
    state: State<'_, EngineState>,
    task_id: String,
    new_status: String,
) -> Result<(), String> {
    info!("[engine] Moving task {} → {}", task_id, new_status);
    let tasks = state.store.list_tasks()?;
    if let Some(mut task) = tasks.into_iter().find(|t| t.id == task_id) {
        let old_status = task.status.clone();
        task.status = new_status.clone();
        state.store.update_task(&task)?;
        let aid = uuid::Uuid::new_v4().to_string();
        state.store.add_task_activity(
            &aid, &task_id, "status_change", None,
            &format!("Moved from {} to {}", old_status, new_status),
        )?;
        Ok(())
    } else {
        Err(format!("Task not found: {}", task_id))
    }
}

#[tauri::command]
pub fn engine_task_activity(
    state: State<'_, EngineState>,
    task_id: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<TaskActivity>, String> {
    let limit = limit.unwrap_or(50);
    match task_id {
        Some(id) => state.store.list_task_activity(&id, limit).map_err(|e| e.to_string()),
        None => state.store.list_all_activity(limit).map_err(|e| e.to_string()),
    }
}

#[tauri::command]
pub fn engine_task_set_agents(
    state: State<'_, EngineState>,
    task_id: String,
    agents: Vec<TaskAgent>,
) -> Result<(), String> {
    info!("[engine] Setting {} agent(s) for task {}", agents.len(), task_id);
    state.store.set_task_agents(&task_id, &agents)?;
    let agent_names: Vec<&str> = agents.iter().map(|a| a.agent_id.as_str()).collect();
    let aid = uuid::Uuid::new_v4().to_string();
    state.store.add_task_activity(
        &aid, &task_id, "assigned", None,
        &format!("Agents assigned: {}", agent_names.join(", ")),
    )?;
    Ok(())
}

/// Run a task: dispatches to execute_task which handles multi-agent spawning.
#[tauri::command]
pub async fn engine_task_run(
    app_handle: tauri::AppHandle,
    state: State<'_, EngineState>,
    task_id: String,
) -> Result<String, String> {
    tasks::execute_task(&app_handle, &state, &task_id).await
}

/// Check for due cron tasks (front-end tick; heartbeat handles execution).
#[tauri::command]
pub fn engine_tasks_cron_tick(
    state: State<'_, EngineState>,
) -> Result<Vec<String>, String> {
    let due = state.store.get_due_cron_tasks()?;
    let mut triggered_ids = Vec::new();

    for task in due {
        info!("[engine] Cron task due: {} ({})", task.title, task.id);
        let now = chrono::Utc::now();
        let next = tasks::compute_next_run(&task.cron_schedule, &now);
        state.store.update_task_cron_run(&task.id, &now.to_rfc3339(), next.as_deref())?;
        let aid = uuid::Uuid::new_v4().to_string();
        state.store.add_task_activity(
            &aid, &task.id, "cron_triggered", None,
            &format!("Cron triggered: {}", task.cron_schedule.as_deref().unwrap_or("unknown")),
        )?;
        triggered_ids.push(task.id);
    }

    Ok(triggered_ids)
}

