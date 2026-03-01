// Paw Agent Engine — Swarm Collaboration
//
// When agents broadcast to a squad, auto-wake recipient agents so they
// read inbound messages, think, and respond — creating real back-and-forth
// collaborative discussion.
//
// Depth-limited: max N auto-wakes per squad activation to prevent infinite loops.
// Counter resets after cooldown period or when explicitly cleared.

use crate::engine::agent_loop;
use crate::engine::chat as chat_org;
use crate::engine::providers::AnyProvider;
use crate::engine::skills;
use crate::engine::state::{normalize_model_name, resolve_provider_for_model, EngineState};
use crate::engine::types::*;
use log::{info, warn};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, LazyLock};
use tauri::{Emitter, Manager};

/// Base auto-wake budget per squad member per human turn.
/// Total limit = members × WAKES_PER_MEMBER (e.g., 3-member squad → 12 wakes).
const WAKES_PER_MEMBER: u32 = 4;

/// Absolute ceiling regardless of squad size.
const MAX_SWARM_WAKES_CAP: u32 = 24;

/// Per-squad swarm counters — tracks how many auto-wakes have fired.
static SWARM_COUNTERS: LazyLock<parking_lot::Mutex<HashMap<String, Arc<AtomicU32>>>> =
    LazyLock::new(|| parking_lot::Mutex::new(HashMap::new()));

/// Per-squad member counts — used to compute dynamic limits.
static SQUAD_MEMBER_COUNT: LazyLock<parking_lot::Mutex<HashMap<String, u32>>> =
    LazyLock::new(|| parking_lot::Mutex::new(HashMap::new()));

/// Set the member count for a squad (called when broadcasts happen).
pub fn set_squad_size(squad_id: &str, member_count: u32) {
    let mut counts = SQUAD_MEMBER_COUNT.lock();
    counts.insert(squad_id.to_string(), member_count);
}

/// Get the dynamic wake limit for a squad based on its member count.
fn wake_limit(squad_id: &str) -> u32 {
    let counts = SQUAD_MEMBER_COUNT.lock();
    let members = counts.get(squad_id).copied().unwrap_or(3);
    (members * WAKES_PER_MEMBER).min(MAX_SWARM_WAKES_CAP)
}

/// Check if we can auto-wake more agents for this squad.
pub fn can_auto_wake(squad_id: &str) -> bool {
    let limit = wake_limit(squad_id);
    let counters = SWARM_COUNTERS.lock();
    counters
        .get(squad_id)
        .map(|c| c.load(Ordering::Relaxed) < limit)
        .unwrap_or(true)
}

/// Increment the swarm counter for a squad. Returns the new count.
fn increment_counter(squad_id: &str) -> u32 {
    let mut counters = SWARM_COUNTERS.lock();
    let counter = counters
        .entry(squad_id.to_string())
        .or_insert_with(|| Arc::new(AtomicU32::new(0)));
    counter.fetch_add(1, Ordering::Relaxed) + 1
}

/// Reset the swarm counter for a squad (called on new human input).
pub fn reset_counter(squad_id: &str) {
    let mut counters = SWARM_COUNTERS.lock();
    counters.remove(squad_id);
}

/// Reset ALL swarm counters — called at the start of each human chat turn
/// so sub-agents can wake up fresh for each new human message.
pub fn reset_all_counters() {
    let mut counters = SWARM_COUNTERS.lock();
    if !counters.is_empty() {
        info!(
            "[swarm] Resetting swarm counters for {} squads (new human turn)",
            counters.len()
        );
        counters.clear();
    }
}

/// Spawn an auto-wake agent turn for a squad member.
///
/// The agent will read its inbound messages, think about the squad
/// context, and respond via squad_broadcast or agent_send_message.
///
/// This is fire-and-forget — the agent turn runs in the background.
/// Results are emitted as `swarm-activity` Tauri events.
pub fn spawn_swarm_reply(
    app_handle: &tauri::AppHandle,
    squad_id: &str,
    squad_name: &str,
    squad_goal: &str,
    sender_id: &str,
    recipient_id: &str,
    message_content: &str,
) -> Result<(), String> {
    // Check depth limit
    if !can_auto_wake(squad_id) {
        info!(
            "[swarm] Skipping auto-wake for '{}' — max depth reached for squad '{}'",
            recipient_id, squad_name
        );
        return Ok(());
    }

    let count = increment_counter(squad_id);
    let limit = wake_limit(squad_id);
    info!(
        "[swarm] Auto-waking '{}' for squad '{}' (wake {}/{})",
        recipient_id, squad_name, count, limit
    );

    // Clone everything we need into the async block
    let app = app_handle.clone();
    let squad_id = squad_id.to_string();
    let squad_name = squad_name.to_string();
    let squad_goal = squad_goal.to_string();
    let sender_id = sender_id.to_string();
    let recipient_id = recipient_id.to_string();
    let message_content = message_content.to_string();

    tauri::async_runtime::spawn(async move {
        // Small stagger so agents don't all hit the API at the exact same instant
        let delay_ms = (count as u64 % 3) * 1500;
        if delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        }

        // Emit swarm-activity: agent waking up
        let _ = app.emit(
            "swarm-activity",
            serde_json::json!({
                "agent_id": recipient_id,
                "squad_id": squad_id,
                "status": "waking",
            }),
        );

        match run_swarm_turn(
            &app,
            &squad_id,
            &squad_name,
            &squad_goal,
            &sender_id,
            &recipient_id,
            &message_content,
        )
        .await
        {
            Ok(text) => {
                info!(
                    "[swarm] Agent '{}' completed: {} chars",
                    recipient_id,
                    text.len()
                );
                let _ = app.emit(
                    "swarm-activity",
                    serde_json::json!({
                        "agent_id": recipient_id,
                        "squad_id": squad_id,
                        "status": "completed",
                        "summary": &text[..text.len().min(200)],
                    }),
                );
            }
            Err(e) => {
                warn!("[swarm] Agent '{}' error: {}", recipient_id, e);
                let _ = app.emit(
                    "swarm-activity",
                    serde_json::json!({
                        "agent_id": recipient_id,
                        "squad_id": squad_id,
                        "status": "error",
                        "error": e.to_string(),
                    }),
                );
            }
        }
    });

    Ok(())
}

/// Internal: run a full agent turn for a swarm-woken agent.
async fn run_swarm_turn(
    app_handle: &tauri::AppHandle,
    squad_id: &str,
    squad_name: &str,
    squad_goal: &str,
    sender_id: &str,
    recipient_id: &str,
    message_content: &str,
) -> Result<String, String> {
    let state = app_handle
        .try_state::<EngineState>()
        .ok_or_else(|| "Engine state not available".to_string())?;

    // Session per agent per squad — persistent across swarm rounds
    let session_id = format!("eng-swarm-{}-{}", squad_id, recipient_id);

    // Resolve model/provider for this agent.
    // Priority: agent's own stored model → model_routing → default_model → "gpt-4o"
    let (provider_config, model) = {
        let agent_model = state.store.get_agent_model(recipient_id);
        let cfg = state.config.lock();
        let default_model = cfg.default_model.clone().unwrap_or_else(|| "gpt-4o".into());
        let model = if let Some(ref am) = agent_model {
            // Agent has an explicit model override in the DB — honour it
            normalize_model_name(am).to_string()
        } else {
            normalize_model_name(&cfg.model_routing.resolve(
                recipient_id,
                "worker",
                "",
                &default_model,
            ))
            .to_string()
        };
        info!(
            "[swarm] Resolved model for '{}': {} (agent_override={})",
            recipient_id,
            model,
            agent_model.is_some()
        );
        let provider = resolve_provider_for_model(&model, &cfg.providers)
            .or_else(|| {
                cfg.default_provider
                    .as_ref()
                    .and_then(|dp| cfg.providers.iter().find(|p| p.id == *dp).cloned())
            })
            .or_else(|| cfg.providers.first().cloned())
            .ok_or_else(|| "No AI provider configured".to_string())?;
        (provider, model)
    };

    // Ensure session exists
    if state
        .store
        .get_session(&session_id)
        .ok()
        .flatten()
        .is_none()
    {
        state
            .store
            .create_session(&session_id, &model, None, Some(recipient_id))?;
    }

    // Build system prompt
    let base_system_prompt = {
        let cfg = state.config.lock();
        cfg.default_system_prompt.clone()
    };
    let agent_context = state
        .store
        .compose_core_context(recipient_id)
        .unwrap_or(None);
    let skill_instructions =
        skills::get_enabled_skill_instructions(&state.store, recipient_id).unwrap_or_default();

    let provider_name = format!("{:?}", provider_config.kind);
    let user_tz = {
        let cfg = state.config.lock();
        cfg.user_timezone.clone()
    };
    let runtime_context = chat_org::build_runtime_context(
        &model,
        &provider_name,
        &session_id,
        recipient_id,
        &user_tz,
    );

    // ── Auto-recall memories for the swarm agent (squad + agent scoped) ────
    let todays_memories = {
        let _squad_mem_scope =
            crate::atoms::engram_types::MemoryScope::squad(squad_id, recipient_id);
        let emb_client = state.embedding_client();
        match crate::engine::engram::bridge::search(
            &state.store,
            message_content,
            10,
            0.2,
            emb_client.as_ref(),
            Some(recipient_id),
        )
        .await
        {
            Ok(results) if !results.is_empty() => {
                let mem_text = results
                    .iter()
                    .map(|r| format!("- [{}] {}", r.category, r.content))
                    .collect::<Vec<_>>()
                    .join("\n");
                Some(format!("## Recalled Memories\n{}", mem_text))
            }
            _ => None,
        }
    };

    let mut full_system_prompt = chat_org::compose_chat_system_prompt(
        base_system_prompt.as_deref(),
        runtime_context,
        agent_context.as_deref(),
        todays_memories.as_deref(),
        &skill_instructions,
    );

    // Add swarm collaboration context
    let swarm_context = format!(
        "\n\n---\n\n## Squad Collaboration\n\
        You are a member of the **{}** squad.\n\
        - **Squad Goal**: {}\n\
        - **Your role**: Collaborate with fellow squad members toward the squad goal.\n\
        - You have just received a message from squad member '{}'.\n\
        - Read your inbound messages using `agent_read_messages` to see the full context.\n\
        - Use `squad_broadcast` to share your thoughts, analysis, and contributions with the entire squad.\n\
        - Focus on making progress toward the squad goal. Build on what others have said.\n\
        - Be concise but substantive. Avoid repeating what others have already covered.\n\
        - If the squad has reached a good conclusion or action plan, summarize it clearly.\n\
        - Do NOT ask questions or wait for user input — you are running autonomously.",
        squad_name, squad_goal, sender_id
    );

    if let Some(ref mut sp) = full_system_prompt {
        sp.push_str(&swarm_context);
    } else {
        full_system_prompt = Some(swarm_context.trim_start_matches("\n\n---\n\n").to_string());
    }

    // Store synthetic user message that triggers the agent
    let user_msg = StoredMessage {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.clone(),
        role: "user".into(),
        content: format!(
            "[Squad '{}' — message from {}]\n\n{}\n\n\
            Read your messages with `agent_read_messages` and respond to the squad using `squad_broadcast`.",
            squad_name, sender_id, message_content
        ),
        tool_calls_json: None,
        tool_call_id: None,
        name: None,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    state.store.add_message(&user_msg)?;

    // Load conversation history
    let context_window = {
        let cfg = state.config.lock();
        cfg.context_window_tokens
    };
    let mut messages = state.store.load_conversation(
        &session_id,
        full_system_prompt.as_deref(),
        Some(context_window),
        Some(recipient_id),
    )?;

    // Build tools — swarm agents get the full tool set (no Tool RAG gating)
    // because they're operating autonomously toward a specific goal.
    let loaded_tools = {
        let mut all_names = std::collections::HashSet::new();
        // Pre-load ALL tool names so swarm agents skip the librarian step
        for t in crate::atoms::types::ToolDefinition::builtins() {
            all_names.insert(t.function.name);
        }
        let enabled_ids: Vec<String> = crate::engine::skills::builtin_skills()
            .iter()
            .filter(|s| {
                state
                    .store
                    .get_skill_enabled_state(&s.id)
                    .unwrap_or(None)
                    .unwrap_or(s.default_enabled)
            })
            .map(|s| s.id.clone())
            .collect();
        for t in crate::atoms::types::ToolDefinition::skill_tools(&enabled_ids) {
            all_names.insert(t.function.name);
        }
        all_names
    };
    let mut tools = chat_org::build_chat_tools(&state.store, true, None, app_handle, &loaded_tools);

    let provider = AnyProvider::from_config(&provider_config);
    let run_id = uuid::Uuid::new_v4().to_string();

    // Extract config values
    let (max_rounds, tool_timeout) = {
        let cfg = state.config.lock();
        (cfg.max_tool_rounds.min(10), cfg.tool_timeout_secs) // Cap rounds for swarm
    };
    let daily_budget = {
        let cfg = state.config.lock();
        cfg.daily_budget_usd
    };

    let approvals = state.pending_approvals.clone();
    let daily_tokens = state.daily_tokens.clone();
    let sem = state.run_semaphore.clone();
    let pre_loop_msg_count = messages.len();

    // Acquire semaphore slot
    let _permit = sem.acquire_owned().await.ok();
    info!("[swarm] Agent '{}' acquired run slot", recipient_id);

    let result = agent_loop::run_agent_turn(
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
        recipient_id,
        daily_budget,
        Some(&daily_tokens),
        None, // thinking_level
        true, // auto_approve_all — swarm agents run autonomously
        &[],  // user_approved_tools
        None, // yield_signal
    )
    .await
    .map_err(|e| e.to_string())?;

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
            let _ = state.store.add_message(&stored);
        }
    }

    Ok(result)
}
