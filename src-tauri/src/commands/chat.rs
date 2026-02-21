// Paw Commands — Chat & Session System Layer
//
// Thin Tauri command wrappers for:
//   - Chat (engine_chat_send, engine_chat_history)
//   - Sessions (engine_sessions_list, _rename, _delete, _clear, _compact)
//   - Tool approval (engine_approve_tool)
//
// Heavy logic lives in crate::engine::chat (the organism).
// These functions: extract state → call organisms → return.

use tauri::{State, Emitter, Manager};
use log::{info, warn, error};

use crate::commands::state::{EngineState, normalize_model_name, resolve_provider_for_model};
use crate::engine::types::*;
use crate::engine::providers::AnyProvider;
use crate::engine::agent_loop;
use crate::engine::memory;
use crate::engine::chat as chat_org;

// ── Chat ─────────────────────────────────────────────────────────────────────

/// Send a chat message and run the agent loop.
/// Returns immediately with a run_id; results stream via `engine-event` Tauri events.
#[tauri::command]
pub async fn engine_chat_send(
    app_handle: tauri::AppHandle,
    state: State<'_, EngineState>,
    request: ChatRequest,
) -> Result<ChatResponse, String> {
    let run_id = uuid::Uuid::new_v4().to_string();

    // ── Resolve or create session ──────────────────────────────────────────
    let session_id = match &request.session_id {
        Some(id) if !id.is_empty() => id.clone(),
        _ => {
            let new_id = format!("eng-{}", uuid::Uuid::new_v4());
            let raw = request.model.clone().unwrap_or_default();
            let model = if raw.is_empty() || raw.eq_ignore_ascii_case("default") {
                let cfg = state.config.lock();
                cfg.default_model.clone().unwrap_or_else(|| "gpt-4o".to_string())
            } else {
                raw
            };
            state.store.create_session(
                &new_id,
                &model,
                request.system_prompt.as_deref(),
                request.agent_id.as_deref(),
            )?;
            new_id
        }
    };

    // ── Resolve model and provider ─────────────────────────────────────────
    let (provider_config, model) = {
        let cfg = state.config.lock();

        let raw_model = request.model.clone().unwrap_or_default();
        let base_model = if raw_model.is_empty() || raw_model.eq_ignore_ascii_case("default") {
            cfg.default_model.clone().unwrap_or_else(|| "gpt-4o".to_string())
        } else {
            raw_model
        };

        let user_explicitly_chose_model = request.model.as_ref()
            .map_or(false, |m| !m.is_empty() && !m.eq_ignore_ascii_case("default"));
        let (model, was_downgraded) = if !user_explicitly_chose_model {
            cfg.model_routing.resolve_auto_tier(&request.message, &base_model)
        } else {
            (base_model, false)
        };
        if was_downgraded {
            info!(
                "[engine] Auto-tier: simple task → using cheap model '{}' instead of default",
                model
            );
        }

        let model = normalize_model_name(&model).to_string();

        let provider = if let Some(pid) = &request.provider_id {
            cfg.providers.iter().find(|p| p.id == *pid).cloned()
        } else {
            resolve_provider_for_model(&model, &cfg.providers)
                .or_else(|| {
                    cfg.providers
                        .iter()
                        .find(|p| p.default_model.as_deref() == Some(model.as_str()))
                        .cloned()
                })
                .or_else(|| {
                    cfg.default_provider
                        .as_ref()
                        .and_then(|dp| cfg.providers.iter().find(|p| p.id == *dp).cloned())
                })
                .or_else(|| cfg.providers.first().cloned())
        };

        match provider {
            Some(p) => (p, model),
            None => {
                return Err("No AI provider configured. Go to Settings → Engine to add an API key.".into())
            }
        }
    };

    // ── Store the user message ─────────────────────────────────────────────
    let user_msg = StoredMessage {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.clone(),
        role: "user".into(),
        content: request.message.clone(),
        tool_calls_json: None,
        tool_call_id: None,
        name: None,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    state.store.add_message(&user_msg)?;

    // ── Base system prompt ─────────────────────────────────────────────────
    let base_system_prompt = request.system_prompt.clone().or_else(|| {
        let cfg = state.config.lock();
        cfg.default_system_prompt.clone()
    });

    // ── Soul context + today's memories ───────────────────────────────────
    let agent_id_owned = request.agent_id.clone().unwrap_or_else(|| "default".to_string());
    let core_context = state.store.compose_core_context(&agent_id_owned).unwrap_or(None);
    if let Some(ref cc) = core_context {
        info!(
            "[engine] Core soul context loaded ({} chars) for agent '{}'",
            cc.len(),
            agent_id_owned
        );
    } else {
        info!("[engine] No core soul files found for agent '{}'", agent_id_owned);
    }

    let todays_memories = state.store.get_todays_memories(&agent_id_owned).unwrap_or(None);
    if let Some(ref tm) = todays_memories {
        info!("[engine] Today's memory notes injected ({} chars)", tm.len());
    }

    // ── Auto-capture flag ──────────────────────────────────────────────────
    let auto_capture_on = state.memory_config.lock().auto_capture;

    // ── Skill instructions ─────────────────────────────────────────────────
    let skill_instructions =
        crate::engine::skills::get_enabled_skill_instructions(&state.store, &agent_id_owned).unwrap_or_default();
    if !skill_instructions.is_empty() {
        info!("[engine] Skill instructions injected ({} chars)", skill_instructions.len());
    }

    // ── Runtime context block (extracted values for organism) ─────────────
    let runtime_context = {
        let cfg = state.config.lock();
        let provider_name = cfg
            .providers
            .iter()
            .find(|p| Some(p.id.clone()) == cfg.default_provider)
            .or_else(|| cfg.providers.first())
            .map(|p| format!("{} ({:?})", p.id, p.kind))
            .unwrap_or_else(|| "unknown".into());
        let user_tz = cfg.user_timezone.clone();
        chat_org::build_runtime_context(&model, &provider_name, &session_id, &agent_id_owned, &user_tz)
    };

    // ── Compose full system prompt (organism) ──────────────────────────────
    let full_system_prompt = chat_org::compose_chat_system_prompt(
        base_system_prompt.as_deref(),
        runtime_context,
        core_context.as_deref(),
        todays_memories.as_deref(),
        &skill_instructions,
    );

    info!(
        "[engine] System prompt: {} chars (core_ctx={}, today_mem={}, skills={})",
        full_system_prompt.as_ref().map(|s| s.len()).unwrap_or(0),
        core_context.is_some(),
        todays_memories.is_some(),
        !skill_instructions.is_empty()
    );

    // ── Load conversation history ──────────────────────────────────────────
    let mut messages =
        state.store.load_conversation(&session_id, full_system_prompt.as_deref())?;

    // ── Process attachments into multi-modal blocks (organism) ────────────
    chat_org::process_attachments(&request.message, &request.attachments, &mut messages);

    // ── Build tool list (organism) ─────────────────────────────────────────
    let tools = chat_org::build_chat_tools(
        &state.store,
        request.tools_enabled.unwrap_or(true),
        request.tool_filter.as_deref(),
        &app_handle,
    );

    // ── Detect response loops (organism) ──────────────────────────────────
    chat_org::detect_response_loop(&mut messages);

    // ── Extract remaining config values ───────────────────────────────────
    let (max_rounds, temperature) = {
        let cfg = state.config.lock();
        (cfg.max_tool_rounds, request.temperature)
    };
    let tool_timeout = {
        let cfg = state.config.lock();
        cfg.tool_timeout_secs
    };
    let daily_budget = {
        let cfg = state.config.lock();
        cfg.daily_budget_usd
    };

    let session_id_clone = session_id.clone();
    let run_id_clone = run_id.clone();
    let approvals = state.pending_approvals.clone();
    let user_message_for_capture = request.message.clone();
    let pre_loop_msg_count = messages.len();
    let app = app_handle.clone();
    let agent_id_for_spawn = agent_id_owned.clone();
    let sem = state.run_semaphore.clone();
    let panic_session_id = session_id.clone();
    let panic_run_id = run_id.clone();
    let panic_app = app_handle.clone();
    let daily_tokens = state.daily_tokens.clone();
    let active_runs = state.active_runs.clone();
    let abort_session_id = session_id.clone();

    // ── Spawn agent loop ───────────────────────────────────────────────────
    let handle = tauri::async_runtime::spawn(async move {
        // Chat gets priority — short timeout then proceed anyway
        let _permit = match tokio::time::timeout(
            std::time::Duration::from_secs(2),
            sem.acquire_owned(),
        )
        .await
        {
            Ok(Ok(permit)) => Some(permit),
            _ => {
                info!("[engine] Chat bypassing concurrency limit (all slots busy)");
                None
            }
        };

        let provider = AnyProvider::from_config(&provider_config);

        match agent_loop::run_agent_turn(
            &app,
            &provider,
            &model,
            &mut messages,
            &tools,
            &session_id_clone,
            &run_id_clone,
            max_rounds,
            temperature,
            &approvals,
            tool_timeout,
            &agent_id_for_spawn,
            daily_budget,
            Some(&daily_tokens),
        )
        .await
        {
            Ok(final_text) => {
                info!("[engine] Agent turn complete: {} chars", final_text.len());

                if let Some(engine_state) = app.try_state::<EngineState>() {
                    // Persist only NEW messages (skip pre-loaded history)
                    for msg in messages.iter().skip(pre_loop_msg_count) {
                        if msg.role == Role::Assistant || msg.role == Role::Tool {
                            let stored = StoredMessage {
                                id: uuid::Uuid::new_v4().to_string(),
                                session_id: session_id_clone.clone(),
                                role: match msg.role {
                                    Role::Assistant => "assistant".into(),
                                    Role::Tool => "tool".into(),
                                    _ => "user".into(),
                                },
                                content: msg.content.as_text(),
                                tool_calls_json: msg
                                    .tool_calls
                                    .as_ref()
                                    .map(|tc| serde_json::to_string(tc).unwrap_or_default()),
                                tool_call_id: msg.tool_call_id.clone(),
                                name: msg.name.clone(),
                                created_at: chrono::Utc::now().to_rfc3339(),
                            };
                            if let Err(e) = engine_state.store.add_message(&stored) {
                                error!("[engine] Failed to store message: {}", e);
                            }
                        }
                    }

                    // Auto-capture memorable facts
                    if auto_capture_on && !final_text.is_empty() {
                        let facts = memory::extract_memorable_facts(
                            &user_message_for_capture,
                            &final_text,
                        );
                        if !facts.is_empty() {
                            let emb_client = engine_state.embedding_client();
                            for (content, category) in &facts {
                                match memory::store_memory(
                                    &engine_state.store,
                                    content,
                                    category,
                                    5,
                                    emb_client.as_ref(),
                                    Some(&agent_id_for_spawn),
                                )
                                .await
                                {
                                    Ok(id) => info!(
                                        "[engine] Auto-captured memory: {}",
                                        crate::engine::types::truncate_utf8(&id, 8)
                                    ),
                                    Err(e) => warn!("[engine] Auto-capture failed: {}", e),
                                }
                            }
                        }
                    }

                    // Session-end summary (powers "Today's Memory Notes" in future sessions)
                    let had_tool_calls = messages.iter().skip(pre_loop_msg_count).any(|m| {
                        m.role == Role::Tool
                            || m.tool_calls
                                .as_ref()
                                .map(|tc| !tc.is_empty())
                                .unwrap_or(false)
                    });
                    let is_substantial = final_text.len() > 200 || had_tool_calls;
                    if is_substantial && !final_text.is_empty() {
                        let summary = if final_text.len() > 300 {
                            format!("{}…", &final_text[..300])
                        } else {
                            final_text.clone()
                        };
                        let session_summary = format!(
                            "Session work: User asked: \"{}\". Agent responded: {}",
                            crate::engine::types::truncate_utf8(
                                &user_message_for_capture,
                                150
                            ),
                            summary,
                        );
                        let emb_client = engine_state.embedding_client();
                        match memory::store_memory(
                            &engine_state.store,
                            &session_summary,
                            "session",
                            3,
                            emb_client.as_ref(),
                            Some(&agent_id_for_spawn),
                        )
                        .await
                        {
                            Ok(_) => info!(
                                "[engine] Session summary stored ({} chars)",
                                session_summary.len()
                            ),
                            Err(e) => warn!("[engine] Session summary store failed: {}", e),
                        }
                    }
                }
            }
            Err(e) => {
                error!("[engine] Agent turn failed: {}", e);
                let _ = app.emit(
                    "engine-event",
                    EngineEvent::Error {
                        session_id: session_id_clone,
                        run_id: run_id_clone,
                        message: e.to_string(),
                    },
                );
            }
        }
    });

    // ── Register abort handle for this session ─────────────────────────────
    active_runs.lock().insert(abort_session_id.clone(), handle.inner().abort_handle());

    // ── Panic safety monitor + abort handle cleanup ────────────────────────
    let cleanup_runs = active_runs.clone();
    let cleanup_session_id = abort_session_id.clone();
    tauri::async_runtime::spawn(async move {
        let result = handle.await;
        // Always clean up the abort handle when the task finishes
        cleanup_runs.lock().remove(&cleanup_session_id);
        if let Err(ref err) = result {
            // Check if the error is a JoinError from cancellation
            let is_cancelled = matches!(err, tauri::Error::JoinError(je) if je.is_cancelled());
            if is_cancelled {
                info!("[engine] Agent task aborted by user for session {}", cleanup_session_id);
                let _ = panic_app.emit(
                    "engine-event",
                    EngineEvent::Complete {
                        session_id: panic_session_id,
                        run_id: panic_run_id,
                        text: String::new(),
                        tool_calls_count: 0,
                        usage: None,
                        model: None,
                    },
                );
            } else {
                let msg = format!("Internal error: agent task crashed — {}", err);
                error!("[engine] {}", msg);
                let _ = panic_app.emit(
                    "engine-event",
                    EngineEvent::Error {
                        session_id: panic_session_id,
                        run_id: panic_run_id,
                        message: msg,
                    },
                );
            }
        }
    });

    Ok(ChatResponse { run_id, session_id })
}

/// Get chat message history for a session.
#[tauri::command]
pub fn engine_chat_history(
    state: State<'_, EngineState>,
    session_id: String,
    limit: Option<i64>,
) -> Result<Vec<StoredMessage>, String> {
    state.store.get_messages(&session_id, limit.unwrap_or(200)).map_err(|e| e.to_string())
}

/// Abort an in-flight agent run for the given session.
#[tauri::command]
pub fn engine_chat_abort(
    state: State<'_, EngineState>,
    session_id: String,
) -> Result<(), String> {
    let mut runs = state.active_runs.lock();
    if let Some(handle) = runs.remove(&session_id) {
        handle.abort();
        info!("[engine] Aborted agent run for session {}", session_id);
        Ok(())
    } else {
        warn!("[engine] No active run found for session {} — may have already finished", session_id);
        Ok(()) // Not an error — the run may have completed between click and arrival
    }
}

// ── Sessions ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn engine_sessions_list(
    state: State<'_, EngineState>,
    limit: Option<i64>,
    agent_id: Option<String>,
) -> Result<Vec<Session>, String> {
    state
        .store
        .list_sessions_filtered(limit.unwrap_or(50), agent_id.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_session_rename(
    state: State<'_, EngineState>,
    session_id: String,
    label: String,
) -> Result<(), String> {
    state.store.rename_session(&session_id, &label).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_session_delete(
    state: State<'_, EngineState>,
    session_id: String,
) -> Result<(), String> {
    state.store.delete_session(&session_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_session_clear(
    state: State<'_, EngineState>,
    session_id: String,
) -> Result<(), String> {
    info!("[engine] Clearing messages for session {}", session_id);
    state.store.clear_messages(&session_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_session_cleanup(
    state: State<'_, EngineState>,
    max_age_secs: Option<i64>,
    exclude_id: Option<String>,
) -> Result<usize, String> {
    let age = max_age_secs.unwrap_or(3600); // default: 1 hour
    state.store.cleanup_empty_sessions(age, exclude_id.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn engine_session_compact(
    state: State<'_, EngineState>,
    session_id: String,
) -> Result<crate::engine::compaction::CompactionResult, String> {
    info!("[engine] Manual compaction requested for session {}", session_id);

    let (provider_config, model) = {
        let cfg = state.config.lock();
        let model = cfg
            .default_model
            .clone()
            .unwrap_or_else(|| "gpt-4o".to_string());
        let provider = cfg
            .default_provider
            .as_ref()
            .and_then(|dp| cfg.providers.iter().find(|p| p.id == *dp).cloned())
            .or_else(|| cfg.providers.first().cloned())
            .ok_or("No AI provider configured.")?;
        (provider, model)
    };

    let provider = crate::engine::providers::AnyProvider::from_config(&provider_config);
    let compact_config = crate::engine::compaction::CompactionConfig::default();
    let store_arc = std::sync::Arc::new(crate::engine::sessions::SessionStore::open().map_err(|e| e.to_string())?);

    crate::engine::compaction::compact_session(
        &store_arc,
        &provider,
        &model,
        &session_id,
        &compact_config,
    )
    .await
    .map_err(|e| e.to_string())
}

// ── Tool approval ─────────────────────────────────────────────────────────────

#[tauri::command]
pub fn engine_approve_tool(
    state: State<'_, EngineState>,
    tool_call_id: String,
    approved: bool,
) -> Result<(), String> {
    let mut map = state
        .pending_approvals
        .lock();

    if let Some(sender) = map.remove(&tool_call_id) {
        info!(
            "[engine] Tool approval resolved: {} → {}",
            tool_call_id,
            if approved { "ALLOWED" } else { "DENIED" }
        );
        let _ = sender.send(approved);
        Ok(())
    } else {
        warn!(
            "[engine] No pending approval found for tool_call_id={}",
            tool_call_id
        );
        Err(format!("No pending approval for {}", tool_call_id))
    }
}
