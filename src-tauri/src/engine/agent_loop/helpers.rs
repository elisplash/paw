// agent_loop/helpers.rs — Extracted helper functions for the agent loop.
//
// Keeps the main `run_agent_turn` loop focused on orchestration by
// pulling out self-contained sub-operations: malformed call recovery,
// empty response nudging, tool-RAG hot-loading, and mid-loop context
// truncation.

use crate::engine::types::*;
use log::{info, warn};
use std::collections::HashSet;
use tauri::Manager;

// ── Malformed tool-call recovery ───────────────────────────────────────

/// Detect `[MALFORMED_TOOL_CALL]` in the model's text output and inject
/// corrective messages so the model retries with valid JSON.
///
/// Returns `true` if a retry was injected (caller should `continue` the loop).
pub fn handle_malformed_tool_call(
    final_text: &str,
    messages: &mut Vec<Message>,
    round: u32,
    max_rounds: u32,
) -> bool {
    let is_malformed = final_text.contains("[MALFORMED_TOOL_CALL]");
    if !is_malformed || round > 2 || round >= max_rounds {
        return false;
    }

    warn!(
        "[engine] MALFORMED_FUNCTION_CALL detected at round {} — retrying with simplified instructions",
        round
    );

    messages.push(Message {
        role: Role::Assistant,
        content: MessageContent::Text(final_text.to_string()),
        tool_calls: None,
        tool_call_id: None,
        name: None,
    });
    messages.push(Message {
        role: Role::User,
        content: MessageContent::Text(
            "Your tool call was malformed. When using fetch with a JSON body, pass `body` as a JSON object, NOT a string. \
            Example: {\"url\":\"...\",\"method\":\"POST\",\"body\":{\"name\":\"test\",\"type\":0}} \
            Try again now — one API call at a time."
                .to_string(),
        ),
        tool_calls: None,
        tool_call_id: None,
        name: None,
    });
    true
}

// ── Empty response nudge ───────────────────────────────────────────────

/// When the model returns an empty response, inject a system nudge that
/// recaps the user's message and asks the model to retry.
///
/// Retries up to 2 times (rounds 1-2) before giving up. Long conversations
/// are more prone to empty responses due to context truncation, so we need
/// to retry beyond just round 1.
///
/// Returns `true` if a retry nudge was injected (caller should `continue`).
pub fn handle_empty_response(
    final_text: &str,
    messages: &mut Vec<Message>,
    round: u32,
    max_rounds: u32,
) -> bool {
    if !final_text.is_empty() || round > 2 || round >= max_rounds {
        return false;
    }

    warn!(
        "[engine] Model returned empty response at round {} — injecting nudge and retrying",
        round
    );

    let user_recap = messages
        .iter()
        .rev()
        .find(|m| m.role == Role::User)
        .map(|m| {
            let t = m.content.as_text_ref();
            if t.len() > 300 {
                format!("{}…", &t[..t.floor_char_boundary(300)])
            } else {
                t.to_string()
            }
        })
        .unwrap_or_default();

    let nudge = if user_recap.is_empty() {
        "[SYSTEM] The model returned an empty response. Retry the user's request. Use tools if needed."
            .to_string()
    } else {
        format!(
            "[SYSTEM] The model returned an empty response. The user's request is: \"{}\"\n\
            Respond to this request directly. Ignore previous conversation topics. Use tools if needed.",
            user_recap
        )
    };

    messages.push(Message {
        role: Role::System,
        content: MessageContent::Text(nudge),
        tool_calls: None,
        tool_call_id: None,
        name: None,
    });
    true
}

/// Return a static fallback message for persistently empty responses.
pub fn empty_response_fallback() -> String {
    "I wasn't able to generate a response. This can happen when:\n\
    - The conversation context is very large (try compacting the session)\n\
    - A content filter was triggered (try rephrasing)\n\
    - The model is overwhelmed — try starting a new session\n\n\
    Please try again or start a new session."
        .to_string()
}

// ── Tool-RAG hot-loading ───────────────────────────────────────────────

/// After tool execution, check if `request_tools` added new tool names to
/// `loaded_tools`. If so, find their definitions and inject them into the
/// active tool list so the model can use them in the next round.
pub fn refresh_tool_rag(app_handle: &tauri::AppHandle, tools: &mut Vec<ToolDefinition>) {
    let Some(state) = app_handle.try_state::<crate::engine::state::EngineState>() else {
        return;
    };

    let loaded = state.loaded_tools.lock().clone();
    let current_names: std::collections::HashSet<String> =
        tools.iter().map(|t| t.function.name.clone()).collect();
    let new_names: Vec<String> = loaded.difference(&current_names).cloned().collect();

    if new_names.is_empty() {
        return;
    }

    // Build the full tool registry to find the definitions
    let mut all_defs = ToolDefinition::builtins();
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
    all_defs.extend(ToolDefinition::skill_tools(&enabled_ids));

    let mut added = 0;
    for def in all_defs {
        if new_names.contains(&def.function.name) {
            info!(
                "[tool-rag] Hot-loading tool '{}' into active round",
                def.function.name
            );
            tools.push(def);
            added += 1;
        }
    }
    if added > 0 {
        info!(
            "[tool-rag] Injected {} new tools into active tool list (now {} total)",
            added,
            tools.len()
        );
    }
}

// ── Mid-loop context truncation ────────────────────────────────────────

/// Estimate the token count of a single message (chars/4 heuristic).
fn estimate_msg_tokens(m: &Message) -> usize {
    let text_len = match &m.content {
        MessageContent::Text(t) => t.len(),
        MessageContent::Blocks(blocks) => blocks
            .iter()
            .map(|b| match b {
                ContentBlock::Text { text } => text.len(),
                ContentBlock::ImageUrl { .. } => 1000,
                ContentBlock::Document { data, .. } => data.len() / 4,
            })
            .sum(),
    };
    let tc_len = m
        .tool_calls
        .as_ref()
        .map(|tcs| {
            tcs.iter()
                .map(|tc| tc.function.arguments.len() + tc.function.name.len() + 20)
                .sum::<usize>()
        })
        .unwrap_or(0);
    (text_len + tc_len) / 4 + 4
}

/// Truncate the message history mid-loop so later rounds don't exceed the
/// context window. Preserves the system prompt (first message) and the last
/// user message. Ensures the first non-system message is a User message
/// (required by Gemini).
pub fn truncate_mid_loop(app_handle: &tauri::AppHandle, messages: &mut Vec<Message>) {
    let mid_loop_max = {
        if let Some(state) = app_handle.try_state::<crate::engine::state::EngineState>() {
            let cfg = state.config.lock();
            cfg.context_window_tokens
        } else {
            32_000
        }
    };

    let mid_total: usize = messages.iter().map(estimate_msg_tokens).sum();
    if mid_total <= mid_loop_max || messages.len() <= 3 {
        return;
    }

    // Preserve system prompt (index 0)
    let sys_msg = if !messages.is_empty() && messages[0].role == Role::System {
        Some(messages.remove(0))
    } else {
        None
    };
    let sys_tokens = sys_msg.as_ref().map(estimate_msg_tokens).unwrap_or(0);
    let msg_tokens: Vec<usize> = messages.iter().map(estimate_msg_tokens).collect();
    let mut running = sys_tokens + msg_tokens.iter().sum::<usize>();

    // Find last user message — never drop past it
    let last_user_idx = messages
        .iter()
        .rposition(|m| m.role == Role::User)
        .unwrap_or(messages.len().saturating_sub(1));
    let mut keep_from = 0;

    for (i, &t) in msg_tokens.iter().enumerate() {
        if running <= mid_loop_max {
            break;
        }
        if i >= last_user_idx {
            break;
        }
        running -= t;
        keep_from = i + 1;
    }

    // Ensure we don't split a tool-call/tool-result pair:
    // If keep_from lands on a Tool message, advance past all
    // consecutive Tool messages so we don't orphan them.
    while keep_from < messages.len() && messages[keep_from].role == Role::Tool {
        if keep_from < msg_tokens.len() {
            running -= msg_tokens[keep_from];
        }
        keep_from += 1;
    }

    // Ensure the first non-system message is a User message.
    // Gemini (and other providers) require the conversation to
    // start with a user turn — starting with an assistant turn
    // containing functionCall causes 400 errors.
    while keep_from < messages.len()
        && keep_from < last_user_idx
        && messages[keep_from].role != Role::User
    {
        if keep_from < msg_tokens.len() {
            running -= msg_tokens[keep_from];
        }
        keep_from += 1;
    }

    if keep_from > 0 {
        *messages = messages.split_off(keep_from);
        if let Some(sys) = sys_msg {
            messages.insert(0, sys);
        }
        info!(
            "[engine] Mid-loop truncation: {} → {} est tokens, {} messages kept",
            mid_total,
            running,
            messages.len()
        );

        // After truncation, orphaned tool_use / tool_result pairs may remain.
        // Re-sanitize to prevent Anthropic 400 errors.
        sanitize_tool_pairs(messages);
    } else if let Some(sys) = sys_msg {
        messages.insert(0, sys);
    }
}

/// Ensure every assistant message with tool_calls has matching tool_result
/// messages, and every tool_result has a matching preceding tool_use.
///
/// Three passes:
///   1. Strip leading orphan tool_result messages (no parent assistant).
///   2. For each assistant+tool_calls, inject synthetic results for missing IDs.
///   3. Remove any remaining orphan tool_results whose tool_use_id doesn't
///      appear in any preceding assistant message.
///
/// This is called after mid-loop truncation and also during conversation
/// loading. It is intentionally duplicated here (rather than calling into
/// sessions::messages) to avoid a cross-module dependency cycle.
pub fn sanitize_tool_pairs(messages: &mut Vec<Message>) {
    // ── Pass 1: strip leading orphan tool results ──────────────────
    let first_non_system = messages
        .iter()
        .position(|m| m.role != Role::System)
        .unwrap_or(0);
    let mut strip_end = first_non_system;
    while strip_end < messages.len() && messages[strip_end].role == Role::Tool {
        strip_end += 1;
    }
    if strip_end > first_non_system {
        let removed = strip_end - first_non_system;
        warn!(
            "[engine] Removing {} orphaned leading tool_result messages",
            removed
        );
        messages.drain(first_non_system..strip_end);
    }

    // ── Pass 2: ensure every assistant+tool_calls has matching results ─
    let mut i = 0;
    while i < messages.len() {
        let has_tc = messages[i].role == Role::Assistant
            && messages[i]
                .tool_calls
                .as_ref()
                .map(|tc| !tc.is_empty())
                .unwrap_or(false);

        if !has_tc {
            i += 1;
            continue;
        }

        // Collect expected tool_call IDs from this assistant message
        let expected_ids: Vec<String> = messages[i]
            .tool_calls
            .as_ref()
            .unwrap()
            .iter()
            .map(|tc| tc.id.clone())
            .collect();

        // Scan following messages for tool results, skipping System messages
        // (context injections can insert System messages between assistant
        // and tool-result blocks).
        let mut found_ids = HashSet::new();
        let mut j = i + 1;
        while j < messages.len() {
            match messages[j].role {
                Role::Tool => {
                    if let Some(ref tcid) = messages[j].tool_call_id {
                        found_ids.insert(tcid.clone());
                    }
                    j += 1;
                }
                Role::System => {
                    // Skip injected system messages — don't break the scan
                    j += 1;
                }
                _ => break,
            }
        }

        // Inject synthetic results for any missing tool_call IDs
        let mut injected = 0;
        for expected_id in &expected_ids {
            if !found_ids.contains(expected_id) {
                let synthetic = Message {
                    role: Role::Tool,
                    content: MessageContent::Text(
                        "[Tool execution was interrupted or result was lost.]".into(),
                    ),
                    tool_calls: None,
                    tool_call_id: Some(expected_id.clone()),
                    name: Some("_synthetic".into()),
                };
                messages.insert(i + 1 + injected, synthetic);
                injected += 1;
            }
        }

        if injected > 0 {
            warn!(
                "[engine] Injected {} synthetic tool_result(s) for orphaned tool_use IDs",
                injected
            );
        }

        // Advance past this assistant message + all following tool/system results
        i += 1;
        while i < messages.len()
            && (messages[i].role == Role::Tool || messages[i].role == Role::System)
        {
            i += 1;
        }
    }

    // ── Pass 3: remove orphan tool_results whose tool_use_id has no ───
    //    matching tool_use in any preceding assistant message.
    let mut known_tc_ids: HashSet<String> = HashSet::new();
    let mut to_remove: Vec<usize> = Vec::new();

    for (idx, msg) in messages.iter().enumerate() {
        if msg.role == Role::Assistant {
            if let Some(tcs) = &msg.tool_calls {
                for tc in tcs {
                    known_tc_ids.insert(tc.id.clone());
                }
            }
        } else if msg.role == Role::Tool {
            if let Some(ref tcid) = msg.tool_call_id {
                if !known_tc_ids.contains(tcid) {
                    to_remove.push(idx);
                }
            }
        }
    }

    if !to_remove.is_empty() {
        warn!(
            "[engine] Removing {} orphaned tool_result messages (no matching tool_use)",
            to_remove.len()
        );
        // Remove in reverse to preserve indices
        for &idx in to_remove.iter().rev() {
            messages.remove(idx);
        }
    }
}
