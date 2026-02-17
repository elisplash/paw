// Paw Agent Engine — Agentic Loop
// The core orchestration loop: send to model → tool calls → execute → repeat.
// This is the core agent loop that drives Pawz AI interactions.

use crate::engine::types::*;
use crate::engine::providers::AnyProvider;
use crate::engine::tool_executor;
use crate::engine::commands::PendingApprovals;
use log::{info, warn, error};
use std::time::Duration;
use tauri::Emitter;

/// Run a complete agent turn: send messages to the model, execute tool calls,
/// and repeat until the model produces a final text response or max rounds hit.
///
/// Emits `engine-event` Tauri events for real-time streaming to the frontend.
pub async fn run_agent_turn(
    app_handle: &tauri::AppHandle,
    provider: &AnyProvider,
    model: &str,
    messages: &mut Vec<Message>,
    tools: &[ToolDefinition],
    session_id: &str,
    run_id: &str,
    max_rounds: u32,
    temperature: Option<f64>,
    pending_approvals: &PendingApprovals,
    tool_timeout_secs: u64,
) -> Result<String, String> {
    let mut round = 0;
    let mut final_text = String::new();
    let mut total_input_tokens: u64 = 0;
    let mut total_output_tokens: u64 = 0;

    loop {
        round += 1;
        if round > max_rounds {
            warn!("[engine] Max tool rounds ({}) reached, stopping", max_rounds);
            return Ok(final_text);
        }

        info!("[engine] Agent round {}/{} session={} run={}", round, max_rounds, session_id, run_id);

        // ── 1. Call the AI model ──────────────────────────────────────
        let chunks = provider.chat_stream(messages, tools, model, temperature).await?;

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

            // Accumulate token usage
            if let Some(usage) = &chunk.usage {
                total_input_tokens += usage.input_tokens;
                total_output_tokens += usage.output_tokens;
            }
        }

        // ── 3. If no tool calls, we're done ──────────────────────────
        if !has_tool_calls || tool_call_map.is_empty() {
            final_text = text_accum.clone();

            // Add assistant message to history
            messages.push(Message {
                role: Role::Assistant,
                content: MessageContent::Text(text_accum),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });

            // Emit completion event
            let usage = if total_input_tokens > 0 || total_output_tokens > 0 {
                Some(TokenUsage {
                    input_tokens: total_input_tokens,
                    output_tokens: total_output_tokens,
                    total_tokens: total_input_tokens + total_output_tokens,
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

            // All built-in tools skip HIL — the agent has full access.
            // Security classification still happens on the frontend side for
            // exec/shell commands, but the agent loop itself doesn't block.
            let safe_tools = [
                // Core tools
                "exec", "fetch", "read_file", "write_file",
                "list_directory", "append_file", "delete_file",
                // Web tools
                "web_search", "web_read", "web_screenshot", "web_browse",
                // Soul / persona tools
                "soul_read", "soul_write", "soul_list",
                // Memory tools
                "memory_store", "memory_search",
                // Self-awareness
                "self_info",
                // Skill tools
                "email_send", "email_read",
                "slack_send", "slack_read",
                "github_api",
                "rest_api_call",
                "webhook_send",
                "image_generate",
            ];
            let skip_hil = safe_tools.contains(&tc.function.name.as_str());

            let approved = if skip_hil {
                info!("[engine] Auto-approved safe tool: {}", tc.function.name);
                true
            } else {
                // Register a oneshot channel for approval
                let (approval_tx, approval_rx) = tokio::sync::oneshot::channel::<bool>();
                {
                    let mut map = pending_approvals.lock().unwrap();
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
                        let mut map = pending_approvals.lock().unwrap();
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

            // Execute the tool
            let result = tool_executor::execute_tool(tc, app_handle).await;

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

        // ── 6. Loop: send tool results back to model ──────────────────
        info!("[engine] {} tool calls executed, feeding results back to model", tc_count);

        // NOTE: Do NOT emit Complete here — only emit Complete when the model
        // produces a final text response (no more tool calls). Intermediate
        // Complete events were causing premature stream resolution on the frontend.

        // Continue the loop — model will see tool results and either respond or call more tools
    }
}
