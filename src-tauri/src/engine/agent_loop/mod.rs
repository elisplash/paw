// Paw Agent Engine — Agentic Loop
// The core orchestration loop: send to model → tool calls → execute → repeat.
// This is the core agent loop that drives Pawz AI interactions.

mod trading;

use crate::engine::types::*;
use crate::engine::providers::AnyProvider;
use crate::engine::tools;
use crate::engine::state::{PendingApprovals, DailyTokenTracker};
use log::{info, warn};
use std::time::Duration;
use tauri::{Emitter, Manager};
use trading::check_trading_auto_approve;
use crate::atoms::error::EngineResult;

/// Run a complete agent turn: send messages to the model, execute tool calls,
/// and repeat until the model produces a final text response or max rounds hit.
///
/// Emits `engine-event` Tauri events for real-time streaming to the frontend.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub async fn run_agent_turn(
    app_handle: &tauri::AppHandle,
    provider: &AnyProvider,
    model: &str,
    messages: &mut Vec<Message>,
    tools: &mut Vec<ToolDefinition>,
    session_id: &str,
    run_id: &str,
    max_rounds: u32,
    temperature: Option<f64>,
    pending_approvals: &PendingApprovals,
    tool_timeout_secs: u64,
    agent_id: &str,
    daily_budget_usd: f64,
    daily_tokens: Option<&DailyTokenTracker>,
    thinking_level: Option<&str>,
    auto_approve_all: bool,
) -> EngineResult<String> {
    let mut round = 0;
    let mut final_text = String::new();
    let mut last_input_tokens: u64 = 0;   // Only the LAST round's input (= actual context size)
    let mut total_output_tokens: u64 = 0;  // Sum of all rounds' output tokens

    loop {
        round += 1;
        if round > max_rounds {
            warn!("[engine] Max tool rounds ({}) reached, stopping", max_rounds);
            if final_text.is_empty() {
                final_text = format!(
                    "I completed {} tool-call rounds but ran out of steps before I could \
                    write a final summary.  You can continue the conversation or increase \
                    the max tool rounds in Settings → Engine (currently {}).",
                    max_rounds, max_rounds
                );
                // Emit the fallback text so the frontend shows *something*
                let _ = app_handle.emit("engine-event", EngineEvent::Complete {
                    session_id: session_id.to_string(),
                    run_id: run_id.to_string(),
                    text: final_text.clone(),
                    tool_calls_count: 0,
                    usage: None,
                    model: None,
                });
            }
            return Ok(final_text);
        }

        info!("[engine] Agent round {}/{} session={} run={}", round, max_rounds, session_id, run_id);

        // ── Budget check: stop before making the API call if over daily limit
        if daily_budget_usd > 0.0 {
            if let Some(tracker) = daily_tokens {
                if let Some(spent) = tracker.check_budget(daily_budget_usd) {
                    let msg = format!(
                        "Daily budget exceeded (${:.2} spent, ${:.2} limit). Stopping to prevent further costs. \
                        You can adjust your daily budget in Settings → Engine.",
                        spent, daily_budget_usd
                    );
                    warn!("[engine] {}", msg);
                    let _ = app_handle.emit("engine-event", EngineEvent::Error {
                        session_id: session_id.to_string(),
                        run_id: run_id.to_string(),
                        message: msg.clone(),
                    });
                    return Err(msg.into());
                }
            }
        }

        // ── 1. Call the AI model ──────────────────────────────────────
        let chunks = provider.chat_stream(messages, tools, model, temperature, thinking_level).await?;

        // ── 2. Assemble the response from chunks ──────────────────────
        let mut text_accum = String::new();
        let mut tool_call_map: std::collections::HashMap<usize, (String, String, String, Option<String>, Vec<ThoughtPart>)> = std::collections::HashMap::new();
        // (id, name, arguments, thought_signature, thought_parts)
        let mut has_tool_calls = false;
        let mut _finished = false;

        // Extract the confirmed model name from the API response
        let confirmed_model: Option<String> = chunks.iter().find_map(|c| c.model.clone());

        for chunk in &chunks {
            // Accumulate text deltas
            if let Some(dt) = &chunk.delta_text {
                text_accum.push_str(dt);

                // Emit streaming delta to frontend
                let _ = app_handle.emit("engine-event", EngineEvent::Delta {
                    session_id: session_id.to_string(),
                    run_id: run_id.to_string(),
                    text: dt.clone(),
                });
            }

            // Emit thinking/reasoning text to frontend
            if let Some(tt) = &chunk.thinking_text {
                let _ = app_handle.emit("engine-event", EngineEvent::ThinkingDelta {
                    session_id: session_id.to_string(),
                    run_id: run_id.to_string(),
                    text: tt.clone(),
                });
            }

            // Accumulate tool call deltas
            for tc_delta in &chunk.tool_calls {
                has_tool_calls = true;
                let entry = tool_call_map.entry(tc_delta.index)
                    .or_insert_with(|| (String::new(), String::new(), String::new(), None, Vec::new()));

                if let Some(id) = &tc_delta.id {
                    entry.0 = id.clone();
                }
                if let Some(name) = &tc_delta.function_name {
                    entry.1 = name.clone();
                }
                if let Some(args_delta) = &tc_delta.arguments_delta {
                    entry.2.push_str(args_delta);
                }
                if tc_delta.thought_signature.is_some() {
                    entry.3 = tc_delta.thought_signature.clone();
                }
            }

            // Collect thought parts from chunks that have tool calls
            if !chunk.thought_parts.is_empty() {
                // Attach to the first tool call index
                let first_idx = chunk.tool_calls.first().map(|tc| tc.index).unwrap_or(0);
                let entry = tool_call_map.entry(first_idx)
                    .or_insert_with(|| (String::new(), String::new(), String::new(), None, Vec::new()));
                entry.4.extend(chunk.thought_parts.clone());
            }

            if let Some(reason) = &chunk.finish_reason {
                if reason == "stop" || reason == "end_turn" || reason == "STOP" {
                    _finished = true;
                }
            }

            // Track token usage — input tokens reflect the full context sent
            // each round, so we keep only the LAST round's input tokens (not a sum).
            // Output tokens are truly incremental, so we sum those across rounds.
            if let Some(usage) = &chunk.usage {
                last_input_tokens = usage.input_tokens; // overwrite, not accumulate
                total_output_tokens += usage.output_tokens;
            }
        }

        // Gather cache token usage from all chunks for accurate cost tracking
        let round_cache_read: u64 = chunks.iter()
            .filter_map(|c| c.usage.as_ref())
            .map(|u| u.cache_read_tokens)
            .sum();
        let round_cache_create: u64 = chunks.iter()
            .filter_map(|c| c.usage.as_ref())
            .map(|u| u.cache_creation_tokens)
            .sum();

        // ── Record this round's token usage against the daily budget tracker
        if let Some(tracker) = daily_tokens {
            let round_input = last_input_tokens;
            let round_output = chunks.iter()
                .filter_map(|c| c.usage.as_ref())
                .map(|u| u.output_tokens)
                .sum::<u64>();
            tracker.record(model, round_input, round_output, round_cache_read, round_cache_create);
            let (total_in, total_out, est_usd) = tracker.estimated_spend_usd();
            if round == 1 || round % 5 == 0 {
                info!("[engine] Daily spend: ~${:.2} ({} in / {} out tokens today, cache read={} create={})",
                    est_usd, total_in, total_out, round_cache_read, round_cache_create);
            }

            // ── Budget warnings: emit events at 50%, 75%, 90% thresholds
            if daily_budget_usd > 0.0 {
                if let Some(pct) = tracker.check_budget_warning(daily_budget_usd) {
                    let msg = format!(
                        "Budget warning: {}% of daily budget used (${:.2} of ${:.2})",
                        pct, est_usd, daily_budget_usd
                    );
                    warn!("[engine] {}", msg);
                    let _ = app_handle.emit("engine-event", EngineEvent::Error {
                        session_id: session_id.to_string(),
                        run_id: run_id.to_string(),
                        message: msg,
                    });
                }
            }
        }

        // ── 3. If no tool calls, we're done ──────────────────────────
        if !has_tool_calls || tool_call_map.is_empty() {
            final_text = text_accum.clone();

            // Handle completely empty responses: the model returned nothing.
            // Auto-retry ONCE by injecting a nudge so the model tries again.
            // Use System role to avoid consecutive user messages (Gemini rejects those).
            if final_text.is_empty() && round == 1 && round < max_rounds {
                warn!("[engine] Model returned empty response at round {} — injecting nudge and retrying", round);
                messages.push(Message {
                    role: Role::System,
                    content: MessageContent::Text(
                        "[SYSTEM] The model returned an empty response. Retry the user's request. Use tools if needed."
                            .to_string(),
                    ),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                });
                continue; // retry the loop with the nudge
            }

            // If still empty after the nudge retry (or not round 1), show fallback
            if final_text.is_empty() {
                warn!("[engine] Model returned empty response (0 chars, 0 tool calls) at round {}", round);
                final_text = "I wasn't able to generate a response. This can happen when:\n\
                    - The conversation context is very large (try compacting the session)\n\
                    - A content filter was triggered (try rephrasing)\n\
                    - The model is overwhelmed — try starting a new session\n\n\
                    Please try again or start a new session."
                    .to_string();
            }

            // Add assistant message to history
            messages.push(Message {
                role: Role::Assistant,
                content: MessageContent::Text(text_accum),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });

            // Emit completion event
            let usage = if last_input_tokens > 0 || total_output_tokens > 0 {
                Some(TokenUsage {
                    input_tokens: last_input_tokens,
                    output_tokens: total_output_tokens,
                    total_tokens: last_input_tokens + total_output_tokens,
                    cache_creation_tokens: round_cache_create,
                    cache_read_tokens: round_cache_read,
                })
            } else {
                None
            };
            let _ = app_handle.emit("engine-event", EngineEvent::Complete {
                session_id: session_id.to_string(),
                run_id: run_id.to_string(),
                text: final_text.clone(),
                tool_calls_count: 0,
                usage,
                model: confirmed_model.clone(),
            });

            return Ok(final_text);
        }

        // ── 4. Process tool calls ─────────────────────────────────────
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut sorted_indices: Vec<usize> = tool_call_map.keys().cloned().collect();
        sorted_indices.sort();

        for idx in sorted_indices {
            let (id, name, arguments, thought_sig, thoughts) = tool_call_map.get(&idx).unwrap();

            // Generate ID if provider didn't supply one
            let call_id = if id.is_empty() {
                format!("call_{}", uuid::Uuid::new_v4())
            } else {
                id.clone()
            };

            tool_calls.push(ToolCall {
                id: call_id.clone(),
                call_type: "function".into(),
                function: FunctionCall {
                    name: name.clone(),
                    arguments: arguments.clone(),
                },
                thought_signature: thought_sig.clone(),
                thought_parts: thoughts.clone(),
            });
        }

        // Add assistant message with tool calls to history
        messages.push(Message {
            role: Role::Assistant,
            content: MessageContent::Text(text_accum),
            tool_calls: Some(tool_calls.clone()),
            tool_call_id: None,
            name: None,
        });

        // ── 5. Execute each tool call (with HIL approval) ──────────────
        let tc_count = tool_calls.len();
        for tc in &tool_calls {
            info!("[engine] Tool call: {} id={}", tc.function.name, tc.id);

            // Per-tool autonomy: auto-approve read-only/informational tools,
            // require user approval for dangerous, side-effect-heavy, or financial tools.
            let auto_approved_tools: &[&str] = &[
                // ── Read-only / informational ──
                "fetch", "read_file", "list_directory",
                "soul_read", "soul_list", "memory_search", "self_info",
                "web_search", "web_read", "web_screenshot", "web_browse",
                "list_tasks", "email_read", "slack_read", "telegram_read",
                // ── Agent memory / profile ──
                "soul_write", "memory_store", "update_profile",
                // ── Task management ──
                "create_task", "manage_task",
                // ── Trading: read-only (balances, quotes, portfolio, info) ──
                "sol_balance", "sol_quote", "sol_portfolio", "sol_token_info",
                "dex_balance", "dex_quote", "dex_portfolio", "dex_token_info",
                "dex_check_token", "dex_search_token", "dex_watch_wallet",
                "dex_whale_transfers", "dex_top_traders", "dex_trending",
                "coinbase_prices", "coinbase_balance",
                // ── Media ──
                "image_generate",
                // ── Agent Management (read/assign skills) ──
                "agent_list", "agent_skills", "agent_skill_assign",
                // ── Community Skills (safe: only fetch/install/list) ──
                "skill_search", "skill_install", "skill_list",
                // ── Inter-agent comms (safe: only sends/reads msgs between agents) ──
                "agent_send_message", "agent_read_messages",
                // ── Squads (safe: team management) ──
                "create_squad", "list_squads", "manage_squad", "squad_broadcast",
                // ── Tool RAG (safe: only searches tool index, loads tools) ──
                "request_tools",
            ];

            // Trading write tools check the policy-based approval function
            let trading_write_tools = [
                "sol_swap", "sol_transfer", "sol_wallet_create",
                "dex_swap", "dex_transfer", "dex_wallet_create",
                "coinbase_trade", "coinbase_transfer", "coinbase_wallet_create",
            ];

            let skip_hil = if auto_approve_all || auto_approved_tools.contains(&tc.function.name.as_str()) {
                true
            } else if trading_write_tools.contains(&tc.function.name.as_str()) {
                check_trading_auto_approve(&tc.function.name, &tc.function.arguments, app_handle)
            } else {
                false
            };

            let approved = if skip_hil {
                // Distinguish agent-level auto-approve from safe-tool auto-approve in logs
                if auto_approve_all && !auto_approved_tools.contains(&tc.function.name.as_str()) {
                    info!("[engine] Tool auto-approved (agent policy): {}", tc.function.name);
                    // Emit audit event so frontend can track agent-policy approvals
                    let _ = app_handle.emit("engine-event", EngineEvent::ToolAutoApproved {
                        session_id: session_id.to_string(),
                        run_id: run_id.to_string(),
                        tool_name: tc.function.name.clone(),
                        tool_call_id: tc.id.clone(),
                    });
                } else {
                    info!("[engine] Auto-approved safe tool: {}", tc.function.name);
                }
                true
            } else {
                info!("[engine] Tool requires user approval: {}", tc.function.name);
                // Register a oneshot channel for approval
                let (approval_tx, approval_rx) = tokio::sync::oneshot::channel::<bool>();
                {
                    let mut map = pending_approvals.lock();
                    map.insert(tc.id.clone(), approval_tx);
                }

                // Emit tool request event — frontend will show approval modal
                let _ = app_handle.emit("engine-event", EngineEvent::ToolRequest {
                    session_id: session_id.to_string(),
                    run_id: run_id.to_string(),
                    tool_call: tc.clone(),
                });

                // Wait for user approval (with timeout)
                let timeout_duration = Duration::from_secs(tool_timeout_secs);
                match tokio::time::timeout(timeout_duration, approval_rx).await {
                    Ok(Ok(allowed)) => allowed,
                    Ok(Err(_)) => {
                        warn!("[engine] Approval channel closed for {}", tc.id);
                        false
                    }
                    Err(_) => {
                        warn!("[engine] Approval timeout ({}s) for tool {}", tool_timeout_secs, tc.function.name);
                        // Clean up the pending entry
                        let mut map = pending_approvals.lock();
                        map.remove(&tc.id);
                        false
                    }
                }
            };

            if !approved {
                info!("[engine] Tool DENIED by user: {} id={}", tc.function.name, tc.id);

                // Emit denial as tool result
                let _ = app_handle.emit("engine-event", EngineEvent::ToolResultEvent {
                    session_id: session_id.to_string(),
                    run_id: run_id.to_string(),
                    tool_call_id: tc.id.clone(),
                    output: "Tool execution denied by user.".into(),
                    success: false,
                });

                // Add denial to message history so the model knows
                messages.push(Message {
                    role: Role::Tool,
                    content: MessageContent::Text("Tool execution denied by user.".into()),
                    tool_calls: None,
                    tool_call_id: Some(tc.id.clone()),
                    name: Some(tc.function.name.clone()),
                });
                continue;
            }

            // Execute the tool (pass agent_id so tools know which agent is calling)
            let result = tools::execute_tool(tc, app_handle, agent_id).await;

            info!("[engine] Tool result: {} success={} output_len={}",
                tc.function.name, result.success, result.output.len());

            // Emit tool result event
            let _ = app_handle.emit("engine-event", EngineEvent::ToolResultEvent {
                session_id: session_id.to_string(),
                run_id: run_id.to_string(),
                tool_call_id: tc.id.clone(),
                output: result.output.clone(),
                success: result.success,
            });

            // Add tool result to message history
            messages.push(Message {
                role: Role::Tool,
                content: MessageContent::Text(result.output),
                tool_calls: None,
                tool_call_id: Some(tc.id.clone()),
                name: Some(tc.function.name.clone()),
            });
        }

        // ── 6. Tool RAG: refresh tools if request_tools was called ─────
        // When the agent calls request_tools, new tool names are added to
        // state.loaded_tools. We need to inject those new ToolDefinitions
        // into the active tool list so the agent can use them in the next round.
        if let Some(state) = app_handle.try_state::<crate::engine::state::EngineState>() {
            let loaded = state.loaded_tools.lock().clone();
            let current_names: std::collections::HashSet<String> = tools.iter()
                .map(|t| t.function.name.clone())
                .collect();
            let new_names: Vec<String> = loaded.difference(&current_names)
                .cloned()
                .collect();
            if !new_names.is_empty() {
                // Build the full tool registry to find the definitions
                let mut all_defs = ToolDefinition::builtins();
                let enabled_ids: Vec<String> = crate::engine::skills::builtin_skills()
                    .iter()
                    .filter(|s| state.store.is_skill_enabled(&s.id).unwrap_or(false))
                    .map(|s| s.id.clone())
                    .collect();
                all_defs.extend(ToolDefinition::skill_tools(&enabled_ids));

                let mut added = 0;
                for def in all_defs {
                    if new_names.contains(&def.function.name) {
                        info!("[tool-rag] Hot-loading tool '{}' into active round", def.function.name);
                        tools.push(def);
                        added += 1;
                    }
                }
                if added > 0 {
                    info!("[tool-rag] Injected {} new tools into active tool list (now {} total)",
                        added, tools.len());
                }
            }
        }

        // ── 7. Mid-loop context truncation ─────────────────────────────
        // The messages Vec grows each round (assistant + tool results).
        // Without trimming, later rounds can send 50k+ tokens to the API.
        // Uses the same context_window_tokens from Settings → Engine as
        // the initial conversation load (default 32K).
        // Always preserves: system prompt (first msg) and last user message.
        let mid_loop_max = {
            if let Some(state) = app_handle.try_state::<crate::engine::state::EngineState>() {
                let cfg = state.config.lock();
                cfg.context_window_tokens
            } else {
                32_000
            }
        };
        let estimate_msg_tokens = |m: &Message| -> usize {
            let text_len = match &m.content {
                MessageContent::Text(t) => t.len(),
                MessageContent::Blocks(blocks) => blocks.iter().map(|b| match b {
                    ContentBlock::Text { text } => text.len(),
                    ContentBlock::ImageUrl { .. } => 1000,
                    ContentBlock::Document { data, .. } => data.len() / 4,
                }).sum(),
            };
            let tc_len = m.tool_calls.as_ref().map(|tcs| {
                tcs.iter().map(|tc2| tc2.function.arguments.len() + tc2.function.name.len() + 20).sum::<usize>()
            }).unwrap_or(0);
            (text_len + tc_len) / 4 + 4
        };
        let mid_total: usize = messages.iter().map(&estimate_msg_tokens).sum();
        if mid_total > mid_loop_max && messages.len() > 3 {
            // Preserve system prompt (index 0)
            let sys_msg = if !messages.is_empty() && messages[0].role == Role::System {
                Some(messages.remove(0))
            } else {
                None
            };
            let sys_tokens = sys_msg.as_ref().map(&estimate_msg_tokens).unwrap_or(0);
            let msg_tokens: Vec<usize> = messages.iter().map(&estimate_msg_tokens).collect();
            let mut running = sys_tokens + msg_tokens.iter().sum::<usize>();
            // Find last user message — never drop past it
            let last_user_idx = messages.iter().rposition(|m| m.role == Role::User)
                .unwrap_or(messages.len().saturating_sub(1));
            let mut keep_from = 0;
            for (i, &t) in msg_tokens.iter().enumerate() {
                if running <= mid_loop_max { break; }
                if i >= last_user_idx { break; }
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
                info!("[engine] Mid-loop truncation: {} → {} est tokens, {} messages kept",
                    mid_total, running, messages.len());
            } else if let Some(sys) = sys_msg {
                messages.insert(0, sys);
            }
        }

        // ── 8. Loop: send tool results back to model ──────────────────
        info!("[engine] {} tool calls executed, feeding results back to model", tc_count);

        // NOTE: Do NOT emit Complete here — only emit Complete when the model
        // produces a final text response (no more tool calls). Intermediate
        // Complete events were causing premature stream resolution on the frontend.

        // Continue the loop — model will see tool results and either respond or call more tools
    }
}
