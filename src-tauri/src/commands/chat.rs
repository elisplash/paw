// Paw Commands — Chat & Session System Layer
//
// Thin Tauri command wrappers for:
//   - Chat (engine_chat_send, engine_chat_history)
//   - Sessions (engine_sessions_list, _rename, _delete, _clear, _compact)
//   - Tool approval (engine_approve_tool)
//
// Heavy logic lives in crate::engine::chat (the organism).
// These functions: extract state → call organisms → return.

use log::{error, info, warn};
use tauri::{Emitter, Manager, State};

use crate::commands::state::{normalize_model_name, resolve_provider_for_model, EngineState};
use crate::engine::agent_loop;
use crate::engine::chat as chat_org;
use crate::engine::memory;
use crate::engine::providers::AnyProvider;
use crate::engine::types::*;

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

    // Reset swarm counters so sub-agents can wake fresh for this human turn
    crate::engine::swarm::reset_all_counters();

    // ── Resolve or create session ──────────────────────────────────────────
    let session_id = match &request.session_id {
        Some(id) if !id.is_empty() => id.clone(),
        _ => {
            let new_id = format!("eng-{}", uuid::Uuid::new_v4());
            let raw = request.model.clone().unwrap_or_default();
            let model = if raw.is_empty() || raw.eq_ignore_ascii_case("default") {
                let cfg = state.config.lock();
                cfg.default_model
                    .clone()
                    .unwrap_or_else(|| "gpt-4o".to_string())
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

    // ── Request queue: if a run is already active for this session, queue ──
    // VS Code pattern: instead of rejecting "Request already in progress",
    // queue the message and signal the active agent to wrap up.
    {
        let has_active_run = state.active_runs.lock().contains_key(&session_id);
        if has_active_run {
            info!(
                "[engine] Session {} has active run — queuing request and signaling yield",
                session_id
            );

            // Signal the active agent to wrap up
            if let Some(signal) = state.yield_signals.lock().get(&session_id) {
                signal.request_yield();
            }

            // Queue this request for processing after the current one completes
            // We need the resolved model/provider, so resolve them now
            let (queued_provider, queued_model) = {
                let cfg = state.config.lock();
                let raw = request.model.clone().unwrap_or_default();
                let m = if raw.is_empty() || raw.eq_ignore_ascii_case("default") {
                    cfg.default_model
                        .clone()
                        .unwrap_or_else(|| "gpt-4o".to_string())
                } else {
                    normalize_model_name(&raw).to_string()
                };
                let p = resolve_provider_for_model(&m, &cfg.providers)
                    .or_else(|| {
                        cfg.default_provider
                            .as_ref()
                            .and_then(|dp| cfg.providers.iter().find(|p| p.id == *dp).cloned())
                    })
                    .or_else(|| cfg.providers.first().cloned());
                match p {
                    Some(provider) => (provider, m),
                    None => return Err("No AI provider configured.".into()),
                }
            };

            let queued = crate::engine::state::QueuedRequest {
                request: request.clone(),
                provider_config: queued_provider,
                model: queued_model,
                system_prompt: request.system_prompt.clone(),
            };
            state
                .request_queue
                .lock()
                .entry(session_id.clone())
                .or_default()
                .push(queued);

            // Return immediately — the queue processor will handle this request
            return Ok(ChatResponse {
                run_id: format!("queued-{}", uuid::Uuid::new_v4()),
                session_id,
            });
        }
    }

    // ── Resolve model and provider ─────────────────────────────────────────
    let (provider_config, model) = {
        let cfg = state.config.lock();

        let raw_model = request.model.clone().unwrap_or_default();
        let base_model = if raw_model.is_empty() || raw_model.eq_ignore_ascii_case("default") {
            cfg.default_model
                .clone()
                .unwrap_or_else(|| "gpt-4o".to_string())
        } else {
            raw_model
        };

        let user_explicitly_chose_model = request
            .model
            .as_ref()
            .is_some_and(|m| !m.is_empty() && !m.eq_ignore_ascii_case("default"));
        let (model, was_downgraded) = if !user_explicitly_chose_model {
            cfg.model_routing
                .resolve_auto_tier(&request.message, &base_model)
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
                return Err(
                    "No AI provider configured. Go to Settings → Engine to add an API key.".into(),
                )
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
    let agent_id_owned = request
        .agent_id
        .clone()
        .unwrap_or_else(|| "default".to_string());
    let core_context = state
        .store
        .compose_core_context(&agent_id_owned)
        .unwrap_or(None);
    if let Some(ref cc) = core_context {
        info!(
            "[engine] Core soul context loaded ({} chars) for agent '{}'",
            cc.len(),
            agent_id_owned
        );
    } else {
        info!(
            "[engine] No core soul files found for agent '{}'",
            agent_id_owned
        );
    }

    let (todays_memories, todays_memory_contents) = {
        let tm = state
            .store
            .get_todays_memories(&agent_id_owned)
            .unwrap_or(None);
        let contents = state
            .store
            .get_todays_memory_contents(&agent_id_owned)
            .unwrap_or_default();
        (tm, contents)
    };
    if let Some(ref tm) = todays_memories {
        info!(
            "[engine] Today's memory notes injected ({} chars, {} entries)",
            tm.len(),
            todays_memory_contents.len()
        );
    }

    // ── Auto-capture flag ──────────────────────────────────────────────────
    let auto_capture_on = state.memory_config.lock().auto_capture;

    // ── Skill instructions ─────────────────────────────────────────────────
    let skill_instructions =
        crate::engine::skills::get_enabled_skill_instructions(&state.store, &agent_id_owned)
            .unwrap_or_default();
    if !skill_instructions.is_empty() {
        info!(
            "[engine] Skill instructions injected ({} chars)",
            skill_instructions.len()
        );
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
        chat_org::build_runtime_context(
            &model,
            &provider_name,
            &session_id,
            &agent_id_owned,
            &user_tz,
        )
    };

    // ── Compose full system prompt (organism) ──────────────────────────────
    let mut full_system_prompt = chat_org::compose_chat_system_prompt(
        base_system_prompt.as_deref(),
        runtime_context,
        core_context.as_deref(),
        todays_memories.as_deref(),
        &skill_instructions,
    );

    // ── Agent roster: inject known agents so delegation works ───────────
    if let Some(roster) = chat_org::build_agent_roster(&state.store, &agent_id_owned) {
        if let Some(ref mut p) = full_system_prompt {
            p.push_str("\n\n---\n\n");
            p.push_str(&roster);
        }
        info!("[engine] Agent roster injected ({} chars)", roster.len());
    }

    // ── Auto-recall: search memories relevant to this message ──────────
    // Same as channel agents — inject relevant long-term memories into
    // the system prompt so the agent doesn't "forget" cross-session context.
    // Dedup: skip memories already present in today's memory notes to avoid
    // double-injection which causes the model to loop on the same content.
    {
        let (auto_recall_on, recall_limit, recall_threshold) = {
            let mcfg = state.memory_config.lock();
            (mcfg.auto_recall, mcfg.recall_limit, mcfg.recall_threshold)
        };
        if auto_recall_on {
            let emb_client = state.embedding_client();
            match memory::search_memories(
                &state.store,
                &request.message,
                recall_limit,
                recall_threshold,
                emb_client.as_ref(),
                Some(&agent_id_owned),
            )
            .await
            {
                Ok(mems) if !mems.is_empty() => {
                    // Filter out memories already in today's notes (dedup)
                    let deduped: Vec<&Memory> = mems
                        .iter()
                        .filter(|m| {
                            !todays_memory_contents.iter().any(|tc| {
                                memory::content_overlap(&m.content, tc)
                                    > memory::DEDUP_OVERLAP_THRESHOLD
                            })
                        })
                        .collect();
                    if !deduped.is_empty() {
                        let ctx: Vec<String> = deduped
                            .iter()
                            .map(|m| format!("- [{}] {}", m.category, m.content))
                            .collect();
                        let memory_block = format!("## Relevant Memories\n{}", ctx.join("\n"));
                        if let Some(ref mut p) = full_system_prompt {
                            p.push_str("\n\n---\n\n");
                            p.push_str(&memory_block);
                        }
                        info!("[engine] Auto-recalled {} memories ({} deduped from today's, {} chars)",
                            mems.len(), mems.len() - deduped.len(), memory_block.len());
                    } else {
                        info!("[engine] Auto-recall: all {} results already in today's notes, skipping", mems.len());
                    }
                }
                Ok(_) => {} // No relevant memories found
                Err(e) => warn!("[engine] Auto-recall search failed: {}", e),
            }
        }
    }

    info!(
        "[engine] System prompt: {} chars (core_ctx={}, today_mem={}, skills={})",
        full_system_prompt.as_ref().map(|s| s.len()).unwrap_or(0),
        core_context.is_some(),
        todays_memories.is_some(),
        !skill_instructions.is_empty()
    );

    // ── Load conversation history ──────────────────────────────────────────
    let context_window = {
        let cfg = state.config.lock();
        cfg.context_window_tokens
    };
    let mut messages = state.store.load_conversation(
        &session_id,
        full_system_prompt.as_deref(),
        Some(context_window),
        Some(&agent_id_owned),
    )?;

    // ── Process attachments into multi-modal blocks (organism) ────────────
    chat_org::process_attachments(&request.message, &request.attachments, &mut messages);

    // ── Clear loaded tools for this new chat turn ─────────────────────────
    // Tool RAG: reset the set of dynamically-loaded tools so each turn starts fresh.
    state.loaded_tools.lock().clear();

    // ── Build tool list (organism) — Tool RAG: core tools + previously loaded ─
    let loaded_tools = state.loaded_tools.lock().clone();
    let mut tools = chat_org::build_chat_tools(
        &state.store,
        request.tools_enabled.unwrap_or(true),
        request.tool_filter.as_deref(),
        &app_handle,
        &loaded_tools,
    );

    // ── Foreman Protocol: inject MCP bridge awareness when MCP tools exist ──
    {
        let has_mcp_tools = tools.iter().any(|t| t.function.name.starts_with("mcp_"));
        if has_mcp_tools {
            if let Some(ref mut p) = full_system_prompt {
                p.push_str("\n\n---\n\n");
                p.push_str(chat_org::build_foreman_awareness());
            }
            info!("[engine] Foreman Protocol awareness injected (MCP tools present)");
        }
    }

    // ── Detect response loops (organism) ──────────────────────────────────
    chat_org::detect_response_loop(&mut messages);

    // Note: Topic detection and retry-override injection have been removed.
    // VS Code pattern: failed messages are deleted from history entirely
    // (in load_conversation → delete_failed_exchanges), so the model never
    // sees past failures and doesn't need prompt-engineering nudges.
    // Users can start a new session for topic changes (Ctrl+L / New Chat).

    // ── Extract remaining config values ───────────────────────────────────
    let (max_rounds, temperature) = {
        let cfg = state.config.lock();
        (cfg.max_tool_rounds, request.temperature)
    };
    let thinking_level = request.thinking_level.clone();
    let auto_approve_all = request.auto_approve_all;
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

    // ── Set up yield signal for this session (VS Code pattern) ────────────
    let yield_signal = {
        let mut signals = state.yield_signals.lock();
        let signal = signals.entry(session_id.clone()).or_default().clone();
        signal.reset(); // Fresh start for this request
        signal
    };
    let yield_signal_for_spawn = yield_signal.clone();
    let request_queue = state.request_queue.clone();
    let yield_signals_cleanup = state.yield_signals.clone();

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
            &mut tools,
            &session_id_clone,
            &run_id_clone,
            max_rounds,
            temperature,
            &approvals,
            tool_timeout,
            &agent_id_for_spawn,
            daily_budget,
            Some(&daily_tokens),
            thinking_level.as_deref(),
            auto_approve_all,
            Some(&yield_signal_for_spawn),
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

                    // Auto-capture memorable facts (with dedup guard)
                    if auto_capture_on && !final_text.is_empty() {
                        let facts =
                            memory::extract_memorable_facts(&user_message_for_capture, &final_text);
                        if !facts.is_empty() {
                            let emb_client = engine_state.embedding_client();
                            for (content, category) in &facts {
                                match memory::store_memory_dedup(
                                    &engine_state.store,
                                    content,
                                    category,
                                    5,
                                    emb_client.as_ref(),
                                    Some(&agent_id_for_spawn),
                                )
                                .await
                                {
                                    Ok(Some(id)) => info!(
                                        "[engine] Auto-captured memory: {}",
                                        crate::engine::types::truncate_utf8(&id, 8)
                                    ),
                                    Ok(None) => {
                                        info!("[engine] Auto-capture skipped (near-duplicate)")
                                    }
                                    Err(e) => warn!("[engine] Auto-capture failed: {}", e),
                                }
                            }
                        }
                    }

                    // Session-end summary (powers "Today's Memory Notes" in future sessions)
                    // Only store when actual tool work was done — plain chat responses
                    // are not worth memorizing and cause memory bloat.
                    // Rate-limit: skip if a session summary was stored in the last 5 minutes
                    // to prevent memory accumulation loops during rapid context switches.
                    let had_tool_calls = messages.iter().skip(pre_loop_msg_count).any(|m| {
                        m.role == Role::Tool
                            || m.tool_calls
                                .as_ref()
                                .map(|tc| !tc.is_empty())
                                .unwrap_or(false)
                    });
                    if had_tool_calls && !final_text.is_empty() {
                        let summary = if final_text.len() > 300 {
                            format!("{}…", &final_text[..final_text.floor_char_boundary(300)])
                        } else {
                            final_text.clone()
                        };
                        let session_summary = format!(
                            "Session work: User asked: \"{}\". Agent responded: {}",
                            crate::engine::types::truncate_utf8(&user_message_for_capture, 150),
                            summary,
                        );
                        let emb_client = engine_state.embedding_client();
                        match memory::store_memory_dedup(
                            &engine_state.store,
                            &session_summary,
                            "session",
                            3,
                            emb_client.as_ref(),
                            Some(&agent_id_for_spawn),
                        )
                        .await
                        {
                            Ok(Some(id)) => info!(
                                "[engine] Session summary stored ({} chars, id={})",
                                session_summary.len(),
                                &id[..id.len().min(8)]
                            ),
                            Ok(None) => info!("[engine] Session summary skipped (near-duplicate)"),
                            Err(e) => warn!("[engine] Session summary store failed: {}", e),
                        }
                    }

                    // ── Auto-prune: cap stored messages per session ──
                    {
                        use crate::atoms::constants::CHAT_SESSION_MAX_MESSAGES;
                        match engine_state
                            .store
                            .prune_session_messages(&session_id_clone, CHAT_SESSION_MAX_MESSAGES)
                        {
                            Ok(pruned) if pruned > 0 => {
                                info!(
                                    "[engine] Pruned {} old messages from session {} (cap={})",
                                    pruned, session_id_clone, CHAT_SESSION_MAX_MESSAGES
                                );
                            }
                            Err(e) => warn!("[engine] Session prune failed: {}", e),
                            _ => {}
                        }
                    }

                    // ── Auto-compact: DISABLED ──
                    // Auto-compaction replaces conversation history with a summary,
                    // which biases the model towards the old topic and prevents
                    // natural topic shifts. Manual compaction is still available
                    // via the session_compact command if a user explicitly wants it.
                    //
                    // Instead, we rely on:
                    //   - Auto-prune (above) to cap stored messages
                    //   - Mid-loop truncation (in agent_loop) to cap context window
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
    active_runs
        .lock()
        .insert(abort_session_id.clone(), handle.inner().abort_handle());

    // ── Panic safety monitor + abort handle cleanup + queue processing ───
    let cleanup_runs = active_runs.clone();
    let cleanup_session_id = abort_session_id.clone();
    let queue_session_id = abort_session_id.clone();
    let queue_ref = request_queue.clone();
    let queue_app = app_handle.clone();
    let yield_cleanup_session = abort_session_id.clone();
    tauri::async_runtime::spawn(async move {
        let result = handle.await;
        // Always clean up the abort handle and yield signal when the task finishes
        cleanup_runs.lock().remove(&cleanup_session_id);
        yield_signals_cleanup.lock().remove(&yield_cleanup_session);

        // ── Process next queued request (VS Code pattern) ─────────────
        // After the current request completes, check if there are queued
        // messages and process the next one.
        {
            let next = queue_ref.lock().get_mut(&queue_session_id).and_then(|q| {
                if q.is_empty() {
                    None
                } else {
                    Some(q.remove(0))
                }
            });
            if let Some(queued) = next {
                info!(
                    "[engine] Processing queued request for session {}",
                    queue_session_id
                );
                // Re-send via the Tauri command system
                if let Some(engine_state) = queue_app.try_state::<EngineState>() {
                    // Store the queued user message
                    let user_msg = StoredMessage {
                        id: uuid::Uuid::new_v4().to_string(),
                        session_id: queue_session_id.clone(),
                        role: "user".into(),
                        content: queued.request.message.clone(),
                        tool_calls_json: None,
                        tool_call_id: None,
                        name: None,
                        created_at: chrono::Utc::now().to_rfc3339(),
                    };
                    let _ = engine_state.store.add_message(&user_msg);
                    // Emit a queue-processing event so frontend knows
                    let _ = queue_app.emit(
                        "engine-event",
                        EngineEvent::Delta {
                            session_id: queue_session_id.clone(),
                            run_id: format!("queue-{}", uuid::Uuid::new_v4()),
                            text: String::new(),
                        },
                    );
                    info!(
                        "[engine] Queued request stored, frontend should re-send via normal flow"
                    );
                }
            }
        }

        if let Err(ref err) = result {
            // Check if the error is a JoinError from cancellation
            let is_cancelled = matches!(err, tauri::Error::JoinError(je) if je.is_cancelled());
            if is_cancelled {
                info!(
                    "[engine] Agent task aborted by user for session {}",
                    cleanup_session_id
                );
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
    state
        .store
        .get_messages(&session_id, limit.unwrap_or(200))
        .map_err(|e| e.to_string())
}

/// Abort an in-flight agent run for the given session.
#[tauri::command]
pub fn engine_chat_abort(state: State<'_, EngineState>, session_id: String) -> Result<(), String> {
    let mut runs = state.active_runs.lock();
    if let Some(handle) = runs.remove(&session_id) {
        handle.abort();
        info!("[engine] Aborted agent run for session {}", session_id);
        Ok(())
    } else {
        warn!(
            "[engine] No active run found for session {} — may have already finished",
            session_id
        );
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
    state
        .store
        .rename_session(&session_id, &label)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_session_delete(
    state: State<'_, EngineState>,
    session_id: String,
) -> Result<(), String> {
    state
        .store
        .delete_session(&session_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_session_clear(
    state: State<'_, EngineState>,
    session_id: String,
) -> Result<(), String> {
    info!("[engine] Clearing messages for session {}", session_id);
    state
        .store
        .clear_messages(&session_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_session_cleanup(
    state: State<'_, EngineState>,
    max_age_secs: Option<i64>,
    exclude_id: Option<String>,
) -> Result<usize, String> {
    let age = max_age_secs.unwrap_or(3600); // default: 1 hour
    state
        .store
        .cleanup_empty_sessions(age, exclude_id.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn engine_session_compact(
    state: State<'_, EngineState>,
    session_id: String,
) -> Result<crate::engine::compaction::CompactionResult, String> {
    info!(
        "[engine] Manual compaction requested for session {}",
        session_id
    );

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
    let store_arc = std::sync::Arc::new(
        crate::engine::sessions::SessionStore::open().map_err(|e| e.to_string())?,
    );

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
    let mut map = state.pending_approvals.lock();

    if let Some(sender) = map.remove(&tool_call_id) {
        info!(
            "[engine] Tool approval resolved: {} → {}",
            tool_call_id,
            if approved { "ALLOWED" } else { "DENIED" }
        );
        let _ = sender.send(approved);
        Ok(())
    } else {
        // Stale approval — the backend already timed out or the tool call
        // completed before the frontend resolved it.  This is normal when
        // session overrides fire after a timeout.  Silently accept it.
        info!(
            "[engine] Stale approval (already resolved/timed-out): tool_call_id={}",
            tool_call_id
        );
        Ok(())
    }
}
