// Paw Agent Engine — Channel Agent Routing
//
// Routes user messages through the agent loop and returns text responses.
// This is the shared core that every channel bridge calls after receiving a message.

use crate::engine::types::*;
use crate::engine::providers::AnyProvider;
use crate::engine::agent_loop;
use crate::engine::skills;
use crate::engine::memory;
use crate::engine::injection;
use crate::engine::chat as chat_org;
use crate::engine::state::{EngineState, PendingApprovals, normalize_model_name, resolve_provider_for_model};
use log::{info, warn, error};
use tauri::Manager;
use crate::atoms::error::EngineResult;

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
) -> EngineResult<String> {
    let engine_state = app_handle.try_state::<EngineState>()
        .ok_or("Engine not initialized")?;

    // ── Prompt injection scan ──────────────────────────────────────
    let scan = injection::scan_for_injection(message);
    if scan.is_injection {
        injection::log_injection_detected(channel_prefix, user_id, &scan);
        // Block critical injections
        if scan.severity == Some(injection::InjectionSeverity::Critical) {
            warn!("[{}] Blocked critical injection from user {}", channel_prefix, user_id);
            return Ok("⚠️ Your message was blocked by the security scanner. If this is a mistake, please rephrase.".into());
        }
    }

    // Per-user per-agent session: eng-{channel}-{agent}-{user_id}
    let session_id = format!("eng-{}-{}-{}", channel_prefix, agent_id, user_id);

    // Get provider config — use model_routing.resolve() so the worker_model
    // setting is respected for chat bridges (instead of burning the expensive
    // default_model on every message)
    let (provider_config, model, system_prompt, max_rounds, tool_timeout) = {
        let cfg = engine_state.config.lock();

        let default_model = cfg.default_model.clone().unwrap_or_else(|| "gpt-4o".into());
        let model = normalize_model_name(
            &cfg.model_routing.resolve(agent_id, "worker", "", &default_model)
        ).to_string();
        let provider = resolve_provider_for_model(&model, &cfg.providers)
            .or_else(|| {
                cfg.default_provider.as_ref()
                    .and_then(|dp| cfg.providers.iter().find(|p| p.id == *dp).cloned())
            })
            .or_else(|| cfg.providers.first().cloned())
            .ok_or("No AI provider configured")?;

        let sp = cfg.default_system_prompt.clone();
        info!("[{}] Resolved model for agent '{}': {} (default: {})", channel_prefix, agent_id, model, default_model);
        (provider, model, sp, cfg.max_tool_rounds, cfg.tool_timeout_secs)
    };

    // Ensure session exists
    let session_exists = engine_state.store.get_session(&session_id)
        .map(|opt| opt.is_some())
        .unwrap_or(false);
    if !session_exists {
        engine_state.store.create_session(&session_id, &model, system_prompt.as_deref(), Some(agent_id))?;
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

    // Compose system prompt with agent context + memory + skills
    let agent_context = engine_state.store.compose_agent_context(agent_id).unwrap_or(None);
    let skill_instructions = skills::get_enabled_skill_instructions(&engine_state.store, agent_id).unwrap_or_default();

    // Load core soul files (IDENTITY.md, SOUL.md, USER.md) — same as UI chat
    let core_context = engine_state.store.compose_core_context(agent_id).unwrap_or(None);
    if let Some(ref cc) = core_context {
        info!("[{}] Core soul context loaded ({} chars) for agent '{}'", channel_prefix, cc.len(), agent_id);
    }

    // Load today's memory notes — same as UI chat
    let todays_memories = engine_state.store.get_todays_memories(agent_id).unwrap_or(None);

    // Auto-recall memories
    let (auto_recall_on, recall_limit, recall_threshold) = {
        let mcfg = engine_state.memory_config.lock();
        (mcfg.auto_recall, mcfg.recall_limit, mcfg.recall_threshold)
    };

    let memory_context = if auto_recall_on {
        let emb_client = engine_state.embedding_client();
        match memory::search_memories(
            &engine_state.store, message, recall_limit, recall_threshold, emb_client.as_ref(), None
        ).await {
            Ok(mems) if !mems.is_empty() => {
                let ctx: Vec<String> = mems.iter().map(|m| format!("- [{}] {}", m.category, m.content)).collect();
                Some(format!("## Relevant Memories\n{}", ctx.join("\n")))
            }
            _ => None,
        }
    } else {
        None
    };

    // Build full system prompt — use the same rich prompt as the UI chat
    // so the agent has full awareness of its tools, soul files, and memory.
    let full_system_prompt = {
        // Build base prompt with channel-specific context prepended
        let base_prompt = match &system_prompt {
            Some(sp) => Some(format!("{}\n\n{}", channel_context, sp)),
            None => Some(channel_context.to_string()),
        };

        // Build runtime context (model, provider, session, agent, time, workspace)
        let provider_name = format!("{:?}", provider_config.kind);
        let user_tz = {
            let cfg = engine_state.config.lock();
            cfg.user_timezone.clone()
        };
        let runtime_context = chat_org::build_runtime_context(
            &model, &provider_name, &session_id, agent_id, &user_tz,
        );

        // Compose the full prompt with Soul Files + Memory instructions
        let mut prompt = chat_org::compose_chat_system_prompt(
            base_prompt.as_deref(),
            runtime_context,
            core_context.as_deref(),
            todays_memories.as_deref(),
            &skill_instructions,
        );

        // Append agent-specific context and auto-recalled memories
        if let Some(ref mut p) = prompt {
            if let Some(ac) = &agent_context {
                p.push_str("\n\n---\n\n");
                p.push_str(ac);
            }
            if let Some(mc) = &memory_context {
                p.push_str("\n\n---\n\n");
                p.push_str(mc);
            }
        }

        prompt
    };

    // Load conversation history
    let mut messages = engine_state.store.load_conversation(
        &session_id,
        full_system_prompt.as_deref(),
    )?;

    // Build tools — read-only tools are auto-approved by agent_loop;
    // side-effect tools (exec, write_file, etc.) will be denied for remote channels.
    let tools = {
        let mut t = ToolDefinition::builtins();
        let enabled_ids: Vec<String> = skills::builtin_skills().iter()
            .filter(|s| engine_state.store.is_skill_enabled(&s.id).unwrap_or(false))
            .map(|s| s.id.clone())
            .collect();
        if !enabled_ids.is_empty() {
            t.extend(ToolDefinition::skill_tools(&enabled_ids));
        }
        t
    };

    let provider = AnyProvider::from_config(&provider_config);
    let run_id = uuid::Uuid::new_v4().to_string();

    // Channel bridge tool policy: deny side-effect tools that the agent loop
    // flags for HIL approval. Read-only tools are already auto-approved by the
    // agent_loop's own `auto_approved_tools` list and never reach this map.
    // Any tool that *does* land here is dangerous (exec, write_file, delete_file,
    // etc.) and must NOT be auto-approved for remote channel users.
    let approvals: PendingApprovals = std::sync::Arc::new(parking_lot::Mutex::new(std::collections::HashMap::new()));
    let approvals_clone = approvals.clone();
    let channel_prefix_owned = channel_prefix.to_string();
    let auto_approver = tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let mut map = approvals_clone.lock();
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                if let Some(sender) = map.remove(&key) {
                    warn!("[{}] Denying side-effect tool call from remote channel: {}", channel_prefix_owned, key);
                    let _ = sender.send(false);
                }
            }
        }
    });

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
            &tools,
            &session_id,
            &run_id,
            max_rounds,
            None,
            &approvals,
            tool_timeout,
            agent_id,
            daily_budget,
            Some(&daily_tokens_tracker),
            None, // thinking_level
        ).await;

        // If the primary provider failed with a billing/auth/rate error, try fallback providers
        match &primary_result {
            Err(e) if is_provider_billing_error(&e.to_string()) => {
                warn!("[{}] Primary provider failed ({}), trying fallback providers", channel_prefix, e);
                let fallback_providers: Vec<ProviderConfig> = {
                    let cfg = engine_state.config.lock();
                    cfg.providers.iter()
                        .filter(|p| p.id != provider_config.id)
                        .cloned()
                        .collect()
                };

                let mut fallback_result = primary_result;
                for fb_provider_cfg in &fallback_providers {
                    let fb_model = fb_provider_cfg.default_model.clone()
                        .unwrap_or_else(|| normalize_model_name(&model).to_string());
                    let fb_provider = AnyProvider::from_config(fb_provider_cfg);
                    info!("[{}] Trying fallback: {:?} / {}", channel_prefix, fb_provider_cfg.kind, fb_model);

                    // Reset messages to pre-loop state for retry
                    messages.truncate(pre_loop_msg_count);

                    let fb_run_id = uuid::Uuid::new_v4().to_string();
                    match agent_loop::run_agent_turn(
                        app_handle,
                        &fb_provider,
                        &fb_model,
                        &mut messages,
                        &tools,
                        &session_id,
                        &fb_run_id,
                        max_rounds,
                        None,
                        &approvals,
                        tool_timeout,
                        agent_id,
                        daily_budget,
                        Some(&daily_tokens_tracker),
                        None, // thinking_level
                    ).await {
                        Ok(text) => {
                            info!("[{}] Fallback {:?} succeeded", channel_prefix, fb_provider_cfg.kind);
                            fallback_result = Ok(text);
                            break;
                        }
                        Err(fb_err) => {
                            warn!("[{}] Fallback {:?} also failed: {}", channel_prefix, fb_provider_cfg.kind, fb_err);
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
                tool_calls_json: msg.tool_calls.as_ref()
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

    // Auto-capture memories
    if let Ok(final_text) = &result {
        let auto_capture = engine_state.memory_config.lock().auto_capture;
        if auto_capture && !final_text.is_empty() {
            let facts = memory::extract_memorable_facts(message, final_text);
            if !facts.is_empty() {
                let emb_client = engine_state.embedding_client();
                for (content, category) in &facts {
                    let _ = memory::store_memory(
                        &engine_state.store, content, category, 5, emb_client.as_ref(), None
                    ).await;
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
        || (lower.contains("api error 4") && (
            lower.contains("401") || lower.contains("402")
            || lower.contains("403") || lower.contains("429")
        ))
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
) -> EngineResult<String> {
    // Load routing config and resolve agent
    let _engine_state = app_handle.try_state::<EngineState>()
        .ok_or("Engine not initialized")?;

    let routing_config = crate::engine::routing::load_routing_config(
        &std::sync::Arc::new(crate::engine::sessions::SessionStore::open()?)
    );

    let route = crate::engine::routing::resolve_route(
        &routing_config,
        channel_prefix,
        user_id,
        channel_id,
    );

    if route.matched_rule_id.is_some() {
        info!(
            "[{}] Routed user {} → agent '{}' (rule: {})",
            channel_prefix, user_id, route.agent_id,
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
    ).await
}
