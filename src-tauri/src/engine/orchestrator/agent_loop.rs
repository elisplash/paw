// Paw Agent Engine — Orchestrator Agent Loop
//
// Unified streaming agent loop used by both boss and worker agents.
// Parameterized by `AgentRole` to handle the few behavioural differences:
//   - Boss intercepts orchestrator tools and stops on `project_complete`
//   - Worker intercepts `report_progress` and stops on status=done
//   - Boss emits EngineEvent::Complete on final text; worker does not

use crate::engine::providers::AnyProvider;
use crate::engine::state::PendingApprovals;
use crate::engine::types::*;
use log::{info, warn};
use tauri::Emitter;

use super::handlers::{execute_boss_tool, execute_worker_tool};
use crate::atoms::error::EngineResult;

// ── Role enum ──────────────────────────────────────────────────────────

/// Distinguishes boss from worker behaviour inside the shared loop.
pub(crate) enum AgentRole<'a> {
    Boss,
    Worker { agent_id: &'a str },
}

// ── Safe tools (shared) ────────────────────────────────────────────────

/// Tools that skip HIL (human-in-the-loop) approval for both roles.
/// Exfiltration-capable tools (email_send, slack_send, webhook_send,
/// rest_api_call, exec, write_file, append_file, delete_file) are
/// intentionally excluded — they always require user approval.
const SAFE_TOOLS: &[&str] = &[
    // Core read-only
    "fetch", "read_file", "list_directory",
    // Web tools
    "web_search", "web_read", "web_screenshot", "web_browse",
    // Soul / persona
    "soul_read", "soul_write", "soul_list",
    // Memory
    "memory_store", "memory_search",
    // Self-awareness
    "self_info",
    // Skill tools (read-only)
    "email_read", "slack_read", "github_api", "image_generate",
    // Orchestrator / worker control — intercepted before reaching HIL,
    // but listed here so they're skipped if interception is bypassed.
    "delegate_task", "check_agent_status", "send_agent_message",
    "project_complete", "create_sub_agent", "report_progress",
    // Inter-agent comms (safe: only sends/reads messages between agents)
    "agent_send_message", "agent_read_messages",
    // Squads (safe: team management)
    "create_squad", "list_squads", "manage_squad", "squad_broadcast",
    // Task management (read/create — not destructive)
    "create_task", "list_tasks",
];

// ── Unified loop ───────────────────────────────────────────────────────

/// Run a streaming agent loop that intercepts role-specific tools.
///
/// This replaces the former `run_boss_agent_loop` and `run_worker_agent_loop`
/// with a single implementation parameterized by [`AgentRole`].
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) async fn run_orchestrator_loop(
    app_handle: &tauri::AppHandle,
    provider: &AnyProvider,
    model: &str,
    messages: &mut Vec<Message>,
    tools: &[ToolDefinition],
    session_id: &str,
    run_id: &str,
    max_rounds: u32,
    pending_approvals: &PendingApprovals,
    tool_timeout_secs: u64,
    project_id: &str,
    agent_id: &str,
    role: AgentRole<'_>,
) -> EngineResult<String> {
    let label = match &role {
        AgentRole::Boss => "Boss".to_string(),
        AgentRole::Worker { agent_id } => format!("Worker {}", agent_id),
    };

    let mut round = 0u32;
    let mut final_text = String::new();

    loop {
        round += 1;
        if round > max_rounds {
            warn!("[orchestrator] {} max rounds ({}) reached", label, max_rounds);
            return Ok(final_text);
        }

        info!("[orchestrator] {} round {}/{} project={}", label, round, max_rounds, project_id);

        // ── Stream from the AI model ───────────────────────────────
        let chunks = provider.chat_stream(messages, tools, model, None, None).await?;

        let mut text_accum = String::new();
        let mut tool_call_map: std::collections::HashMap<
            usize,
            (String, String, String, Option<String>, Vec<ThoughtPart>),
        > = std::collections::HashMap::new();
        let mut has_tool_calls = false;
        let confirmed_model: Option<String> = chunks.iter().find_map(|c| c.model.clone());

        for chunk in &chunks {
            if let Some(dt) = &chunk.delta_text {
                text_accum.push_str(dt);
                let _ = app_handle.emit("engine-event", EngineEvent::Delta {
                    session_id: session_id.to_string(),
                    run_id: run_id.to_string(),
                    text: dt.clone(),
                });
            }
            // Emit thinking/reasoning text
            if let Some(tt) = &chunk.thinking_text {
                let _ = app_handle.emit("engine-event", EngineEvent::ThinkingDelta {
                    session_id: session_id.to_string(),
                    run_id: run_id.to_string(),
                    text: tt.clone(),
                });
            }
            for tc_delta in &chunk.tool_calls {
                has_tool_calls = true;
                let entry = tool_call_map.entry(tc_delta.index)
                    .or_insert_with(|| (String::new(), String::new(), String::new(), None, Vec::new()));
                if let Some(id) = &tc_delta.id { entry.0 = id.clone(); }
                if let Some(name) = &tc_delta.function_name { entry.1 = name.clone(); }
                if let Some(args_delta) = &tc_delta.arguments_delta { entry.2.push_str(args_delta); }
                if tc_delta.thought_signature.is_some() { entry.3 = tc_delta.thought_signature.clone(); }
            }
            if !chunk.thought_parts.is_empty() {
                let first_idx = chunk.tool_calls.first().map(|tc| tc.index).unwrap_or(0);
                let entry = tool_call_map.entry(first_idx)
                    .or_insert_with(|| (String::new(), String::new(), String::new(), None, Vec::new()));
                entry.4.extend(chunk.thought_parts.clone());
            }
        }

        // ── No tool calls → final response ─────────────────────────
        if !has_tool_calls || tool_call_map.is_empty() {
            final_text = text_accum.clone();
            messages.push(Message {
                role: Role::Assistant,
                content: MessageContent::Text(text_accum),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });

            if matches!(role, AgentRole::Boss) {
                let _ = app_handle.emit("engine-event", EngineEvent::Complete {
                    session_id: session_id.to_string(),
                    run_id: run_id.to_string(),
                    text: final_text.clone(),
                    tool_calls_count: 0,
                    usage: None,
                    model: confirmed_model.clone(),
                });
            }

            return Ok(final_text);
        }

        // ── Assemble tool calls ────────────────────────────────────
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut sorted_indices: Vec<usize> = tool_call_map.keys().cloned().collect();
        sorted_indices.sort();
        for idx in sorted_indices {
            let (id, name, arguments, thought_sig, thoughts) = tool_call_map.get(&idx).unwrap();
            let call_id = if id.is_empty() { format!("call_{}", uuid::Uuid::new_v4()) } else { id.clone() };
            tool_calls.push(ToolCall {
                id: call_id,
                call_type: "function".into(),
                function: FunctionCall { name: name.clone(), arguments: arguments.clone() },
                thought_signature: thought_sig.clone(),
                thought_parts: thoughts.clone(),
            });
        }

        messages.push(Message {
            role: Role::Assistant,
            content: MessageContent::Text(text_accum),
            tool_calls: Some(tool_calls.clone()),
            tool_call_id: None,
            name: None,
        });

        // ── Execute tool calls ─────────────────────────────────────
        let mut should_stop = false;

        for tc in &tool_calls {
            info!("[orchestrator] {} tool call: {} id={}", label, tc.function.name, tc.id);

            // Try role-specific interception first
            let intercepted: Option<Result<String, String>> = match &role {
                AgentRole::Boss => {
                    let result = execute_boss_tool(tc, app_handle, project_id).await;
                    if tc.function.name == "project_complete" { should_stop = true; }
                    result
                }
                AgentRole::Worker { .. } => {
                    let result = execute_worker_tool(tc, app_handle, project_id, agent_id).await;
                    if tc.function.name == "report_progress" {
                        let args: serde_json::Value = serde_json::from_str(&tc.function.arguments).unwrap_or_default();
                        if args["status"].as_str() == Some("done") {
                            should_stop = true;
                        }
                    }
                    result
                }
            };

            if let Some(result) = intercepted {
                let output = match result {
                    Ok(text) => text,
                    Err(e) => format!("Error: {}", e),
                };

                let _ = app_handle.emit("engine-event", EngineEvent::ToolResultEvent {
                    session_id: session_id.to_string(),
                    run_id: run_id.to_string(),
                    tool_call_id: tc.id.clone(),
                    output: output.clone(),
                    success: true,
                });

                messages.push(Message {
                    role: Role::Tool,
                    content: MessageContent::Text(output),
                    tool_calls: None,
                    tool_call_id: Some(tc.id.clone()),
                    name: Some(tc.function.name.clone()),
                });
                continue;
            }

            // Standard tools — apply HIL policy
            let skip_hil = SAFE_TOOLS.contains(&tc.function.name.as_str());
            let approved = if skip_hil {
                true
            } else {
                let (approval_tx, approval_rx) = tokio::sync::oneshot::channel::<bool>();
                {
                    let mut map = pending_approvals.lock();
                    map.insert(tc.id.clone(), approval_tx);
                }
                let _ = app_handle.emit("engine-event", EngineEvent::ToolRequest {
                    session_id: session_id.to_string(),
                    run_id: run_id.to_string(),
                    tool_call: tc.clone(),
                });
                match tokio::time::timeout(
                    std::time::Duration::from_secs(tool_timeout_secs),
                    approval_rx,
                ).await {
                    Ok(Ok(allowed)) => allowed,
                    _ => {
                        let mut map = pending_approvals.lock();
                        map.remove(&tc.id);
                        false
                    }
                }
            };

            if !approved {
                messages.push(Message {
                    role: Role::Tool,
                    content: MessageContent::Text("Tool execution denied by user.".into()),
                    tool_calls: None,
                    tool_call_id: Some(tc.id.clone()),
                    name: Some(tc.function.name.clone()),
                });
                continue;
            }

            let result = crate::engine::tools::execute_tool(tc, app_handle, agent_id).await;
            let _ = app_handle.emit("engine-event", EngineEvent::ToolResultEvent {
                session_id: session_id.to_string(),
                run_id: run_id.to_string(),
                tool_call_id: tc.id.clone(),
                output: result.output.clone(),
                success: result.success,
            });
            messages.push(Message {
                role: Role::Tool,
                content: MessageContent::Text(result.output),
                tool_calls: None,
                tool_call_id: Some(tc.id.clone()),
                name: Some(tc.function.name.clone()),
            });
        }

        if should_stop {
            info!("[orchestrator] {} signalled stop, ending loop", label);
            return Ok(final_text);
        }
    }
}
