// Paw Agent Engine — Agentic Loop
// The core orchestration loop: send to model → tool calls → execute → repeat.
// This is the core agent loop that drives Pawz AI interactions.

pub(crate) mod helpers;
mod trading;

use crate::atoms::error::EngineResult;
use crate::engine::providers::AnyProvider;
use crate::engine::state::{DailyTokenTracker, PendingApprovals};
use crate::engine::telemetry::{integration as telem, RunCollector};
use crate::engine::tools;
use crate::engine::types::*;
use log::{info, warn};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};
use trading::check_trading_auto_approve;

/// Emit an engine event to both the Tauri webview frontend AND any SSE
/// subscribers (e.g. the Pawz VS Code extension via `/chat/stream`).
/// All `engine-event` emissions must go through this instead of calling
/// `app_handle.emit()` directly so SSE clients receive live streaming events.
fn fire(app: &tauri::AppHandle, event: EngineEvent) {
    let _ = app.emit("engine-event", &event);
    if let Some(es) = app.try_state::<crate::engine::state::EngineState>() {
        if let Ok(json) = serde_json::to_string(&event) {
            let _ = es.sse_events.send(json);
        }
    }
}

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
    let mut total_cache_read: u64 = 0; // Sum of all rounds' cache read tokens
    let mut total_cache_create: u64 = 0; // Sum of all rounds' cache creation tokens

    // ── Telemetry: per-turn collector (Canvas Phase 5) ────────────────
    let mut telem_collector = RunCollector::new(session_id, run_id, model);
    let telem_root_id = telem_collector.root_span("agent_turn");
    let turn_start = Instant::now();
    let mut tool_duration_total_ms: u64 = 0;
    let mut tool_call_count: u32 = 0;

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

    // ── Phase 3: Binary IPC delta batcher ─────────────────────────────
    // Batches streaming text deltas before emitting IPC events to the frontend.
    // Reduces per-token IPC overhead for fast models (50-100 tokens/sec).
    let mut delta_batcher = crate::engine::binary_ipc::EventBatcher::new(
        session_id,
        run_id,
        crate::engine::binary_ipc::BatchConfig::default(),
    );

    // ── Phase 4: Speculative tool execution tracking ──────────────────
    // Track the previously-executed tool name so the speculative engine can
    // record A→B transitions and predict the next tool call.
    let mut previous_tool: Option<String> = None;
    // Load speculation config and stats tracker from engine state
    let speculation_config = app_handle
        .try_state::<crate::engine::state::EngineState>()
        .map(|es| es.speculation_config.clone())
        .unwrap_or_default();
    let mut speculation_stats = crate::engine::speculative::SpeculationStats::default();

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
                fire(
                    app_handle,
                    EngineEvent::Complete {
                        session_id: session_id.to_string(),
                        run_id: run_id.to_string(),
                        text: final_text.clone(),
                        tool_calls_count: 0,
                        usage: None,
                        model: None,
                        total_rounds: Some(round),
                        max_rounds: Some(max_rounds),
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
                fire(
                    app_handle,
                    EngineEvent::Complete {
                        session_id: session_id.to_string(),
                        run_id: run_id.to_string(),
                        text: final_text.clone(),
                        tool_calls_count: 0,
                        usage: None,
                        model: None,
                        total_rounds: Some(round),
                        max_rounds: Some(max_rounds),
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
                        "Daily budget exceeded (${:.2} spent of ${:.2} limit). \
                        To continue, go to Settings → Advanced → Daily Budget and increase or clear the limit.",
                        spent, daily_budget_usd
                    );
                    warn!("[engine] {}", msg);
                    fire(
                        app_handle,
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

                // Phase 3: Batch deltas before emitting to reduce IPC overhead.
                // push_delta returns Some(batch) when flush is needed.
                if let Some(batch) = delta_batcher.push_delta(dt) {
                    fire(
                        app_handle,
                        EngineEvent::Delta {
                            session_id: session_id.to_string(),
                            run_id: run_id.to_string(),
                            text: batch.combined_text,
                        },
                    );
                }
            }

            // Emit thinking/reasoning text to frontend
            if let Some(tt) = &chunk.thinking_text {
                fire(
                    app_handle,
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
                    entry.0.push_str(id);
                }
                if let Some(name) = &tc_delta.function_name {
                    entry.1.push_str(name);
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

        // Accumulate cache totals across rounds
        total_cache_read += round_cache_read;
        total_cache_create += round_cache_create;

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
                    fire(
                        app_handle,
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
            // Skip retry when constrained decoding is active — the parse failure
            // indicates a deeper issue, not a model formatting mistake.
            let constraint_level =
                crate::engine::constrained::detect_constraints(provider.kind(), model).level;
            let constrained_active =
                constraint_level != crate::engine::constrained::ConstraintLevel::None;
            if !constrained_active
                && helpers::handle_malformed_tool_call(&final_text, messages, round, max_rounds)
            {
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

            // ── Phase 3: Flush remaining batched deltas BEFORE Complete ──
            // Must happen first so the streaming bubble on the frontend shows
            // the full text before lifecycle:end resolves the stream promise.
            // Previously this flush happened after Complete, causing the
            // streaming preview to briefly show truncated content.
            if let Some(batch) = delta_batcher.flush() {
                fire(
                    app_handle,
                    EngineEvent::Delta {
                        session_id: session_id.to_string(),
                        run_id: run_id.to_string(),
                        text: batch.combined_text,
                    },
                );
            }
            // Log binary IPC batcher stats for the session
            crate::engine::binary_ipc::log_session_stats(&delta_batcher.stats(), 0);

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
            fire(
                app_handle,
                EngineEvent::Complete {
                    session_id: session_id.to_string(),
                    run_id: run_id.to_string(),
                    text: final_text.clone(),
                    tool_calls_count: 0,
                    usage,
                    model: confirmed_model.clone(),
                    total_rounds: Some(round),
                    max_rounds: Some(max_rounds),
                },
            );

            // ── Telemetry flush (Canvas Phase 5) ──────────────────────
            {
                let total_ms = turn_start.elapsed().as_millis() as u64;
                let llm_ms = total_ms.saturating_sub(tool_duration_total_ms);
                let mut summary = telem_collector.build_summary(
                    last_input_tokens,
                    total_output_tokens,
                    round,
                    tool_call_count,
                );
                summary.total_duration_ms = total_ms;
                summary.llm_duration_ms = llm_ms;
                summary.tool_duration_ms = tool_duration_total_ms;
                summary.cost_usd = crate::engine::types::estimate_cost_usd(
                    model,
                    last_input_tokens,
                    total_output_tokens,
                    total_cache_read,
                    total_cache_create,
                );

                if let Some(es) = app_handle.try_state::<crate::engine::state::EngineState>() {
                    telem::persist_summary(&es.store, &summary);
                }
                telem::emit_summary(app_handle, &summary);
            }

            // ── Phase 4: Log speculation stats for the session ────────
            crate::engine::speculative::log_session_speculation_stats(&speculation_stats);

            return Ok(final_text);
        }

        // ── 4. Process tool calls ─────────────────────────────────────
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut sorted_indices: Vec<usize> = tool_call_map.keys().cloned().collect();
        sorted_indices.sort();

        for idx in sorted_indices {
            let entry = match tool_call_map.get(&idx) {
                Some(e) => e,
                None => continue, // skip missing indices (shouldn't happen)
            };
            let (id, name, arguments, thought_sig, thoughts) = entry;

            info!(
                "[engine] tool_call_map[{}]: id={:?} name={:?} args_len={}",
                idx,
                id,
                name,
                arguments.len()
            );

            // Generate ID if provider didn't supply one, or if the
            // accumulated ID is suspiciously short (SSE chunk corruption).
            let call_id = if id.is_empty() || (id.len() < 8 && !id.starts_with("call_")) {
                if !id.is_empty() {
                    warn!(
                        "[engine] Replacing suspicious tool_call id '{}' (len={}) with generated UUID",
                        id, id.len()
                    );
                }
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
        //
        // Tool tiers (VS Code-inspired, adapted for Pawz multi-capability scope):
        //
        //  T1 — SAFE: Read-only, zero side effects → always auto-approve
        //  T2 — REVERSIBLE: Local writes that can be undone (files, memory, tasks) → auto-approve
        //  T3 — EXTERNAL: Irreversible outbound actions (send email, post to Slack,
        //        create Google docs) → require approval, offer "Always Allow"
        //  T4 — DANGEROUS: Shell exec, financial trades, destructive ops → always prompt
        //
        let tc_count = tool_calls.len();

        // ── Plan interception: if the model called execute_plan, hand off
        // to the DAG executor instead of normal tool-by-tool execution ──
        if tool_calls.len() == 1 && tool_calls[0].function.name == "execute_plan" {
            let tc = &tool_calls[0];
            info!("[engine] Intercepting execute_plan — routing to DAG executor");

            let args_str = &tc.function.arguments;
            let args: serde_json::Value =
                match serde_json::from_str(if args_str.trim().is_empty() {
                    "{}"
                } else {
                    args_str
                }) {
                    Ok(v) => v,
                    Err(e) => {
                        let err_msg = format!(
                            "Failed to parse execute_plan arguments: {}. \
                         Please provide a valid JSON plan with 'nodes' array.",
                            e
                        );
                        messages.push(Message {
                            role: Role::Tool,
                            content: MessageContent::Text(err_msg),
                            tool_calls: None,
                            tool_call_id: Some(tc.id.clone()),
                            name: Some("execute_plan".to_string()),
                        });
                        continue;
                    }
                };

            // Parse the plan
            let plan = match crate::engine::plan::parse_plan(&args) {
                Ok(p) => p,
                Err(e) => {
                    messages.push(Message {
                        role: Role::Tool,
                        content: MessageContent::Text(format!(
                            "Plan parsing failed: {}. Fix the plan and retry, or call tools individually.",
                            e
                        )),
                        tool_calls: None,
                        tool_call_id: Some(tc.id.clone()),
                        name: Some("execute_plan".to_string()),
                    });
                    continue;
                }
            };

            // Validate against available tools
            let validation_errors = crate::engine::plan::validate_plan(&plan, tools);
            if !validation_errors.is_empty() {
                let err_list: Vec<String> =
                    validation_errors.iter().map(|e| e.to_string()).collect();
                messages.push(Message {
                    role: Role::Tool,
                    content: MessageContent::Text(format!(
                        "Plan validation failed:\n- {}\nFix these issues and retry, or call tools individually.",
                        err_list.join("\n- ")
                    )),
                    tool_calls: None,
                    tool_call_id: Some(tc.id.clone()),
                    name: Some("execute_plan".to_string()),
                });
                continue;
            }

            // Execute the plan (parallel DAG execution)
            let results =
                crate::engine::plan::execute_plan(&plan, app_handle, agent_id, session_id, run_id)
                    .await;

            // Build results context for the model
            let results_context = crate::engine::plan::build_results_context(&plan, &results);

            // Track tool calls and timing for telemetry
            let plan_node_count = plan.nodes.len() as u32;
            tool_call_count += plan_node_count;

            messages.push(Message {
                role: Role::Tool,
                content: MessageContent::Text(results_context),
                tool_calls: None,
                tool_call_id: Some(tc.id.clone()),
                name: Some("execute_plan".to_string()),
            });

            // Continue the loop — model will synthesize results into a response
            info!(
                "[engine] Plan execution complete: {} nodes executed, feeding results back to model",
                plan_node_count
            );
            continue;
        }

        for tc in &tool_calls {
            info!("[engine] Tool call: {} id={}", tc.function.name, tc.id);

            // ─── T1: Safe — read-only / informational (always auto-approve) ───
            let tier1_safe: &[&str] = &[
                "fetch",
                "read_file",
                "list_directory",
                "soul_read",
                "soul_list",
                "memory_search",
                "memory_stats",
                "self_info",
                "web_search",
                "web_read",
                "web_screenshot",
                "web_browse",
                "list_tasks",
                "email_read",
                "slack_read",
                "telegram_read",
                "google_gmail_list",
                "google_gmail_read",
                "google_calendar_list",
                "google_drive_list",
                "google_drive_read",
                "google_sheets_read",
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
                "agent_list",
                "agent_skills",
                "agent_read_messages",
                "list_squads",
                "skill_search",
                "skill_list",
                "request_tools",
                "mcp_refresh",
                "search_ncnodes",
                "n8n_list_workflows",
                // Canvas (internal UI — zero side effects)
                "canvas_push",
                "canvas_update",
                "canvas_save",
                "canvas_load",
                "canvas_list_dashboards",
                "canvas_delete_dashboard",
                "canvas_list_templates",
                "canvas_from_template",
                "canvas_create_template",
                "trello_list_boards",
                "trello_get_board",
                "trello_get_lists",
                "trello_get_cards",
                "trello_get_card",
                "trello_search",
                "trello_get_labels",
                "trello_get_members",
                "execute_plan",
            ];

            // ─── T2: Reversible — local writes, can be undone (auto-approve) ───
            let tier2_reversible: &[&str] = &[
                "soul_write",
                "memory_store",
                "memory_knowledge",
                "update_profile",
                "create_task",
                "manage_task",
                "write_file",
                "agent_skill_assign",
                "skill_install",
                "agent_send_message",
                "create_squad",
                "manage_squad",
                "squad_broadcast",
            ];

            // ─── T3: External — irreversible outbound actions (prompt, offer Always Allow) ───
            // These leave the user's machine — can't be undone once sent.
            let tier3_external: &[&str] = &[
                "email_send",
                "google_gmail_send",
                "google_docs_create",
                "google_drive_upload",
                "google_drive_share",
                "google_calendar_create",
                "google_sheets_append",
                "google_api",
                "image_generate",
                "trello_create_board",
                "trello_update_board",
                "trello_create_list",
                "trello_update_list",
                "trello_archive_list",
                "trello_create_card",
                "trello_update_card",
                "trello_move_card",
                "trello_add_comment",
                "trello_create_label",
                "trello_update_label",
                "trello_add_label",
                "trello_remove_label",
                "trello_create_checklist",
                "trello_add_checklist_item",
                "trello_toggle_checklist_item",
            ];

            // ─── T4: Dangerous — financial / destructive (always prompt) ───
            let tier4_dangerous: &[&str] = &[
                "exec",
                "run_command",
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

            // Combined auto-approve set: T1 + T2
            let auto_approved_tools: Vec<&str> = tier1_safe
                .iter()
                .chain(tier2_reversible.iter())
                .copied()
                .collect();

            // Trading write tools check the policy-based approval function
            let trading_write_tools = tier4_dangerous
                .iter()
                .filter(|t| {
                    t.starts_with("sol_") || t.starts_with("dex_") || t.starts_with("coinbase_")
                })
                .copied()
                .collect::<Vec<&str>>();

            // Determine the tier label for the tool (sent to frontend for UI hints)
            let _tool_tier = if tier1_safe.contains(&tc.function.name.as_str()) {
                "safe"
            } else if tier2_reversible.contains(&tc.function.name.as_str()) {
                "reversible"
            } else if tier3_external.contains(&tc.function.name.as_str()) {
                "external"
            } else if tier4_dangerous.contains(&tc.function.name.as_str()) {
                "dangerous"
            } else {
                "unknown" // MCP/dynamic tools — default to requiring approval
            };

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
                    fire(
                        app_handle,
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
                fire(
                    app_handle,
                    EngineEvent::ToolRequest {
                        session_id: session_id.to_string(),
                        run_id: run_id.to_string(),
                        tool_call: tc.clone(),
                        tool_tier: Some(_tool_tier.to_string()),
                        round_number: Some(round + 1),
                        loaded_tools: None,
                        context_tokens: None,
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

                // Audit: log tool denial
                if let Some(es) = app_handle.try_state::<crate::engine::state::EngineState>() {
                    crate::engine::audit::log_tool_denied(
                        &es.store,
                        agent_id,
                        session_id,
                        &tc.function.name,
                        &tc.id,
                    );
                }

                // Emit denial as tool result
                fire(
                    app_handle,
                    EngineEvent::ToolResultEvent {
                        session_id: session_id.to_string(),
                        run_id: run_id.to_string(),
                        tool_call_id: tc.id.clone(),
                        output: "Tool execution denied by user.".into(),
                        success: false,
                        duration_ms: None,
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
            let tool_timer = telem::ToolTimer::start(&tc.function.name);
            let result = tools::execute_tool(tc, app_handle, agent_id).await;
            let tool_ms = tool_timer.finish(&telem_collector, &telem_root_id, result.success);
            tool_duration_total_ms += tool_ms;
            tool_call_count += 1;

            info!(
                "[engine] Tool result: {} success={} output_len={}",
                tc.function.name,
                result.success,
                result.output.len()
            );

            // Audit: log tool execution result
            if let Some(es) = app_handle.try_state::<crate::engine::state::EngineState>() {
                crate::engine::audit::log_tool_call(
                    &es.store,
                    agent_id,
                    session_id,
                    &tc.function.name,
                    &tc.id,
                    &tc.function.arguments,
                    result.success,
                    &result.output,
                );
            }

            // Emit tool result event
            fire(
                app_handle,
                EngineEvent::ToolResultEvent {
                    session_id: session_id.to_string(),
                    run_id: run_id.to_string(),
                    tool_call_id: tc.id.clone(),
                    output: result.output.clone(),
                    success: result.success,
                    duration_ms: Some(tool_ms),
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

            // ── Phase 4: Record tool transition & predict next tool ───
            // After each tool execution, record the A→B transition in SQLite
            // and predict the next likely tool call for speculative pre-warming.
            if let Some(es) = app_handle.try_state::<crate::engine::state::EngineState>() {
                let conn = es.store.conn();
                let db = conn.lock();
                if let Some(candidate) = crate::engine::speculative::predict_and_record(
                    &db,
                    previous_tool.as_deref(),
                    &tc.function.name,
                    &speculation_config,
                ) {
                    speculation_stats.predictions += 1;
                    info!(
                        "[speculative] Predicted next tool: {} (p={:.2})",
                        candidate.tool_name, candidate.probability
                    );

                    // Pre-warm connection for the predicted tool's API domain
                    if speculation_config.warm_connections {
                        if let Some(target) =
                            crate::engine::speculative::warm_target_for_domain(&candidate.tool_name)
                        {
                            if let Ok(dur) = crate::engine::speculative::warm_connection(&target) {
                                speculation_stats.connections_warmed += 1;
                                info!(
                                    "[speculative] Pre-warmed connection to {}:{} in {:.1}ms",
                                    target.host,
                                    target.port,
                                    dur.as_secs_f64() * 1000.0
                                );
                            }
                        }
                    }
                } else {
                    // Still record transition even if no prediction
                    // (predict_and_record already does this internally)
                }
            }
            // Update previous_tool for the next iteration's transition recording
            previous_tool = Some(tc.function.name.clone());
        }

        // ── 6. Tool RAG: refresh tools if request_tools was called ─────
        helpers::refresh_tool_rag(app_handle, tools);

        // ── 7. Mid-loop context truncation ─────────────────────────────
        // §24 Checkpoint: snapshot conversation state before truncation destroys messages
        if let Some(es) = app_handle.try_state::<crate::engine::state::EngineState>() {
            let checkpoint_msgs: Vec<crate::atoms::engram_types::CheckpointMessage> = messages
                .iter()
                .map(|m| crate::atoms::engram_types::CheckpointMessage {
                    role: format!("{:?}", m.role).to_lowercase(),
                    content: match &m.content {
                        MessageContent::Text(t) => t.clone(),
                        MessageContent::Blocks(blocks) => blocks
                            .iter()
                            .filter_map(|b| match b {
                                ContentBlock::Text { text } => Some(text.as_str()),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join("\n"),
                    },
                    timestamp: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                })
                .collect();
            let empty_wm = crate::atoms::engram_types::WorkingMemorySnapshot {
                agent_id: agent_id.to_string(),
                slots: vec![],
                momentum_embeddings: vec![],
                saved_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            };
            let empty_hashes = std::collections::HashMap::new();
            let req = crate::engine::engram::context_continuity::CaptureCheckpointRequest {
                agent_id,
                session_id,
                messages: &checkpoint_msgs,
                working_memory: &empty_wm,
                file_hashes: &empty_hashes,
                tasks: &[],
                key_decisions: &[],
            };
            if let Err(e) =
                crate::engine::engram::context_continuity::capture_checkpoint(&es.store, &req)
            {
                warn!(
                    "[engine] Failed to capture pre-truncation checkpoint: {}",
                    e
                );
            }
        }
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
