// Paw Agent Engine — Channel Agent Routing
//
// Routes user messages through the agent loop and returns text responses.
// This is the shared core that every channel bridge calls after receiving a message.

use crate::atoms::error::EngineResult;
use crate::engine::agent_loop;
use crate::engine::chat as chat_org;
use crate::engine::engram;
use crate::engine::injection;
use crate::engine::memory;
use crate::engine::providers::AnyProvider;
use crate::engine::state::{
    normalize_model_name, resolve_provider_for_model, EngineState, PendingApprovals,
};
use crate::engine::types::*;
use log::{error, info, warn};
use tauri::Manager;

/// Run a user message through the agent loop and return the text response.
/// This is the shared core that every channel bridge calls after receiving a message.
///
/// - `channel_prefix`: e.g. "discord", "irc" — used for session IDs ("eng-discord-{user_id}")
/// - `channel_context`: extra system prompt text (e.g. "User is on Discord. Keep replies concise.")
/// - `message`:      the user's message text
/// - `user_id`:      unique user identifier (platform-specific)
/// - `agent_id`:     which agent config to use ("default" if unset)
pub async fn run_channel_agent(
    app_handle: &tauri::AppHandle,
    channel_prefix: &str,
    channel_context: &str,
    message: &str,
    user_id: &str,
    agent_id: &str,
    allow_dangerous_tools: bool,
) -> EngineResult<String> {
    let engine_state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine not initialized")?;

    // ── Prompt injection scan ──────────────────────────────────────
    let scan = injection::scan_for_injection(message);
    if scan.is_injection {
        injection::log_injection_detected(channel_prefix, user_id, &scan);
        // Block critical injections
        if scan.severity == Some(injection::InjectionSeverity::Critical) {
            warn!(
                "[{}] Blocked critical injection from user {}",
                channel_prefix, user_id
            );
            return Ok("Your message was blocked by the security scanner. If this is a mistake, please rephrase.".into());
        }
    }

    // Per-user per-agent session: eng-{channel}-{agent}-{user_id}
    let session_id = format!("eng-{}-{}-{}", channel_prefix, agent_id, user_id);

    // Get provider config — channel bridges use the DEFAULT model (not worker_model).
    // Channel bridges handle complex multi-step tasks (creating 15+ Discord channels,
    // managing permissions, etc.) that require a capable model. The worker_model is
    // intended for cheap sub-agent tasks, not interactive channel management.
    let (provider_config, model, system_prompt, max_rounds, tool_timeout) = {
        let cfg = engine_state.config.lock();

        let default_model = cfg.default_model.clone().unwrap_or_else(|| "gpt-4o".into());
        // Use "channel" role — falls through to the default model since there's
        // no channel-specific override in model routing. Users can add one in
        // agent_models if they want a specific model for a specific agent.
        let model = normalize_model_name(&cfg.model_routing.resolve(
            agent_id,
            "channel",
            "",
            &default_model,
        ))
        .to_string();
        let provider = resolve_provider_for_model(&model, &cfg.providers)
            .or_else(|| {
                cfg.default_provider
                    .as_ref()
                    .and_then(|dp| cfg.providers.iter().find(|p| p.id == *dp).cloned())
            })
            .or_else(|| cfg.providers.first().cloned())
            .ok_or("No AI provider configured")?;

        let sp = cfg.default_system_prompt.clone();
        info!(
            "[{}] Resolved model for agent '{}': {} (default: {})",
            channel_prefix, agent_id, model, default_model
        );
        (
            provider,
            model,
            sp,
            cfg.max_tool_rounds,
            cfg.tool_timeout_secs,
        )
    };

    // Ensure session exists
    let session_exists = engine_state
        .store
        .get_session(&session_id)
        .map(|opt| opt.is_some())
        .unwrap_or(false);
    if !session_exists {
        engine_state.store.create_session(
            &session_id,
            &model,
            system_prompt.as_deref(),
            Some(agent_id),
        )?;
    } else {
        // Check if the previous conversation is poisoned by:
        // 1. Failed tool-call loops (all tool calls, no useful text)
        // 2. Accumulated error patterns from empty responses / fallbacks
        // 3. Malformed tool calls or model confusion
        // In any of these cases, clear the session to start fresh.
        let recent = engine_state
            .store
            .load_conversation(&session_id, None, Some(2_000), Some(agent_id))
            .unwrap_or_default();

        let should_clear = if recent.len() >= 4 {
            let last_msgs: Vec<&Message> = recent.iter().rev().take(6).collect();
            // Detect tool-call spam: all recent messages are tool/system with no user-visible text
            let all_tool_spam = last_msgs.iter().all(|m| {
                m.role == Role::Tool
                    || m.role == Role::System
                    || (m.role == Role::Assistant && m.tool_calls.is_some())
            });
            // Detect error / empty-response accumulation across recent history
            let error_count = last_msgs
                .iter()
                .filter(|m| {
                    if m.role != Role::Assistant {
                        return false;
                    }
                    let text = m.content.as_text_ref();
                    text.contains("wasn't able to generate")
                        || text.contains("empty response")
                        || text.contains("[MALFORMED_TOOL_CALL]")
                        || text.contains("start a new session")
                        || text.contains("content filter was triggered")
                        || (text.len() < 30 && text.trim().is_empty())
                })
                .count();
            all_tool_spam || error_count >= 2
        } else {
            false
        };

        if should_clear {
            info!(
                "[{}] Session {} appears poisoned. Clearing history for fresh start.",
                channel_prefix, session_id
            );
            let _ = engine_state.store.clear_messages(&session_id);
        }
    }

    // Store user message
    let user_msg = StoredMessage {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.clone(),
        role: "user".into(),
        content: message.to_string(),
        tool_calls_json: None,
        tool_call_id: None,
        name: None,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    engine_state.store.add_message(&user_msg)?;

    // Load core soul files (IDENTITY.md, SOUL.md, USER.md) — lean identity only.
    // We do NOT load compose_agent_context() here — it includes ALL agent files
    // (IDENTITY, SOUL, USER, AGENTS, TOOLS, custom files) which can add ~10K chars
    // of irrelevant context about other agents, coding tools, etc.
    let core_context = engine_state
        .store
        .compose_core_context(agent_id)
        .unwrap_or(None);
    if let Some(ref cc) = core_context {
        info!(
            "[{}] Core soul context loaded ({} chars) for agent '{}'",
            channel_prefix,
            cc.len(),
            agent_id
        );
    }

    // ── Engram Cognitive Pipeline for channels (§1/§8) ────────────────
    // Activate the same three-tier pipeline as chat: sensory buffer + working
    // memory + gated_search. This gives channel users cross-session memory
    // continuity (Discord/Slack/Telegram users can build on prior context).
    // Uses a lightweight budget (top-5 recall, low-latency gate) to keep
    // channel latency acceptable while still enabling trajectory recall.
    let cognitive_lock = engine_state.get_cognitive_state(agent_id);
    {
        let mut cognitive = cognitive_lock.lock().await;
        cognitive.decay_turn();
        // §8.2 Adapt WM budget to the actual model for this channel request.
        cognitive.adapt_wm_budget(&model);
    }

    // Lightweight gated recall — channel-scoped, with momentum from WM
    let channel_recalled: Option<Vec<crate::atoms::engram_types::RetrievedMemory>> = {
        let emb_client = engine_state.embedding_client();
        let scope = crate::atoms::engram_types::MemoryScope {
            agent_id: Some(agent_id.to_string()),
            channel: Some(channel_prefix.to_string()),
            channel_user_id: Some(user_id.to_string()),
            ..Default::default()
        };
        let config = crate::atoms::engram_types::MemorySearchConfig::default();
        // Issue a signed capability token for read-path scope verification (§43.4)
        let read_cap = engram::memory_bus::issue_read_capability(agent_id).ok();
        // Get momentum from cognitive state for trajectory recall.
        // §59.4: If a user override was detected (explicit redirect command),
        // suppress momentum so recall serves the NEW topic, not the old trajectory.
        let override_detected = chat_org::is_user_override_phrase(&message.to_lowercase());
        let cognitive = cognitive_lock.lock().await;
        let mom_vecs: Vec<Vec<f32>> = if override_detected {
            Vec::new() // suppress old-topic momentum
        } else {
            cognitive.working_memory.momentum().to_vec()
        };
        let mom_ref: Option<&[Vec<f32>]> = if mom_vecs.is_empty() {
            None
        } else {
            Some(&mom_vecs)
        };
        drop(cognitive);

        match engram::gated_search::gated_search(
            &engine_state.store,
            &engram::gated_search::GatedSearchRequest {
                query: message,
                scope: &scope,
                config: &config,
                embedding_client: emb_client.as_ref(),
                budget_tokens: 8_000, // lightweight budget for channels
                momentum: mom_ref,
                model: Some(&model), // per-model injection limits (§58.5)
                capability: read_cap.as_ref(),
                hnsw_index: Some(&engine_state.hnsw_index),
            },
        )
        .await
        {
            Ok(result) if !result.memories.is_empty() => {
                info!(
                    "[{}] Engram gated recall: {} memories for agent '{}'",
                    channel_prefix,
                    result.memories.len(),
                    agent_id
                );
                Some(result.memories)
            }
            Ok(_) => None,
            Err(e) => {
                warn!(
                    "[{}] Engram recall failed (non-fatal): {}",
                    channel_prefix, e
                );
                None
            }
        }
    };

    // Build full system prompt via ContextBuilder (§14 budget-aware).
    //
    // Channel agents intentionally skip the base system prompt (which describes
    // 41+ tools not available to channel bridges) and use channel_context as the
    // primary section. Memory recall was already performed above via gated_search
    // (with override detection + CRAG quality gating), so we inject those results
    // as a custom section rather than using ContextBuilder's recall_from.
    let cb_context_window = {
        let cfg = engine_state.config.lock();
        cfg.context_window_tokens
    };

    let provider_name = format!("{:?}", provider_config.kind);
    let user_tz = {
        let cfg = engine_state.config.lock();
        cfg.user_timezone.clone()
    };
    let runtime_ctx =
        chat_org::build_runtime_context(&model, &provider_name, &session_id, agent_id, &user_tz);

    let discipline_text = "## Conversation Discipline\n\
        - **Act immediately.** When the user asks you to do something, start doing it with your tools right now. Don't ask for confirmation.\n\
        - **For creating channels/categories:** Use `discord_setup_channels` — it creates everything in ONE call.\n\
        - **For individual operations** (sending messages, editing, permissions): Use `fetch`.\n\
        - **Never ask for information you already have.** Your server ID and API reference are above.\n\
        - **If a call fails, try again.** Don't give up or ask the user to do it manually.\n\
        - **Keep responses short.** Brief updates between actions, not essays.";

    let full_system_prompt = {
        let cognitive = cognitive_lock.lock().await;

        let mut builder = crate::engine::engram::context_builder::ContextBuilder::new(&model)
            .context_window(cb_context_window);

        // Channel context at priority 0 (highest — tells agent its job/API reference)
        builder = builder.custom_section("channel_context", channel_context, 0);

        // Core identity from soul files (IDENTITY.md, SOUL.md, USER.md)
        if let Some(ref cc) = core_context {
            builder = builder.core_context(cc.clone());
        }

        // Runtime context (model, session, time info)
        builder = builder.runtime_context(runtime_ctx);

        // Conversation discipline at priority 1
        builder = builder.custom_section("conversation_discipline", discipline_text, 1);

        // Pre-recalled memories from gated_search above (with CRAG quality gating)
        if let Some(ref recalled) = channel_recalled {
            let mut mem_parts: Vec<String> = vec!["## Recalled Context".to_string()];
            for (i, mem) in recalled.iter().take(5).enumerate() {
                mem_parts.push(format!("{}. [{}] {}", i + 1, mem.memory_type, mem.content));
            }
            builder = builder.custom_section("recalled_context", mem_parts.join("\n"), 7);
        }

        // Working memory slots (Tier 1)
        builder = builder.working_memory(&cognitive.working_memory);

        match builder.build().await {
            Ok(ctx) => {
                info!(
                    "[{}] ContextBuilder: sys={}tok reply={}tok mem={} for agent '{}'",
                    channel_prefix,
                    ctx.budget.system_prompt_tokens,
                    ctx.budget.available_for_reply,
                    ctx.budget.memories_injected,
                    agent_id
                );
                if let Some(emb) = ctx.query_embedding {
                    let cognitive_lock2 = engine_state.get_cognitive_state(agent_id);
                    let mut cog = cognitive_lock2.lock().await;
                    cog.working_memory.push_momentum(emb);
                }
                ctx.system_prompt
            }
            Err(e) => {
                warn!(
                    "[{}] ContextBuilder failed, falling back: {}",
                    channel_prefix, e
                );
                // Fallback: manual assembly
                let mut parts: Vec<String> = Vec::new();
                parts.push(channel_context.to_string());
                if let Some(ref cc) = core_context {
                    parts.push(cc.to_string());
                }
                parts.push(discipline_text.to_string());
                Some(parts.join("\n\n---\n\n"))
            }
        }
    };

    // Load conversation history.
    // Use the model capability registry to determine the actual context window
    // for this model, instead of a hardcoded 16K cap. Channel bridges benefit
    // from larger windows for multi-step tasks (e.g., creating 15+ channels).
    let context_window = {
        let cfg = engine_state.config.lock();
        let model_window = crate::engine::engram::model_caps::resolve_context_window(
            &model,
            cfg.context_window_tokens,
        );
        // Use the smaller of user config and model capability.
        // For channel bridges, cap at 50% of model window to leave room for tools.
        let channel_cap = model_window / 2;
        std::cmp::min(cfg.context_window_tokens, channel_cap.max(16_000))
    };
    let mut messages = engine_state.store.load_conversation(
        &session_id,
        full_system_prompt.as_deref(),
        Some(context_window),
        Some(agent_id),
    )?;

    // Build tools — CHANNEL WHITELIST.
    //
    // The full builtins() returns 41 tools + up to 48 skill tools = 89 tool
    // definitions. Each definition includes name + description + full JSON schema,
    // easily consuming 15-20K tokens. For a channel bridge that only needs `fetch`
    // to call Discord/Telegram APIs, this is catastrophic — the model drowns in
    // irrelevant tool definitions and returns empty responses.
    //
    // Channel bridges get ONLY the tools they can actually use:
    //   - fetch: HTTP calls to platform APIs (Discord, Telegram, etc.)
    //   - memory_store / memory_search: remember things across conversations
    //   - self_info: introspect own config when asked
    let mut tools: Vec<ToolDefinition> = {
        if allow_dangerous_tools {
            // Full tool set for trusted local clients (e.g. VS Code extension, local
            // webhook). allow_dangerous_tools=true means the caller is on the same
            // machine as the user — give them the full desktop-chat tool suite:
            // read_file, write_file, exec, memory, browser, and all built-ins.
            let tools = ToolDefinition::builtins();
            info!(
                "[{}] Full tool set ({} tools) — allow_dangerous_tools=true",
                channel_prefix,
                tools.len()
            );
            tools
        } else {
            // Restricted whitelist for untrusted remote channel clients
            // (Discord, Telegram, Slack, IRC, etc.). These bridges expose the
            // agent to arbitrary external users — limit to safe, read-only ops.
            let mut all_builtins = ToolDefinition::builtins();
            // Add all discord tools
            all_builtins.extend(crate::engine::tools::discord::definitions());
            let whitelist = [
                "fetch",
                "memory_store",
                "memory_search",
                "self_info",
                // channels
                "discord_setup_channels",
                "discord_list_channels",
                "discord_delete_channels",
                "discord_edit_channel",
                // messages
                "discord_send_message",
                "discord_edit_message",
                "discord_delete_messages",
                "discord_get_messages",
                "discord_pin_message",
                "discord_unpin_message",
                "discord_react",
                // roles
                "discord_list_roles",
                "discord_create_role",
                "discord_delete_role",
                "discord_assign_role",
                "discord_remove_role",
                // members
                "discord_list_members",
                "discord_get_member",
                "discord_kick",
                "discord_ban",
                "discord_unban",
                // server
                "discord_server_info",
                "discord_create_invite",
            ];
            let filtered: Vec<ToolDefinition> = all_builtins
                .into_iter()
                .filter(|t| whitelist.contains(&t.function.name.as_str()))
                .collect();
            info!(
                "[{}] Channel tool whitelist: {} tools",
                channel_prefix,
                filtered.len()
            );
            filtered
        }
    };

    let provider = AnyProvider::from_config(&provider_config);
    let run_id = uuid::Uuid::new_v4().to_string();

    // Channel bridge tool policy: deny side-effect tools that the agent loop
    // flags for HIL approval. Read-only tools are already auto-approved by the
    // agent_loop's own `auto_approved_tools` list and never reach this map.
    // Any tool that *does* land here is dangerous (exec, write_file, delete_file,
    // etc.) and must NOT be auto-approved for remote channel users.
    let approvals: PendingApprovals =
        std::sync::Arc::new(parking_lot::Mutex::new(std::collections::HashMap::new()));
    let approvals_clone = approvals.clone();
    let channel_prefix_owned = channel_prefix.to_string();
    let auto_approver = tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let mut map = approvals_clone.lock();
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                if let Some(sender) = map.remove(&key) {
                    if allow_dangerous_tools {
                        warn!(
                            "[{}] Auto-APPROVING dangerous tool (allow_dangerous_tools=true): {}",
                            channel_prefix_owned, key
                        );
                        let _ = sender.send(true);
                    } else {
                        warn!(
                            "[{}] Denying side-effect tool call from remote channel: {}",
                            channel_prefix_owned, key
                        );
                        let _ = sender.send(false);
                    }
                }
            }
        }
    });

    // ── Topic-shift & loop detection (§59.1) ────────────────────────────
    // Channel bridges previously had NO detect_response_loop protection.
    // The chat UI calls this before run_agent_turn — channels must too.
    // This catches: repetition, question loops, topic fixation, and short-
    // directive loops — injecting system redirects before the model runs.
    chat_org::detect_response_loop(&mut messages);

    // ── Explicit user-override detection (§59.2) ──────────────────────
    // Detects explicit commands like "stop", "focus on my question", "I asked
    // you to...", etc.  These get a STRONGER redirect than the statistical
    // loop detection and also clear working memory momentum so recalled
    // context serves the new topic.
    if chat_org::detect_user_override(&mut messages) {
        // Clear momentum to prevent old-topic recall bias
        let mut cognitive = cognitive_lock.lock().await;
        cognitive.working_memory.clear_momentum();
        info!(
            "[{}] User override detected — momentum cleared for agent '{}'",
            channel_prefix, agent_id
        );
    }

    let pre_loop_msg_count = messages.len();

    // Get daily budget config
    let daily_budget = {
        let cfg = engine_state.config.lock();
        cfg.daily_budget_usd
    };
    let daily_tokens_tracker = engine_state.daily_tokens.clone();

    // Run the agent loop — with provider fallback on billing/auth errors
    let result = {
        let primary_result = agent_loop::run_agent_turn(
            app_handle,
            &provider,
            &model,
            &mut messages,
            &mut tools,
            &session_id,
            &run_id,
            max_rounds,
            None,
            &approvals,
            tool_timeout,
            agent_id,
            daily_budget,
            Some(&daily_tokens_tracker),
            None,  // thinking_level
            false, // auto_approve_all — channels use safe default; Phase C adds per-channel policy
            &[],   // user_approved_tools — not available from channels
            None,  // yield_signal
        )
        .await;

        // If the primary provider failed with a billing/auth/rate error, try fallback providers
        match &primary_result {
            Err(e) if is_provider_billing_error(&e.to_string()) => {
                warn!(
                    "[{}] Primary provider failed ({}), trying fallback providers",
                    channel_prefix, e
                );
                let fallback_providers: Vec<ProviderConfig> = {
                    let cfg = engine_state.config.lock();
                    cfg.providers
                        .iter()
                        .filter(|p| p.id != provider_config.id)
                        .cloned()
                        .collect()
                };

                let mut fallback_result = primary_result;
                for fb_provider_cfg in &fallback_providers {
                    let fb_model = fb_provider_cfg
                        .default_model
                        .clone()
                        .unwrap_or_else(|| normalize_model_name(&model).to_string());
                    let fb_provider = AnyProvider::from_config(fb_provider_cfg);
                    info!(
                        "[{}] Trying fallback: {:?} / {}",
                        channel_prefix, fb_provider_cfg.kind, fb_model
                    );

                    // Reset messages to pre-loop state for retry
                    messages.truncate(pre_loop_msg_count);

                    let fb_run_id = uuid::Uuid::new_v4().to_string();
                    match agent_loop::run_agent_turn(
                        app_handle,
                        &fb_provider,
                        &fb_model,
                        &mut messages,
                        &mut tools,
                        &session_id,
                        &fb_run_id,
                        max_rounds,
                        None,
                        &approvals,
                        tool_timeout,
                        agent_id,
                        daily_budget,
                        Some(&daily_tokens_tracker),
                        None,  // thinking_level
                        false, // auto_approve_all — channels: safe default
                        &[],   // user_approved_tools
                        None,  // yield_signal
                    )
                    .await
                    {
                        Ok(text) => {
                            info!(
                                "[{}] Fallback {:?} succeeded",
                                channel_prefix, fb_provider_cfg.kind
                            );
                            fallback_result = Ok(text);
                            break;
                        }
                        Err(fb_err) => {
                            warn!(
                                "[{}] Fallback {:?} also failed: {}",
                                channel_prefix, fb_provider_cfg.kind, fb_err
                            );
                        }
                    }
                }
                fallback_result
            }
            _ => primary_result,
        }
    };

    // Stop the auto-approver
    auto_approver.abort();

    // Store new messages from the agent turn
    for msg in messages.iter().skip(pre_loop_msg_count) {
        if msg.role == Role::Assistant || msg.role == Role::Tool {
            let stored = StoredMessage {
                id: uuid::Uuid::new_v4().to_string(),
                session_id: session_id.clone(),
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
                error!("[{}] Failed to store message: {}", channel_prefix, e);
            }
        }
    }

    // Auto-capture memories (with dedup)
    if let Ok(final_text) = &result {
        // §3.1 Push into sensory buffer → working memory promotion pipeline
        // This gives channel users the same Tier 0 → Tier 1 promotion
        // that chat users get, enabling cross-turn context within a session.
        if !final_text.is_empty() {
            let mut cognitive = cognitive_lock.lock().await;
            cognitive.push_message(message, final_text);
        }

        let auto_capture = engine_state.memory_config.lock().auto_capture;
        if auto_capture && !final_text.is_empty() {
            let facts = memory::extract_memorable_facts_heuristic(message, final_text);
            if !facts.is_empty() {
                let emb_client = engine_state.embedding_client();
                for (content, category) in &facts {
                    // Legacy memory store
                    match memory::store_memory_dedup(
                        &engine_state.store,
                        content,
                        category,
                        5,
                        emb_client.as_ref(),
                        None,
                    )
                    .await
                    {
                        Ok(Some(_)) => {}
                        Ok(None) => info!("[channel-agent] Skipped duplicate memory"),
                        Err(e) => warn!("[channel-agent] Memory store failed: {}", e),
                    }

                    // Engram three-tier store (with channel/user scope)
                    let _ = engram::bridge::store_auto_capture(
                        &engine_state.store,
                        content,
                        category,
                        emb_client.as_ref(),
                        Some(agent_id),
                        Some(&session_id),
                        Some(channel_prefix),
                        Some(user_id),
                        Some(&engine_state.hnsw_index),
                    )
                    .await;
                }
            }
        }
    }

    result
}

// ── Utility ────────────────────────────────────────────────────────────

/// Detect billing, auth, quota, or rate-limit errors that warrant trying
/// a different provider instead of failing outright.
pub(crate) fn is_provider_billing_error(err: &str) -> bool {
    let lower = err.to_lowercase();
    lower.contains("credit balance")
        || lower.contains("insufficient_quota")
        || lower.contains("billing")
        || lower.contains("rate_limit")
        || lower.contains("quota exceeded")
        || lower.contains("payment required")
        || lower.contains("account")
        || (lower.contains("api error 4")
            && (lower.contains("401")
                || lower.contains("402")
                || lower.contains("403")
                || lower.contains("429")))
}

/// Convenience wrapper: resolve routing config to determine the agent_id,
/// then call run_channel_agent with that agent. Channels should prefer this
/// over calling run_channel_agent directly.
pub async fn run_routed_channel_agent(
    app_handle: &tauri::AppHandle,
    channel_prefix: &str,
    channel_context: &str,
    message: &str,
    user_id: &str,
    channel_id: Option<&str>,
    allow_dangerous_tools: bool,
) -> EngineResult<String> {
    // Load routing config and resolve agent
    let _engine_state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine not initialized")?;

    let routing_config = crate::engine::routing::load_routing_config(&std::sync::Arc::new(
        crate::engine::sessions::SessionStore::open()?,
    ));

    let route =
        crate::engine::routing::resolve_route(&routing_config, channel_prefix, user_id, channel_id);

    if route.matched_rule_id.is_some() {
        info!(
            "[{}] Routed user {} → agent '{}' (rule: {})",
            channel_prefix,
            user_id,
            route.agent_id,
            route.matched_rule_label.as_deref().unwrap_or("?")
        );
    }

    run_channel_agent(
        app_handle,
        channel_prefix,
        channel_context,
        message,
        user_id,
        &route.agent_id,
        allow_dangerous_tools,
    )
    .await
}
