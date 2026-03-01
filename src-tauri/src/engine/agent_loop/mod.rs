// Paw Agent Engine — Agentic Loop
// The core orchestration loop: send to model → tool calls → execute → repeat.
// This is the core agent loop that drives Pawz AI interactions.

mod helpers;
mod trading;

use crate::atoms::error::EngineResult;
use crate::engine::providers::AnyProvider;
use crate::engine::state::{DailyTokenTracker, PendingApprovals};
use crate::engine::tools;
use crate::engine::types::*;
use log::{info, warn};
use std::time::Duration;
use tauri::Emitter;
use trading::check_trading_auto_approve;

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
    user_approved_tools: &[String],
    yield_signal: Option<&crate::engine::state::YieldSignal>,
) -> EngineResult<String> {
    let mut round = 0;
    let mut final_text = String::new();
    let mut last_input_tokens: u64 = 0; // Only the LAST round's input (= actual context size)
    let mut total_output_tokens: u64 = 0; // Sum of all rounds' output tokens

    // Circuit breaker: track consecutive failures per tool name.
    // After MAX_CONSECUTIVE_TOOL_FAILS of the same tool, inject a system nudge.
    // After HARD_STOP_TOOL_FAILS, block further execution of that tool entirely.
    let mut tool_fail_counter: std::collections::HashMap<String, u32> =
        std::collections::HashMap::new();
    const MAX_CONSECUTIVE_TOOL_FAILS: u32 = 3;
    const HARD_STOP_TOOL_FAILS: u32 = 5;

    // Repetition detector: track the tool-call "signature" (hashed tool names
    // + args) for each round.  If the same signature appears consecutively
    // MAX_REPEATED_SIGNATURES times, the model is stuck in a tool-calling loop
    // (common after model/context changes mid-conversation).
    let mut round_signatures: Vec<u64> = Vec::new();
    const MAX_REPEATED_SIGNATURES: usize = 3;

    loop {
        round += 1;

        // ── Yield check: if a new user message was queued, wrap up gracefully ─
        // VS Code pattern: when yield is requested, the agent stops its loop
        // and returns whatever it has so far.  The queued message will be
        // processed next by the request queue handler.
        if let Some(ys) = yield_signal {
            if ys.is_yield_requested() {
                warn!(
                    "[engine] Yield requested — wrapping up agent turn at round {}",
                    round
                );
                if final_text.is_empty() {
                    final_text = "I was wrapping up to handle your new message. \
                        My previous work may be incomplete."
                        .to_string();
                }
                let _ = app_handle.emit(
                    "engine-event",
                    EngineEvent::Complete {
                        session_id: session_id.to_string(),
                        run_id: run_id.to_string(),
                        text: final_text.clone(),
                        tool_calls_count: 0,
                        usage: None,
                        model: None,
                    },
                );
                return Ok(final_text);
            }
        }

        if round > max_rounds {
            warn!(
                "[engine] Max tool rounds ({}) reached, stopping",
                max_rounds
            );
            if final_text.is_empty() {
                final_text = format!(
                    "I completed {} tool-call rounds but ran out of steps before I could \
                    write a final summary.  You can continue the conversation or increase \
                    the max tool rounds in Settings → Engine (currently {}).",
                    max_rounds, max_rounds
                );
                // Emit the fallback text so the frontend shows *something*
                let _ = app_handle.emit(
                    "engine-event",
                    EngineEvent::Complete {
                        session_id: session_id.to_string(),
                        run_id: run_id.to_string(),
                        text: final_text.clone(),
                        tool_calls_count: 0,
                        usage: None,
                        model: None,
                    },
                );
            }
            return Ok(final_text);
        }

        info!(
            "[engine] Agent round {}/{} session={} run={}",
            round, max_rounds, session_id, run_id
        );

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
                    let _ = app_handle.emit(
                        "engine-event",
                        EngineEvent::Error {
                            session_id: session_id.to_string(),
                            run_id: run_id.to_string(),
                            message: msg.clone(),
                        },
                    );
                    return Err(msg.into());
                }
            }
        }

        // ── 1. Call the AI model ──────────────────────────────────────
        let chunks = provider
            .chat_stream(messages, tools, model, temperature, thinking_level)
            .await?;

        // ── 2. Assemble the response from chunks ──────────────────────
        let mut text_accum = String::new();
        let mut tool_call_map: std::collections::HashMap<
            usize,
            (String, String, String, Option<String>, Vec<ThoughtPart>),
        > = std::collections::HashMap::new();
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
                let _ = app_handle.emit(
                    "engine-event",
                    EngineEvent::Delta {
                        session_id: session_id.to_string(),
                        run_id: run_id.to_string(),
                        text: dt.clone(),
                    },
                );
            }

            // Emit thinking/reasoning text to frontend
            if let Some(tt) = &chunk.thinking_text {
                let _ = app_handle.emit(
                    "engine-event",
                    EngineEvent::ThinkingDelta {
                        session_id: session_id.to_string(),
                        run_id: run_id.to_string(),
                        text: tt.clone(),
                    },
                );
            }

            // Accumulate tool call deltas
            for tc_delta in &chunk.tool_calls {
                has_tool_calls = true;
                let entry = tool_call_map.entry(tc_delta.index).or_insert_with(|| {
                    (
                        String::new(),
                        String::new(),
                        String::new(),
                        None,
                        Vec::new(),
                    )
                });

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
                let entry = tool_call_map.entry(first_idx).or_insert_with(|| {
                    (
                        String::new(),
                        String::new(),
                        String::new(),
                        None,
                        Vec::new(),
                    )
                });
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
        let round_cache_read: u64 = chunks
            .iter()
            .filter_map(|c| c.usage.as_ref())
            .map(|u| u.cache_read_tokens)
            .sum();
        let round_cache_create: u64 = chunks
            .iter()
            .filter_map(|c| c.usage.as_ref())
            .map(|u| u.cache_creation_tokens)
            .sum();

        // ── Record this round's token usage against the daily budget tracker
        if let Some(tracker) = daily_tokens {
            let round_input = last_input_tokens;
            let round_output = chunks
                .iter()
                .filter_map(|c| c.usage.as_ref())
                .map(|u| u.output_tokens)
                .sum::<u64>();
            tracker.record(
                model,
                round_input,
                round_output,
                round_cache_read,
                round_cache_create,
            );
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
                    let _ = app_handle.emit(
                        "engine-event",
                        EngineEvent::Error {
                            session_id: session_id.to_string(),
                            run_id: run_id.to_string(),
                            message: msg,
                        },
                    );
                }
            }
        }

        // ── 3. If no tool calls, we're done ──────────────────────────
        if !has_tool_calls || tool_call_map.is_empty() {
            final_text = text_accum.clone();

            // Retry on malformed tool calls (Gemini JSON issues)
            if helpers::handle_malformed_tool_call(&final_text, messages, round, max_rounds) {
                continue;
            }

            // Retry on empty response (nudge with user recap)
            if helpers::handle_empty_response(&final_text, messages, round, max_rounds) {
                continue;
            }

            // Persistent empty → fallback message
            if final_text.is_empty() {
                warn!(
                    "[engine] Model returned empty response (0 chars, 0 tool calls) at round {}",
                    round
                );
                final_text = helpers::empty_response_fallback();
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
            let _ = app_handle.emit(
                "engine-event",
                EngineEvent::Complete {
                    session_id: session_id.to_string(),
                    run_id: run_id.to_string(),
                    text: final_text.clone(),
                    tool_calls_count: 0,
                    usage,
                    model: confirmed_model.clone(),
                },
            );

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

        // ── Repetition detector: break tool-calling loops ──────────────
        // Hash the sorted tool names + full args into a u64 fingerprint.
        // If the same fingerprint appears MAX_REPEATED_SIGNATURES times
        // consecutively, the model is stuck repeating the same tool calls
        // (common when model or context is changed mid-conversation).
        // Uses a hash to avoid UTF-8 boundary issues and keep memory flat.
        {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let mut sig_parts: Vec<(&str, &str)> = tool_calls
                .iter()
                .map(|tc| (tc.function.name.as_str(), tc.function.arguments.as_str()))
                .collect();
            sig_parts.sort();

            let mut hasher = DefaultHasher::new();
            for (name, args) in &sig_parts {
                name.hash(&mut hasher);
                args.hash(&mut hasher);
            }
            let signature = hasher.finish();
            round_signatures.push(signature);

            // Check the last N signatures for consecutive repetition
            let sig_len = round_signatures.len();
            if sig_len >= MAX_REPEATED_SIGNATURES {
                let all_same = round_signatures[sig_len - MAX_REPEATED_SIGNATURES..]
                    .iter()
                    .all(|&s| s == signature);
                if all_same {
                    // Check whether we (or detect_response_loop) already
                    // injected a loop/redirect message. If so, the model
                    // ignored the first nudge — hard-break to prevent
                    // unbounded redirect stacking.
                    let already_redirected = messages.iter().any(|m| {
                        m.role == Role::System && {
                            let t = m.content.as_text_ref();
                            t.contains("stuck in a tool-calling loop")
                                || t.contains("stuck in a response loop")
                                || t.contains("stuck repeating yourself")
                                || t.contains("TOPIC CHANGE")
                                || t.contains("stuck asking clarifying questions")
                        }
                    });
                    if already_redirected {
                        warn!(
                            "[engine] Model ignored tool-loop redirect — hard-breaking agent turn"
                        );
                        messages.pop(); // remove the repeated assistant message
                        return Ok(
                            "I was stuck calling the same tools repeatedly and couldn't make \
                            progress. Please try rephrasing your request or switching context."
                                .to_string(),
                        );
                    }

                    warn!(
                        "[engine] Tool-call loop detected: same tool signature repeated {} times — injecting redirect",
                        MAX_REPEATED_SIGNATURES
                    );
                    // Remove the assistant message we just pushed (it has the repeated tools)
                    messages.pop();
                    // Inject a redirect message
                    messages.push(Message {
                        role: Role::System,
                        content: MessageContent::Text(
                            "[SYSTEM] You are stuck in a tool-calling loop — you have called the \
                            same tools with the same arguments multiple times in a row. STOP calling \
                            tools and provide a direct text response to the user summarizing what you \
                            have accomplished and any issues encountered. Do NOT make any more tool calls."
                                .to_string(),
                        ),
                        tool_calls: None,
                        tool_call_id: None,
                        name: None,
                    });
                    continue; // Go back to model call — it should now produce text
                }
            }
        }

        // ── 5. Execute each tool call (with HIL approval) ──────────────
        let tc_count = tool_calls.len();
        for tc in &tool_calls {
            info!("[engine] Tool call: {} id={}", tc.function.name, tc.id);

            // Per-tool autonomy: auto-approve read-only/informational tools,
            // require user approval for dangerous, side-effect-heavy, or financial tools.
            let auto_approved_tools: &[&str] = &[
                // ── Read-only / informational ──
                "fetch",
                "read_file",
                "list_directory",
                "soul_read",
                "soul_list",
                "memory_search",
                "self_info",
                "web_search",
                "web_read",
                "web_screenshot",
                "web_browse",
                "list_tasks",
                "email_read",
                "email_send", // redirects to google_gmail_send internally
                "slack_read",
                "telegram_read",
                // ── Agent memory / profile ──
                "soul_write",
                "memory_store",
                "memory_knowledge",
                "memory_stats",
                "update_profile",
                // ── Task management ──
                "create_task",
                "manage_task",
                // ── Google Workspace: read-only (list, read, search) ──
                "google_gmail_list",
                "google_gmail_read",
                "google_calendar_list",
                "google_drive_list",
                "google_drive_read",
                "google_sheets_read",
                // ── Google Workspace: write (create docs, upload, share, send) ──
                "google_docs_create",
                "google_drive_upload",
                "google_drive_share",
                "google_gmail_send",
                "google_calendar_create",
                "google_sheets_append",
                "google_api",
                // ── Trading: read-only (balances, quotes, portfolio, info) ──
                "sol_balance",
                "sol_quote",
                "sol_portfolio",
                "sol_token_info",
                "dex_balance",
                "dex_quote",
                "dex_portfolio",
                "dex_token_info",
                "dex_check_token",
                "dex_search_token",
                "dex_watch_wallet",
                "dex_whale_transfers",
                "dex_top_traders",
                "dex_trending",
                "coinbase_prices",
                "coinbase_balance",
                // ── Media ──
                "image_generate",
                // ── Agent Management (read/assign skills) ──
                "agent_list",
                "agent_skills",
                "agent_skill_assign",
                // ── Community Skills (safe: only fetch/install/list) ──
                "skill_search",
                "skill_install",
                "skill_list",
                // ── Inter-agent comms (safe: only sends/reads msgs between agents) ──
                "agent_send_message",
                "agent_read_messages",
                // ── Squads (safe: team management) ──
                "create_squad",
                "list_squads",
                "manage_squad",
                "squad_broadcast",
                // ── Tool RAG (safe: only searches tool index, loads tools) ──
                "request_tools",
                // ── n8n / MCP (refresh tool list — read-only discovery) ──
                "mcp_refresh",
                "search_ncnodes",
                "n8n_list_workflows",
                // ── Trello (project management — boards, lists, cards) ──
                "trello_list_boards",
                "trello_get_board",
                "trello_create_board",
                "trello_update_board",
                "trello_get_lists",
                "trello_create_list",
                "trello_update_list",
                "trello_archive_list",
                "trello_get_cards",
                "trello_create_card",
                "trello_get_card",
                "trello_update_card",
                "trello_move_card",
                "trello_add_comment",
                "trello_search",
                "trello_get_labels",
                "trello_create_label",
                "trello_update_label",
                "trello_add_label",
                "trello_remove_label",
                "trello_create_checklist",
                "trello_add_checklist_item",
                "trello_toggle_checklist_item",
                "trello_get_members",
            ];

            // Trading write tools check the policy-based approval function
            let trading_write_tools = [
                "sol_swap",
                "sol_transfer",
                "sol_wallet_create",
                "dex_swap",
                "dex_transfer",
                "dex_wallet_create",
                "coinbase_trade",
                "coinbase_transfer",
                "coinbase_wallet_create",
            ];

            // ── Circuit breaker: block tools that already hit HARD_STOP ──
            if let Some(count) = tool_fail_counter.get(&tc.function.name) {
                if *count >= HARD_STOP_TOOL_FAILS {
                    warn!(
                        "[engine] Circuit breaker: blocking '{}' (already failed {} times)",
                        tc.function.name, count
                    );
                    messages.push(Message {
                        role: Role::Tool,
                        content: MessageContent::Text(format!(
                            "Error: Tool '{}' is blocked after {} consecutive failures. Use a different tool or tell the user.",
                            tc.function.name, count
                        )),
                        tool_calls: None,
                        tool_call_id: Some(tc.id.clone()),
                        name: Some(tc.function.name.clone()),
                    });
                    continue;
                }
            }

            let skip_hil = if auto_approve_all
                || auto_approved_tools.contains(&tc.function.name.as_str())
                || user_approved_tools.iter().any(|t| t == &tc.function.name)
            {
                true
            } else if trading_write_tools.contains(&tc.function.name.as_str()) {
                check_trading_auto_approve(&tc.function.name, &tc.function.arguments, app_handle)
            } else {
                false
            };

            let approved = if skip_hil {
                // Distinguish agent-level auto-approve from safe-tool auto-approve in logs
                if auto_approve_all && !auto_approved_tools.contains(&tc.function.name.as_str()) {
                    info!(
                        "[engine] Tool auto-approved (agent policy): {}",
                        tc.function.name
                    );
                    // Emit audit event so frontend can track agent-policy approvals
                    let _ = app_handle.emit(
                        "engine-event",
                        EngineEvent::ToolAutoApproved {
                            session_id: session_id.to_string(),
                            run_id: run_id.to_string(),
                            tool_name: tc.function.name.clone(),
                            tool_call_id: tc.id.clone(),
                        },
                    );
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
                let _ = app_handle.emit(
                    "engine-event",
                    EngineEvent::ToolRequest {
                        session_id: session_id.to_string(),
                        run_id: run_id.to_string(),
                        tool_call: tc.clone(),
                    },
                );

                // Wait for user approval (with timeout)
                let timeout_duration = Duration::from_secs(tool_timeout_secs);
                match tokio::time::timeout(timeout_duration, approval_rx).await {
                    Ok(Ok(allowed)) => allowed,
                    Ok(Err(_)) => {
                        warn!("[engine] Approval channel closed for {}", tc.id);
                        false
                    }
                    Err(_) => {
                        warn!(
                            "[engine] Approval timeout ({}s) for tool {}",
                            tool_timeout_secs, tc.function.name
                        );
                        // Clean up the pending entry
                        let mut map = pending_approvals.lock();
                        map.remove(&tc.id);
                        false
                    }
                }
            };

            if !approved {
                info!(
                    "[engine] Tool DENIED by user: {} id={}",
                    tc.function.name, tc.id
                );

                // Emit denial as tool result
                let _ = app_handle.emit(
                    "engine-event",
                    EngineEvent::ToolResultEvent {
                        session_id: session_id.to_string(),
                        run_id: run_id.to_string(),
                        tool_call_id: tc.id.clone(),
                        output: "Tool execution denied by user.".into(),
                        success: false,
                    },
                );

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

            info!(
                "[engine] Tool result: {} success={} output_len={}",
                tc.function.name,
                result.success,
                result.output.len()
            );

            // Emit tool result event
            let _ = app_handle.emit(
                "engine-event",
                EngineEvent::ToolResultEvent {
                    session_id: session_id.to_string(),
                    run_id: run_id.to_string(),
                    tool_call_id: tc.id.clone(),
                    output: result.output.clone(),
                    success: result.success,
                },
            );

            // Add tool result to message history
            messages.push(Message {
                role: Role::Tool,
                content: MessageContent::Text(result.output.clone()),
                tool_calls: None,
                tool_call_id: Some(tc.id.clone()),
                name: Some(tc.function.name.clone()),
            });

            // ── Circuit breaker: track consecutive failures per tool ──
            if !result.success {
                let count = tool_fail_counter
                    .entry(tc.function.name.clone())
                    .or_insert(0);
                *count += 1;
                if *count >= HARD_STOP_TOOL_FAILS {
                    warn!(
                        "[engine] Circuit breaker HARD STOP: tool '{}' failed {} consecutive times. Blocking further calls.",
                        tc.function.name, count
                    );
                    messages.push(Message {
                        role: Role::System,
                        content: MessageContent::Text(format!(
                            "[SYSTEM] HARD STOP: The tool '{}' has failed {} times in a row and is now BLOCKED. \
                            Do NOT call '{}' again — it will not work. \
                            Instead, tell the user what happened and suggest they check their \
                            skill configuration or try a different approach. Provide a text summary now.",
                            tc.function.name, count, tc.function.name
                        )),
                        tool_calls: None,
                        tool_call_id: None,
                        name: None,
                    });
                } else if *count >= MAX_CONSECUTIVE_TOOL_FAILS {
                    warn!(
                        "[engine] Circuit breaker: tool '{}' failed {} consecutive times. Injecting stop-retry nudge.",
                        tc.function.name, count
                    );
                    messages.push(Message {
                        role: Role::System,
                        content: MessageContent::Text(format!(
                            "[SYSTEM] The tool '{}' has failed {} times in a row. \
                            Stop calling '{}' with the same arguments — try a DIFFERENT tool or approach instead. \
                            Use `request_tools` to discover alternative tools that might work better. \
                            For example, if google_api failed, try dedicated tools like google_docs_create, \
                            google_drive_upload, or google_drive_share instead.",
                            tc.function.name, count, tc.function.name
                        )),
                        tool_calls: None,
                        tool_call_id: None,
                        name: None,
                    });
                }
            } else {
                // Reset counter on success
                tool_fail_counter.remove(&tc.function.name);
            }
        }

        // ── 6. Tool RAG: refresh tools if request_tools was called ─────
        helpers::refresh_tool_rag(app_handle, tools);

        // ── 7. Mid-loop context truncation ─────────────────────────────
        helpers::truncate_mid_loop(app_handle, messages);

        // ── 8. Loop: send tool results back to model ──────────────────
        info!(
            "[engine] {} tool calls executed, feeding results back to model",
            tc_count
        );

        // NOTE: Do NOT emit Complete here — only emit Complete when the model
        // produces a final text response (no more tool calls). Intermediate
        // Complete events were causing premature stream resolution on the frontend.

        // Continue the loop — model will see tool results and either respond or call more tools
    }
}
