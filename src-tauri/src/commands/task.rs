// commands/task.rs — Task commands + execute_task organism + background cron heartbeat.

use crate::commands::state::{EngineState, normalize_model_name, resolve_provider_for_model};
use crate::atoms::constants::{CRON_SESSION_KEEP_MESSAGES, CRON_MAX_TOOL_ROUNDS};
use crate::engine::{agent_loop, skills, sessions, telegram, sol_dex};
use crate::engine::providers::AnyProvider;
use crate::engine::types::*;
use log::{info, warn, error};
use std::collections::HashMap;
use tauri::{State, Emitter, Manager};

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
    execute_task(&app_handle, &state, &task_id).await
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
        let next = compute_next_run(&task.cron_schedule, &now);
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

// ── Core Organism: execute_task ────────────────────────────────────────

/// Standalone task execution — callable from both the Tauri command and the
/// background cron heartbeat. Handles multi-agent spawning and session
/// management. This is the core logic for running a task.
pub async fn execute_task(
    app_handle: &tauri::AppHandle,
    state: &EngineState,
    task_id: &str,
) -> Result<String, String> {
    // ── Dedup guard: skip if this task is already running ──
    {
        let mut inflight = state.inflight_tasks.lock();
        if inflight.contains(task_id) {
            info!("[engine] Task '{}' already in flight — skipping duplicate", task_id);
            return Err(format!("Task {} is already running", task_id));
        }
        inflight.insert(task_id.to_string());
    }

    let run_id = uuid::Uuid::new_v4().to_string();

    let tasks = state.store.list_tasks()?;
    let task = tasks.into_iter().find(|t| t.id == task_id)
        .ok_or_else(|| format!("Task not found: {}", task_id))?;

    let agent_ids: Vec<String> = if !task.assigned_agents.is_empty() {
        task.assigned_agents.iter().map(|a| a.agent_id.clone()).collect()
    } else if let Some(ref agent) = task.assigned_agent {
        vec![agent.clone()]
    } else {
        vec!["default".to_string()]
    };

    info!("[engine] Running task '{}' with {} agent(s): {:?}", task.title, agent_ids.len(), agent_ids);

    {
        let mut t = task.clone();
        t.status = "in_progress".to_string();
        state.store.update_task(&t)?;
    }

    for agent_id in &agent_ids {
        let aid = uuid::Uuid::new_v4().to_string();
        state.store.add_task_activity(
            &aid, task_id, "agent_started", Some(agent_id),
            &format!("Agent {} started working on: {}", agent_id, task.title),
        )?;
    }

    let task_prompt = if task.description.is_empty() {
        task.title.clone()
    } else {
        format!("{}\n\n{}", task.title, task.description)
    };

    let (base_system_prompt, max_rounds, tool_timeout, model_routing, default_model) = {
        let cfg = state.config.lock();
        (
            cfg.default_system_prompt.clone(),
            cfg.max_tool_rounds,
            cfg.tool_timeout_secs,
            cfg.model_routing.clone(),
            cfg.default_model.clone().unwrap_or_else(|| "gpt-4o".to_string()),
        )
    };

    let first_agent_id = agent_ids.first().map(|s| s.as_str()).unwrap_or("default");
    let skill_instructions = skills::get_enabled_skill_instructions(&state.store, first_agent_id).unwrap_or_default();

    let mut all_tools = ToolDefinition::builtins();
    let enabled_ids: Vec<String> = skills::builtin_skills().iter()
        .filter(|s| state.store.is_skill_enabled(&s.id).unwrap_or(false))
        .map(|s| s.id.clone())
        .collect();
    if !enabled_ids.is_empty() {
        all_tools.extend(ToolDefinition::skill_tools(&enabled_ids));
    }
    if !enabled_ids.contains(&"telegram".into()) {
        if let Ok(tg_cfg) = telegram::load_telegram_config(app_handle) {
            if !tg_cfg.bot_token.is_empty() {
                all_tools.push(ToolDefinition::telegram_send());
                all_tools.push(ToolDefinition::telegram_read());
            }
        }
    }

    // Add tools from connected MCP servers
    let mcp_tools = ToolDefinition::mcp_tools(app_handle);
    if !mcp_tools.is_empty() {
        info!("[engine] Adding {} MCP tools for task", mcp_tools.len());
        all_tools.extend(mcp_tools);
    }

    let pending = state.pending_approvals.clone();
    let store_path = sessions::engine_db_path();
    let task_id_for_spawn = task_id.to_string();
    let agent_count = agent_ids.len();
    let is_recurring = task.cron_schedule.as_ref().is_some_and(|s| !s.is_empty());
    let sem = state.run_semaphore.clone();
    let inflight = state.inflight_tasks.clone();

    let is_persistent = task.persistent;

    let task_daily_budget = {
        let cfg = state.config.lock();
        cfg.daily_budget_usd
    };
    let task_daily_tokens = state.daily_tokens.clone();

    let mut handles = Vec::new();

    for agent_id in agent_ids.clone() {
        let session_id = format!("eng-task-{}-{}", task.id, agent_id);

        if is_recurring {
            match state.store.prune_session_messages(&session_id, CRON_SESSION_KEEP_MESSAGES) {
                Ok(pruned) if pruned > 0 => {
                    info!("[engine] Pruned {} old messages from cron session {} (kept {})",
                        pruned, session_id, CRON_SESSION_KEEP_MESSAGES);
                }
                Err(e) => {
                    warn!("[engine] Failed to prune cron session {}: {}", session_id, e);
                }
                _ => {}
            }
        }

        let agent_model = if let Some(ref task_model) = task.model {
            if !task_model.is_empty() {
                let normalized = normalize_model_name(task_model).to_string();
                if normalized != *task_model {
                    info!("[engine] Task '{}' model remapped: {} → {}", task.title, task_model, normalized);
                }
                info!("[engine] Task '{}' has explicit model override: {}", task.title, normalized);
                normalized
            } else {
                model_routing.resolve(&agent_id, "worker", "", &default_model)
            }
        } else {
            model_routing.resolve(&agent_id, "worker", "", &default_model)
        };
        info!("[engine] Agent '{}' resolved model: {} (task_override: {:?}, default: {})", agent_id, agent_model, task.model, default_model);

        let (provider_config, model) = {
            let cfg = state.config.lock();
            let model = agent_model;
            let provider = resolve_provider_for_model(&model, &cfg.providers)
                .or_else(|| {
                    cfg.providers.iter().find(|p| p.default_model.as_deref() == Some(model.as_str())).cloned()
                })
                .or_else(|| {
                    cfg.default_provider.as_ref()
                        .and_then(|dp| cfg.providers.iter().find(|p| p.id == *dp).cloned())
                })
                .or_else(|| cfg.providers.first().cloned());
            match provider {
                Some(p) => (p, model),
                None => return Err("No AI provider configured".into()),
            }
        };

        if state.store.get_session(&session_id).ok().flatten().is_none() {
            state.store.create_session(&session_id, &model, None, Some(&agent_id))?;
        }

        let agent_context = state.store.compose_core_context(&agent_id).unwrap_or(None);

        let mut parts: Vec<String> = Vec::new();
        if let Some(sp) = &base_system_prompt { parts.push(sp.clone()); }
        if let Some(ac) = agent_context { parts.push(ac); }
        if !skill_instructions.is_empty() { parts.push(skill_instructions.clone()); }

        {
            let user_tz = {
                let cfg = state.config.lock();
                cfg.user_timezone.clone()
            };
            let now_utc = chrono::Utc::now();
            if let Ok(tz) = user_tz.parse::<chrono_tz::Tz>() {
                let local: chrono::DateTime<chrono_tz::Tz> = now_utc.with_timezone(&tz);
                parts.push(format!(
                    "## Local Time\n- **Current time**: {}\n- **Timezone**: {} (UTC{})\n- **Day of week**: {}",
                    local.format("%Y-%m-%d %H:%M:%S"),
                    tz.name(),
                    local.format("%:z"),
                    local.format("%A"),
                ));
            } else {
                let local = chrono::Local::now();
                parts.push(format!(
                    "## Local Time\n- **Current time**: {}\n- **Timezone**: {} (UTC{})\n- **Day of week**: {}",
                    local.format("%Y-%m-%d %H:%M:%S"),
                    local.format("%Z"),
                    local.format("%:z"),
                    local.format("%A"),
                ));
            }
        }

        let agent_count_note = if agent_count > 1 {
            format!("\n\nYou are agent '{}', one of {} agents working on this task collaboratively. Focus on your area of expertise. Be thorough but avoid duplicating work other agents may do.", agent_id, agent_count)
        } else {
            String::new()
        };

        let cron_context = if let Some(ref sched) = task.cron_schedule {
            if !sched.is_empty() {
                format!(
                    "\n\n**Execution mode:** This task was triggered automatically by a scheduled cron job (schedule: `{}`).\n\
                    - You are running autonomously — there is no human operator watching.\n\
                    - Complete your work, produce a clear summary, and exit cleanly.\n\
                    - Do NOT ask questions or wait for user input.\n\
                    - If you encounter errors, log them clearly and move on.\n\
                    - This task will run again on schedule, so focus on the current cycle only.",
                    sched
                )
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        parts.push(format!(
            "## Current Task\nYou are working on a task from the task board.\n- **Title:** {}\n- **Priority:** {}{}{}\n\nComplete this task thoroughly. When done, summarize what you accomplished.",
            task.title, task.priority, agent_count_note, cron_context
        ));

        let full_system_prompt = parts.join("\n\n---\n\n");

        let user_msg = StoredMessage {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.clone(),
            role: "user".into(),
            content: task_prompt.clone(),
            tool_calls_json: None,
            tool_call_id: None,
            name: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        state.store.add_message(&user_msg)?;

        let mut messages = state.store.load_conversation(&session_id, Some(&full_system_prompt))?;

        let provider = AnyProvider::from_config(&provider_config);
        let pending_clone = pending.clone();
        let task_id_clone = task_id.to_string();
        let store_path_clone = store_path.clone();
        let run_id_clone = run_id.clone();
        let app_handle_clone = app_handle.clone();
        let all_tools_clone = all_tools.clone();
        let model_clone = model.clone();
        let sem_clone = sem.clone();
        let task_daily_tokens_clone = task_daily_tokens.clone();
        let task_daily_budget_clone = task_daily_budget;

        let effective_max_rounds = if is_recurring {
            max_rounds.min(CRON_MAX_TOOL_ROUNDS)
        } else {
            max_rounds
        };

        let handle = tauri::async_runtime::spawn(async move {
            let _permit = sem_clone.acquire_owned().await.ok();
            info!("[engine] Task agent '{}' acquired run slot", agent_id);

            let result = agent_loop::run_agent_turn(
                &app_handle_clone,
                &provider,
                &model_clone,
                &mut messages,
                &all_tools_clone,
                &session_id,
                &run_id_clone,
                effective_max_rounds,
                None,
                &pending_clone,
                tool_timeout,
                &agent_id,
                task_daily_budget_clone,
                Some(&task_daily_tokens_clone),
                None, // thinking_level
                false, // auto_approve_all — tasks use safe default; opt-in is per-chat
            ).await;

            if let Ok(conn) = rusqlite::Connection::open(&store_path_clone) {
                match &result {
                    Ok(text) => {
                        let msg_id = uuid::Uuid::new_v4().to_string();
                        conn.execute(
                            "INSERT INTO messages (id, session_id, role, content) VALUES (?1, ?2, 'assistant', ?3)",
                            rusqlite::params![msg_id, session_id, text],
                        ).ok();
                        let aid = uuid::Uuid::new_v4().to_string();
                        conn.execute(
                            "INSERT INTO task_activity (id, task_id, kind, agent, content) VALUES (?1, ?2, 'agent_completed', ?3, ?4)",
                            rusqlite::params![aid, task_id_clone, agent_id, format!("Agent {} completed. Summary: {}", agent_id, truncate_utf8(text, 200))],
                        ).ok();
                    }
                    Err(err) => {
                        let aid = uuid::Uuid::new_v4().to_string();
                        conn.execute(
                            "INSERT INTO task_activity (id, task_id, kind, agent, content) VALUES (?1, ?2, 'agent_error', ?3, ?4)",
                            rusqlite::params![aid, task_id_clone, agent_id, format!("Agent {} error: {}", agent_id, err)],
                        ).ok();
                    }
                }
            }

            result
        });

        handles.push(handle);
    }

    let app_handle_final = app_handle.clone();
    let inflight_clone = inflight.clone();
    let task_id_for_cleanup = task_id.to_string();
    tauri::async_runtime::spawn(async move {
        let mut any_ok = false;
        for handle in handles {
            if let Ok(Ok(_)) = handle.await {
                any_ok = true;
            }
        }

        inflight_clone.lock().remove(&task_id_for_cleanup);

        if let Ok(conn) = rusqlite::Connection::open(store_path) {
            let new_status = if is_recurring || is_persistent {
                "in_progress"
            } else if any_ok {
                "review"
            } else {
                "blocked"
            };
            conn.execute(
                "UPDATE tasks SET status = ?2, updated_at = datetime('now') WHERE id = ?1",
                rusqlite::params![task_id_for_spawn, new_status],
            ).ok();

            // Persistent tasks re-queue immediately with a short cooldown
            if is_persistent && !is_recurring {
                let next = chrono::Utc::now() + chrono::Duration::seconds(30);
                conn.execute(
                    "UPDATE tasks SET next_run_at = ?2, cron_enabled = 1, last_run_at = ?3 WHERE id = ?1",
                    rusqlite::params![task_id_for_spawn, next.to_rfc3339(), chrono::Utc::now().to_rfc3339()],
                ).ok();
            }

            let aid = uuid::Uuid::new_v4().to_string();
            let summary = if is_persistent {
                if agent_count > 1 {
                    format!("All {} agents finished (persistent). Re-queuing in 30s.", agent_count)
                } else {
                    "Persistent task cycle completed. Re-queuing in 30s.".to_string()
                }
            } else if is_recurring {
                if agent_count > 1 {
                    format!("All {} agents finished (recurring). Staying in_progress for next run.", agent_count)
                } else {
                    "Cron cycle completed. Staying in_progress for next run.".to_string()
                }
            } else if agent_count > 1 {
                format!("All {} agents finished. Status: {}", agent_count, new_status)
            } else {
                format!("Task completed. Status: {}", new_status)
            };
            conn.execute(
                "INSERT INTO task_activity (id, task_id, kind, content) VALUES (?1, ?2, 'status_change', ?3)",
                rusqlite::params![aid, task_id_for_spawn, summary],
            ).ok();
        }

        app_handle_final.emit("task-updated", serde_json::json!({
            "task_id": task_id_for_spawn,
            "status": if is_recurring || is_persistent { "in_progress" } else if any_ok { "review" } else { "blocked" },
        })).ok();
    });

    Ok(run_id)
}

// ── Background Cron Heartbeat ──────────────────────────────────────────

/// Check all open positions against current prices. Auto-sell on SL/TP.
async fn check_positions(app_handle: &tauri::AppHandle) {
    let state = app_handle.state::<EngineState>();

    let positions = match state.store.list_positions(Some("open")) {
        Ok(p) => p,
        Err(e) => {
            warn!("[positions] Failed to load open positions: {}", e);
            return;
        }
    };

    if positions.is_empty() { return; }

    let creds = match skills::get_skill_credentials(&state.store, "solana") {
        Ok(c) => c,
        Err(e) => {
            warn!("[positions] Cannot load Solana credentials: {}", e);
            return;
        }
    };

    if !creds.contains_key("SOLANA_WALLET_ADDRESS") || !creds.contains_key("SOLANA_PRIVATE_KEY") {
        return;
    }

    info!("[positions] Checking {} open position(s)", positions.len());

    for pos in &positions {
        if let Some(ref last) = pos.last_checked_at {
            if let Ok(last_dt) = chrono::NaiveDateTime::parse_from_str(last, "%Y-%m-%d %H:%M:%S") {
                let now = chrono::Utc::now().naive_utc();
                if (now - last_dt).num_seconds() < 55 {
                    continue;
                }
            }
        }

        let current_price = match sol_dex::get_token_price_usd(&pos.mint).await {
            Ok(p) => p,
            Err(e) => {
                warn!("[positions] Price lookup failed for {} ({}): {}", pos.symbol, &pos.mint[..std::cmp::min(8, pos.mint.len())], e);
                continue;
            }
        };

        let _ = state.store.update_position_price(&pos.id, current_price);

        if pos.entry_price_usd <= 0.0 { continue; }
        let ratio = current_price / pos.entry_price_usd;

        if ratio <= (1.0 - pos.stop_loss_pct) {
            info!("[positions] 🛑 STOP-LOSS triggered for {} — entry ${:.8}, now ${:.8} ({:.1}% loss)",
                pos.symbol, pos.entry_price_usd, current_price, (1.0 - ratio) * 100.0);

            let sell_result = execute_position_sell(app_handle, &creds, &pos.mint, &pos.symbol, pos.current_amount).await;
            match sell_result {
                Ok(tx) => {
                    let _ = state.store.close_position(&pos.id, "closed_sl", Some(&tx));
                    let _ = state.store.insert_trade(
                        "sol_swap", Some("sell"), Some(&format!("{} → SOL", pos.symbol)),
                        Some(&pos.mint), &pos.current_amount.to_string(),
                        None, None, "completed", None, Some("SOL"),
                        &format!("Auto stop-loss at {:.1}% loss", (1.0 - ratio) * 100.0),
                        None, None, Some(&tx),
                    );
                    app_handle.emit("position-closed", serde_json::json!({
                        "id": pos.id, "symbol": pos.symbol, "reason": "stop_loss",
                        "entry_price": pos.entry_price_usd, "exit_price": current_price,
                    })).ok();
                    info!("[positions] ✅ Stop-loss sell executed for {} — tx: {}", pos.symbol, &tx[..std::cmp::min(16, tx.len())]);
                }
                Err(e) => {
                    error!("[positions] ❌ Stop-loss sell FAILED for {}: {}", pos.symbol, e);
                }
            }
        } else if ratio >= pos.take_profit_pct {
            info!("[positions] 🎯 TAKE-PROFIT triggered for {} — entry ${:.8}, now ${:.8} ({:.1}x)",
                pos.symbol, pos.entry_price_usd, current_price, ratio);

            let sell_amount = pos.current_amount / 2.0;
            let sell_result = execute_position_sell(app_handle, &creds, &pos.mint, &pos.symbol, sell_amount).await;
            match sell_result {
                Ok(tx) => {
                    let remaining = pos.current_amount - sell_amount;
                    if remaining < 1.0 {
                        let _ = state.store.close_position(&pos.id, "closed_tp", Some(&tx));
                    } else {
                        let _ = state.store.reduce_position(&pos.id, remaining);
                        let _ = state.store.update_position_targets(&pos.id, 0.05, pos.take_profit_pct * 1.5);
                    }
                    let _ = state.store.insert_trade(
                        "sol_swap", Some("sell"), Some(&format!("{} → SOL", pos.symbol)),
                        Some(&pos.mint), &sell_amount.to_string(),
                        None, None, "completed", None, Some("SOL"),
                        &format!("Auto take-profit at {:.1}x", ratio),
                        None, None, Some(&tx),
                    );
                    app_handle.emit("position-closed", serde_json::json!({
                        "id": pos.id, "symbol": pos.symbol, "reason": "take_profit",
                        "entry_price": pos.entry_price_usd, "exit_price": current_price,
                    })).ok();
                    info!("[positions] ✅ Take-profit sell executed for {} — tx: {}", pos.symbol, &tx[..std::cmp::min(16, tx.len())]);
                }
                Err(e) => {
                    error!("[positions] ❌ Take-profit sell FAILED for {}: {}", pos.symbol, e);
                }
            }
        }
    }
}

/// Execute a sell of `amount` tokens of `mint` for SOL via the swap infrastructure.
async fn execute_position_sell(
    _app_handle: &tauri::AppHandle,
    creds: &HashMap<String, String>,
    mint: &str,
    symbol: &str,
    amount: f64,
) -> Result<String, String> {
    let args = serde_json::json!({
        "token_in": mint,
        "token_out": "SOL",
        "amount": format!("{}", amount as u64),
        "reason": format!("Auto position management for {}", symbol),
        "slippage_bps": 300
    });

    let result = sol_dex::execute_sol_swap(&args, creds).await?;

    if let Some(start) = result.find("solscan.io/tx/") {
        let after = &result[start + 14..];
        if let Some(end) = after.find(')') {
            return Ok(after[..end].to_string());
        }
    }

    Ok(format!("swap_ok_{}", chrono::Utc::now().timestamp()))
}

/// Background cron heartbeat — called every 60 seconds from the Tauri
/// setup hook. Checks open positions (SL/TP) and executes due cron tasks.
pub async fn run_cron_heartbeat(app_handle: &tauri::AppHandle) {
    let state = app_handle.state::<EngineState>();

    check_positions(app_handle).await;

    let due_tasks = match state.store.get_due_cron_tasks() {
        Ok(tasks) => tasks,
        Err(e) => {
            warn!("[heartbeat] Failed to query due cron tasks: {}", e);
            return;
        }
    };

    if due_tasks.is_empty() { return; }

    info!("[heartbeat] {} cron task(s) due", due_tasks.len());

    for task in due_tasks {
        let task_id = task.id.clone();
        let task_title = task.title.clone();

        let now = chrono::Utc::now();
        let next = compute_next_run(&task.cron_schedule, &now);
        if let Err(e) = state.store.update_task_cron_run(&task_id, &now.to_rfc3339(), next.as_deref()) {
            error!("[heartbeat] Failed to update cron timestamps for '{}': {}", task_title, e);
            continue;
        }

        let aid = uuid::Uuid::new_v4().to_string();
        state.store.add_task_activity(
            &aid, &task_id, "cron_triggered", None,
            &format!("Cron triggered: {}", task.cron_schedule.as_deref().unwrap_or("unknown")),
        ).ok();

        let app = app_handle.clone();
        tauri::async_runtime::spawn(async move {
            let st = app.state::<EngineState>();
            match execute_task(&app, &st, &task_id).await {
                Ok(run_id) => {
                    info!("[heartbeat] Cron task '{}' started, run_id={}", task_title, run_id);
                }
                Err(e) => {
                    error!("[heartbeat] Cron task '{}' failed to start: {}", task_title, e);
                    if let Ok(conn) = rusqlite::Connection::open(sessions::engine_db_path()) {
                        let aid = uuid::Uuid::new_v4().to_string();
                        conn.execute(
                            "INSERT INTO task_activity (id, task_id, kind, content) VALUES (?1, ?2, 'cron_error', ?3)",
                            rusqlite::params![aid, task_id, format!("Cron execution failed: {}", e)],
                        ).ok();
                    }
                }
            }
        });
    }

    app_handle.emit("cron-heartbeat", serde_json::json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
    })).ok();
}

// ── Schedule helpers ───────────────────────────────────────────────────

/// Simple schedule parser: "every Xm", "every Xh", "daily HH:MM"
fn compute_next_run(schedule: &Option<String>, from: &chrono::DateTime<chrono::Utc>) -> Option<String> {
    let s = schedule.as_deref()?;
    let s = s.trim().to_lowercase();

    if s.starts_with("every ") {
        let rest = s.strip_prefix("every ")?.trim();
        if rest.ends_with('m') {
            let mins: i64 = rest.trim_end_matches('m').trim().parse().ok()?;
            return Some((*from + chrono::Duration::minutes(mins)).to_rfc3339());
        } else if rest.ends_with('h') {
            let hours: i64 = rest.trim_end_matches('h').trim().parse().ok()?;
            return Some((*from + chrono::Duration::hours(hours)).to_rfc3339());
        }
    } else if s.starts_with("daily ") {
        let time_str = s.strip_prefix("daily ")?.trim();
        let parts: Vec<&str> = time_str.split(':').collect();
        if parts.len() == 2 {
            let hour: u32 = parts[0].parse().ok()?;
            let minute: u32 = parts[1].parse().ok()?;
            let today = from.date_naive();
            let target_time = today.and_hms_opt(hour, minute, 0)?;
            let target = target_time.and_utc();
            if target > *from {
                return Some(target.to_rfc3339());
            } else {
                let tomorrow = today.succ_opt()?;
                let next = tomorrow.and_hms_opt(hour, minute, 0)?.and_utc();
                return Some(next.to_rfc3339());
            }
        }
    }

    Some((*from + chrono::Duration::hours(1)).to_rfc3339())
}
