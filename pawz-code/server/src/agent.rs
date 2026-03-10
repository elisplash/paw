// pawz-code — agent.rs
// The agent loop: call LLM, execute tools, loop, emit SSE events throughout.
// Now integrates: protocols, token reduction pipeline, engram context,
// cancellation support, and rolling task summaries.

use crate::engram;
use crate::memory;
use crate::protocols;
use crate::provider;
use crate::reduction::{self, PromptAssembler};
use crate::state::AppState;
use crate::tools;
use crate::types::*;
use std::sync::Arc;
use std::time::Instant;

/// Build the system prompt using the reduction pipeline:
/// base prompt + active protocols + engram context + memory + workspace map.
fn build_system_prompt(state: &AppState, message: &str, history: &[Message]) -> String {
    // Base identity
    let base = "You are Pawz CODE — a highly capable developer AI agent.\n\
         You have full access to the user's codebase and development tools.\n\
         You can read and write files, run shell commands, grep code, and fetch URLs.\n\
         You have persistent memory across sessions via the remember/recall tools.\n\
         You have deep codebase understanding via the engram_store/engram_recall tools.";

    // Workspace info
    let workspace = state
        .config
        .workspace_root
        .as_deref()
        .map(|w| format!("\nWorkspace root: {}\n", w))
        .unwrap_or_default();

    let base_with_workspace = format!("{}{}", base, workspace);

    // Protocol context (always inject coding + edit + repo_safety + verification)
    let protocol_context = protocols::default_protocol_context(state);

    // Engram context — load for workspace scope if configured
    let engram_context = state
        .config
        .workspace_root
        .as_deref()
        .map(|root| engram::scope_context(state, root))
        .unwrap_or_default();

    // Memory context
    let memory_context = memory::all_memories_context(state);

    // Workspace map — only inject for architecture/exploration tasks
    let request_kind = reduction::classify_request(message);
    let workspace_summary = match request_kind {
        reduction::RequestKind::Architecture | reduction::RequestKind::Exploration => {
            state.config.workspace_root.as_deref().map(|root| {
                reduction::workspace_map(std::path::Path::new(root), 3)
            })
        }
        _ => None,
    };

    // Rolling task summary — compress long sessions
    let task_summary = if history.len() > 10 {
        let s = reduction::rolling_task_summary(history, 10);
        if s.is_empty() { None } else { Some(s) }
    } else {
        None
    };

    // Assemble compressed prompt
    let assembler = PromptAssembler {
        workspace_summary,
        engram_context: if engram_context.is_empty() { None } else { Some(engram_context) },
        memory_context: if memory_context.is_empty() { None } else { Some(memory_context) },
        protocol_context: if protocol_context.is_empty() { None } else { Some(protocol_context) },
        task_summary,
    };

    assembler.build(&base_with_workspace)
}

/// Run a complete agent turn for a single chat request.
/// Publishes EngineEvent JSON strings to `state.sse_tx` which the SSE handler broadcasts.
pub async fn run(state: Arc<AppState>, req: ChatRequest, session_id: String, run_id: String) {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .unwrap_or_default();

    // Load conversation history
    let mut history = match memory::load_history(&state, &session_id) {
        Ok(h) => h,
        Err(e) => {
            fire_error(&state, &session_id, &run_id, &format!("Failed to load history: {}", e));
            return;
        }
    };

    // Build system prompt with reduction pipeline
    let system = build_system_prompt(&state, &req.message, &history);

    // Build the user message, injecting VS Code workspace context if provided
    let user_text = if let Some(ctx) = &req.context {
        format!("{}\n\n---\n{}", req.message, ctx)
    } else {
        req.message.clone()
    };

    let user_msg = Message::user(&user_text);
    history.push(user_msg.clone());
    if let Err(e) = memory::save_message(&state, &session_id, &user_msg) {
        log::warn!("[agent] Failed to save user message: {}", e);
    }

    let tool_defs = tools::all_tools();
    let max_rounds = state.config.max_rounds;
    let mut round = 0u32;
    let mut total_tool_calls = 0usize;
    let mut final_text = String::new();
    let mut total_usage: Option<TokenUsage> = None;
    let mut actual_model = state.config.model.clone();

    // Resolve the model for this request based on its classified role
    let request_role = match reduction::classify_request(&req.message) {
        reduction::RequestKind::Conversational => "fast",
        reduction::RequestKind::Exploration => "default",
        reduction::RequestKind::Edit => "coder",
        reduction::RequestKind::Execution => "default",
        reduction::RequestKind::Architecture => "long_context",
        reduction::RequestKind::Memory => "cheap",
    };
    let resolved_model = state.config.model_for_role(request_role).to_owned();
    log::debug!(
        "[agent] request_role={} resolved_model={}",
        request_role, resolved_model
    );

    // ── Agent loop ────────────────────────────────────────────────────────────
    loop {
        // Check cancellation
        if state.is_cancelled(&run_id) {
            fire_error(&state, &session_id, &run_id, "Run cancelled by operator.");
            return;
        }

        if round >= max_rounds {
            fire_error(
                &state,
                &session_id,
                &run_id,
                &format!("Max rounds ({}) reached — stopping.", max_rounds),
            );
            break;
        }
        round += 1;

        // Clone state for the delta closure (Arc is cheap)
        let state_c = state.clone();
        let sid = session_id.clone();
        let rid = run_id.clone();

        let model_override = if resolved_model != state.config.model {
            Some(resolved_model.as_str())
        } else {
            None
        };

        let result = provider::call_streaming(
            &state.config,
            &client,
            &system,
            &history,
            &tool_defs,
            move |delta_text| {
                let ev = EngineEvent::Delta {
                    session_id: sid.clone(),
                    run_id: rid.clone(),
                    text: delta_text.to_string(),
                };
                state_c.fire(event_to_json(&ev));
            },
            model_override,
        )
        .await;

        let llm = match result {
            Ok(r) => r,
            Err(e) => {
                fire_error(&state, &session_id, &run_id, &e.to_string());
                return;
            }
        };

        if let Some(u) = llm.usage {
            total_usage = Some(u);
        }
        // Use the model name returned by the API if available
        if let Some(m) = llm.model.clone() {
            actual_model = m;
        }
        if !llm.text.is_empty() {
            final_text = llm.text.clone();
        }

        // If no tool calls, we're done
        if llm.tool_calls.is_empty() {
            break;
        }

        // Save the assistant message (text + tool_use blocks)
        let mut assistant_blocks = Vec::new();
        if !llm.text.is_empty() {
            assistant_blocks.push(ContentBlock::Text { text: llm.text.clone() });
        }
        for tc in &llm.tool_calls {
            let input: serde_json::Value = serde_json::from_str(&tc.function.arguments)
                .unwrap_or(serde_json::Value::Null);
            assistant_blocks.push(ContentBlock::ToolUse {
                id: tc.id.clone(),
                name: tc.function.name.clone(),
                input,
            });
        }
        let assistant_msg = Message {
            role: "assistant".into(),
            blocks: assistant_blocks,
        };
        history.push(assistant_msg.clone());
        if let Err(e) = memory::save_message(&state, &session_id, &assistant_msg) {
            log::warn!("[agent] Failed to save assistant message: {}", e);
        }

        // Execute each tool call, collect results
        let mut tool_result_blocks = Vec::new();
        for tc in &llm.tool_calls {
            // Check cancellation before each tool
            if state.is_cancelled(&run_id) {
                fire_error(&state, &session_id, &run_id, "Run cancelled by operator.");
                return;
            }

            total_tool_calls += 1;

            let args: serde_json::Value = serde_json::from_str(&tc.function.arguments)
                .unwrap_or(serde_json::Value::Null);

            // Fire ToolRequest event
            state.fire(event_to_json(&EngineEvent::ToolRequest {
                session_id: session_id.clone(),
                run_id: run_id.clone(),
                tool_call: tc.clone(),
                tool_tier: Some("safe".into()),
                round_number: Some(round),
            }));
            // Auto-approve (all tools are pre-approved in the coding agent)
            state.fire(event_to_json(&EngineEvent::ToolAutoApproved {
                session_id: session_id.clone(),
                run_id: run_id.clone(),
                tool_name: tc.function.name.clone(),
                tool_call_id: tc.id.clone(),
            }));

            let start = Instant::now();
            let exec_result = tools::execute(&tc.function.name, &args, &state).await;

            let duration_ms = start.elapsed().as_millis() as u64;

            let (output, success) = match exec_result {
                Some(Ok(out)) => (out, true),
                Some(Err(e)) => (format!("Error: {}", e), false),
                None => (format!("Unknown tool: {}", tc.function.name), false),
            };

            // Fire ToolResult event
            state.fire(event_to_json(&EngineEvent::ToolResult {
                session_id: session_id.clone(),
                run_id: run_id.clone(),
                tool_call_id: tc.id.clone(),
                tool_name: tc.function.name.clone(),
                output: output.clone(),
                success,
                duration_ms: Some(duration_ms),
            }));

            tool_result_blocks.push(ContentBlock::ToolResult {
                tool_use_id: tc.id.clone(),
                content: output,
                is_error: !success,
            });
        }

        // Append tool results as a user message
        let tool_result_msg = Message {
            role: "user".into(),
            blocks: tool_result_blocks,
        };
        history.push(tool_result_msg.clone());
        if let Err(e) = memory::save_message(&state, &session_id, &tool_result_msg) {
            log::warn!("[agent] Failed to save tool result message: {}", e);
        }
    }

    // Save final assistant response if not yet saved
    if !final_text.is_empty() && !history.last().map_or(false, |m| m.role == "assistant") {
        let final_msg = Message::assistant(&final_text);
        if let Err(e) = memory::save_message(&state, &session_id, &final_msg) {
            log::warn!("[agent] Failed to save final message: {}", e);
        }
    }

    // Fire Complete
    state.fire(event_to_json(&EngineEvent::Complete {
        session_id: session_id.clone(),
        run_id: run_id.clone(),
        text: final_text,
        tool_calls_count: total_tool_calls,
        usage: total_usage,
        model: Some(actual_model),
        total_rounds: Some(round),
        max_rounds: Some(max_rounds),
    }));
}

fn fire_error(state: &AppState, session_id: &str, run_id: &str, message: &str) {
    log::error!("[agent] {}", message);
    state.fire(event_to_json(&EngineEvent::Error {
        session_id: session_id.into(),
        run_id: run_id.into(),
        message: message.into(),
    }));
}
