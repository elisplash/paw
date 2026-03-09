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
use crate::engine::engram;
use crate::engine::memory;
use crate::engine::providers::AnyProvider;
use crate::engine::types::*;
use crate::engine::util::safe_truncate;

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
        Some(id) if !id.is_empty() => {
            // Ensure session exists in the database (callers may pass custom IDs)
            if state.store.get_session(id)?.is_none() {
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
                    id,
                    &model,
                    request.system_prompt.as_deref(),
                    request.agent_id.as_deref(),
                )?;
            }
            id.clone()
        }
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

    let (todays_memories, _todays_memory_contents) = {
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
            _todays_memory_contents.len()
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

    // ── Compose system prompt + recall + history via Engram ContextBuilder ──
    // The ContextBuilder uses accurate token counting via the model capability
    // registry, budget-aware assembly (priority-ordered section dropping),
    // and BM25+vector+graph fusion for auto-recall. This replaces the old
    // compose_chat_system_prompt → budget trimming → load_conversation pipeline.
    let agent_roster = chat_org::build_agent_roster(&state.store, &agent_id_owned);

    let auto_recall_on = {
        let mcfg = state.memory_config.lock();
        mcfg.auto_recall
    };

    let context_window_override = {
        let cfg = state.config.lock();
        cfg.context_window_tokens
    };

    // Load raw conversation history for the ContextBuilder to budget-trim
    let raw_messages = state
        .store
        .load_conversation_raw(&session_id, Some(&agent_id_owned))
        .unwrap_or_default();

    let history_pairs: Vec<(String, String)> = raw_messages
        .iter()
        .map(|m| (m.role.clone(), m.content.clone()))
        .collect();

    let emb_client_for_recall = state.embedding_client();
    let recall_scope = crate::atoms::engram_types::MemoryScope {
        global: false,
        agent_id: Some(agent_id_owned.clone()),
        ..Default::default()
    };

    // ── Activate Cognitive State (Engram three-tier pipeline) ────────────
    // Get or create the per-agent CognitiveState. This holds the sensory
    // buffer (Tier 0) and working memory (Tier 1). Tier 2 is SessionStore.
    // Uses Arc<tokio::sync::Mutex> per agent — safe to hold across .await.
    let cognitive_lock = state.get_cognitive_state(&agent_id_owned);
    let mut cognitive = cognitive_lock.lock().await;

    // Decay working memory priorities each turn (0.95× factor)
    cognitive.decay_turn();

    // §8.2 Adapt WM budget to the actual model being used this turn.
    // The CognitiveState may have been created with the default model's budget,
    // but auto-tier routing or explicit model selection can change the model.
    cognitive.adapt_wm_budget(&model);

    let mut builder = engram::context_builder::ContextBuilder::new(&model)
        .context_window(context_window_override);

    // ── Inject platform awareness + foreman protocol (priority 0 — never dropped)
    // These were missing from the ContextBuilder path, causing the agent to lose
    // self-awareness of what OpenPawz is and what tools/capabilities it has.
    builder = builder.platform_awareness(chat_org::build_platform_awareness());
    builder = builder.foreman_protocol(chat_org::build_foreman_awareness().to_string());

    if let Some(ref bp) = base_system_prompt {
        builder = builder.base_prompt(bp.clone());
    }
    builder = builder.runtime_context(runtime_context);
    if let Some(ref cc) = core_context {
        builder = builder.core_context(cc.clone());
    }
    if let Some(ref tm) = todays_memories {
        builder = builder.todays_memories(tm.clone());
    }
    if !skill_instructions.is_empty() {
        builder = builder.skill_instructions(skill_instructions.clone());
    }
    if let Some(ref roster) = agent_roster {
        builder = builder.agent_roster(roster.clone());
    }
    if auto_recall_on {
        builder = builder.recall_from(
            &state.store,
            emb_client_for_recall.as_ref(),
            recall_scope,
            request.message.clone(),
        );
        builder = builder.hnsw_index(&state.hnsw_index);
    }
    // Wire working memory into the ContextBuilder (Tier 1 → prompt assembly)
    builder = builder.working_memory(&cognitive.working_memory);
    builder = builder.messages(history_pairs);

    // Build the assembled context
    let assembled = builder.build().await;

    let (_full_system_prompt, mut messages, _budget_report) = match assembled {
        Ok(ctx) => {
            info!(
                "[engram:chat] Context assembled: sys={}tok hist={}tok reply={}tok mem={} msgs={}/{}",
                ctx.budget.system_prompt_tokens,
                ctx.budget.history_tokens,
                ctx.budget.available_for_reply,
                ctx.budget.memories_injected,
                ctx.budget.messages_included,
                ctx.budget.messages_included + ctx.budget.messages_trimmed,
            );

            // §8.6 Trajectory-aware recall: push the raw query embedding into
            // the momentum vector. On subsequent turns, graph::search will blend
            // the new query with this history to bias recall toward conversation
            // direction — critical for anaphoric queries ("deploy it", "fix that").
            if let Some(emb) = ctx.query_embedding {
                cognitive.working_memory.push_momentum(emb);
            }

            // Prepend system prompt as a system message, then add history
            let mut chat_messages: Vec<Message> = Vec::new();
            if let Some(ref sys) = ctx.system_prompt {
                chat_messages.push(Message {
                    role: Role::System,
                    content: MessageContent::Text(sys.clone()),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                });
            }
            // Convert (role, content) pairs to Message for the agent loop.
            // Cross-reference against raw_messages to restore tool_calls and
            // tool_call_id metadata that the ContextBuilder's (String, String)
            // pairs lost. Without this, the OpenAI API rejects the request
            // with "messages with role 'tool' must be a response to a
            // preceding message with 'tool_calls'" once prior tool exchanges
            // survive context trimming.
            let mut raw_cursor = 0; // track position in raw_messages for matching
            for (role, content) in &ctx.messages {
                // Try to find the matching raw StoredMessage so we can
                // restore tool_calls_json / tool_call_id / name.
                let mut tool_calls: Option<Vec<ToolCall>> = None;
                let mut tool_call_id: Option<String> = None;
                let mut msg_name: Option<String> = None;

                // Linear scan from raw_cursor — messages appear in the same
                // order, but trim_history may have dropped some from the front.
                for (i, rm) in raw_messages.iter().enumerate().skip(raw_cursor) {
                    if rm.role == *role && rm.content == *content {
                        tool_calls = rm
                            .tool_calls_json
                            .as_ref()
                            .and_then(|json| serde_json::from_str(json).ok());
                        tool_call_id = rm.tool_call_id.clone();
                        msg_name = rm.name.clone();
                        raw_cursor = i + 1;
                        break;
                    }
                }

                chat_messages.push(Message {
                    role: match role.as_str() {
                        "assistant" => Role::Assistant,
                        "system" => Role::System,
                        "tool" => Role::Tool,
                        _ => Role::User,
                    },
                    content: MessageContent::Text(content.clone()),
                    tool_calls,
                    tool_call_id,
                    name: msg_name,
                });
            }

            // Sanitize after reconstruction: context trimming may have
            // dropped assistant messages while keeping their tool results
            // (or vice versa). This prevents 400 errors from the API.
            crate::engine::agent_loop::helpers::sanitize_tool_pairs(&mut chat_messages);

            (ctx.system_prompt, chat_messages, ctx.budget)
        }
        Err(e) => {
            // Fallback to legacy system prompt composition if ContextBuilder fails
            warn!(
                "[engram:chat] ContextBuilder failed ({}), falling back to legacy path",
                e
            );
            let mut fallback_prompt = chat_org::compose_chat_system_prompt(
                base_system_prompt.as_deref(),
                {
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
                },
                core_context.as_deref(),
                todays_memories.as_deref(),
                &skill_instructions,
            );
            if let Some(ref roster) = agent_roster {
                if let Some(ref mut p) = fallback_prompt {
                    p.push_str("\n\n---\n\n");
                    p.push_str(roster);
                }
            }
            let context_window = context_window_override;
            let fallback_msgs = state.store.load_conversation(
                &session_id,
                fallback_prompt.as_deref(),
                Some(context_window),
                Some(&agent_id_owned),
            )?;
            let budget = engram::context_builder::BudgetReport::default();
            (fallback_prompt, fallback_msgs, budget)
        }
    };

    // ── Process attachments into multi-modal blocks (organism) ────────────
    chat_org::process_attachments(&request.message, &request.attachments, &mut messages);

    // Release cognitive state lock after ContextBuilder is done borrowing it.
    // The spawn closure will re-acquire it when the response is ready.
    drop(cognitive);

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

    // ── Detect response loops (organism) ──────────────────────────────────
    chat_org::detect_response_loop(&mut messages);

    // ── Detect explicit user overrides (§59.2) ────────────────────────────
    // If the user explicitly tells the agent to stop, refocus, etc., inject
    // a strong system redirect and clear working memory momentum.
    if chat_org::detect_user_override(&mut messages) {
        let cognitive_lock = state.get_cognitive_state(&agent_id_owned);
        let mut cog = cognitive_lock.lock().await;
        cog.working_memory.clear_momentum();
        log::info!(
            "[engine] User override detected in chat — momentum cleared for '{}'",
            agent_id_owned
        );
    }
    // ── Detect implicit topic shifts ──────────────────────────────────────
    // After a tool-heavy exchange, if the user sends a short message that
    // doesn't look like a task continuation ("option 1", "yes", "go ahead"),
    // inject a gentle nudge and clear momentum so recall isn't biased.
    else if chat_org::detect_implicit_topic_shift(&mut messages) {
        let cognitive_lock = state.get_cognitive_state(&agent_id_owned);
        let mut cog = cognitive_lock.lock().await;
        cog.working_memory.clear_momentum();
        cog.working_memory.clear();

        // Strip today's memory notes from the system prompt — they anchor
        // the model to the old topic even after tool messages are stripped.
        if let Some(sys_msg) = messages.first_mut() {
            if sys_msg.role == Role::System {
                if let MessageContent::Text(ref mut text) = sys_msg.content {
                    if let Some(start) = text.find("## Today's Memory Notes") {
                        // Find the next section boundary (\n## or end)
                        let rest = &text[start..];
                        let end = rest[1..]
                            .find("\n## ")
                            .map(|i| start + 1 + i)
                            .unwrap_or(text.len());
                        text.replace_range(start..end, "");
                        log::info!("[engine] Stripped today's memory notes from system prompt (topic shift)");
                    }
                }
            }
        }
    }

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
    let user_approved_tools = request.user_approved_tools.clone();
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
            &user_approved_tools,
            Some(&yield_signal_for_spawn),
        )
        .await
        {
            Ok(final_text) => {
                info!("[engine] Agent turn complete: {} chars", final_text.len());

                if let Some(engine_state) = app.try_state::<EngineState>() {
                    // Persist only NEW messages (skip pre-loaded history)
                    // Skip empty assistant messages — they waste context and
                    // cause the model to mimic the empty-response pattern.
                    for msg in messages.iter().skip(pre_loop_msg_count) {
                        if msg.role == Role::Assistant || msg.role == Role::Tool {
                            // Don't persist empty assistant messages that have
                            // no tool_calls. Assistant messages WITH tool_calls
                            // must always be persisted even if their text is
                            // empty — otherwise tool result messages become
                            // orphans and corrupt the conversation history.
                            if msg.role == Role::Assistant
                                && msg.tool_calls.as_ref().is_none_or(|tc| tc.is_empty())
                            {
                                let text = msg.content.as_text();
                                if text.trim().is_empty() {
                                    info!("[engine] Skipping empty assistant message (not persisting)");
                                    continue;
                                }
                            }
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

                    // ── Push message pair into sensory buffer (Tier 0) ──
                    // This feeds the three-tier cognitive pipeline so that
                    // recent exchanges are available in working memory.
                    if !final_text.is_empty() {
                        let cognitive_lock = engine_state.get_cognitive_state(&agent_id_for_spawn);
                        let mut cognitive = cognitive_lock.lock().await;
                        let wm_evictions =
                            cognitive.push_message(&user_message_for_capture, &final_text);
                        if wm_evictions > 0 {
                            info!(
                                "[engine] Sensory buffer push: {} WM evictions for agent '{}'",
                                wm_evictions, agent_id_for_spawn
                            );
                        }
                    }

                    // Auto-capture memorable facts via Engram (with dedup guard)
                    // Uses LLM-powered extraction for 5x better fact coverage.
                    // Falls back to heuristic extraction if LLM call fails.
                    if auto_capture_on && !final_text.is_empty() {
                        let extraction_provider = AnyProvider::from_config(&provider_config);
                        let facts = memory::extract_memorable_facts_llm(
                            &user_message_for_capture,
                            &final_text,
                            &extraction_provider,
                            &model,
                        )
                        .await;
                        if !facts.is_empty() {
                            let emb_client = engine_state.embedding_client();
                            for (content, category) in &facts {
                                // Store in Engram (three-tier episodic memory)
                                match engram::bridge::store_auto_capture(
                                    &engine_state.store,
                                    content,
                                    category,
                                    emb_client.as_ref(),
                                    Some(&agent_id_for_spawn),
                                    Some(&session_id_clone),
                                    None, // no channel context
                                    None, // no channel user
                                    Some(&engine_state.hnsw_index),
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
                                    Err(e) => warn!("[engine] Engram auto-capture failed: {}", e),
                                }
                            }
                        }
                    }

                    // Session-end summary (powers "Today's Memory Notes" in future sessions)
                    // Uses LLM to generate a concise summary instead of truncating.
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
                        // Generate a concise LLM summary instead of naive truncation
                        let session_summary = memory::generate_session_summary(
                            &user_message_for_capture,
                            &final_text,
                            &AnyProvider::from_config(&provider_config),
                            &model,
                        )
                        .await;
                        let emb_client = engine_state.embedding_client();

                        // Store session summary in Engram only (no legacy dual-write)
                        match engram::bridge::store_auto_capture(
                            &engine_state.store,
                            &session_summary,
                            "session",
                            emb_client.as_ref(),
                            Some(&agent_id_for_spawn),
                            Some(&session_id_clone),
                            None, // no channel context
                            None, // no channel user
                            Some(&engine_state.hnsw_index),
                        )
                        .await
                        {
                            Ok(Some(id)) => info!(
                                "[engine] Session summary stored in Engram ({} chars, id={})",
                                session_summary.len(),
                                safe_truncate(&id, 8)
                            ),
                            Ok(None) => info!("[engine] Session summary skipped (near-duplicate)"),
                            Err(e) => {
                                warn!("[engine] Engram session summary failed: {}", e)
                            }
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
                // Don't store the user message here — the normal engine_chat_send
                // flow will store it when the frontend re-sends. Storing here would
                // create a duplicate.
                //
                // Emit a queue-ready event so the frontend re-sends via normal flow.
                // The frontend listens for "engine-queue-ready" and calls engineChatSend
                // with the queued message, which goes through the full chat pipeline
                // (system prompt construction, context loading, tool building, etc.)
                let _ = queue_app.emit(
                    "engine-queue-ready",
                    serde_json::json!({
                        "sessionId": queue_session_id,
                        "message": queued.request.message,
                        "model": queued.request.model,
                    }),
                );
                info!("[engine] Emitted engine-queue-ready for frontend re-send");
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
                        total_rounds: None,
                        max_rounds: None,
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
