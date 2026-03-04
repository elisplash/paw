// Worker Delegation — Route tool calls through a cheaper worker model.
//
// When a worker_model is configured in Model Routing, tool calls from
// the boss model are delegated to the worker for cost-efficient execution.
// The worker can be ANY model from ANY provider:
//   - Cloud: gemini-2.0-flash, gpt-4o-mini, claude-haiku-4-5, deepseek-chat
//   - Local: worker-qwen (Ollama), llama3.2:3b, phi3:mini
//
// Flow: Boss decides "call fetch" or "call mcp_n8n_execute_workflow" →
//       Worker (cheap model) receives task + tool schemas →
//       Worker executes tools (fetch, MCP) →
//       Result returned to boss as tool output.
//
// Security: Worker tools are limited to fetch, MCP, and n8n management.
// Dangerous tools (exec, write_file, delete_file, append_file) are BLOCKED
// from worker execution to maintain HIL (Human-in-the-Loop) integrity.
// All worker tool results are scanned for prompt injection before being
// returned to the boss model.

use crate::atoms::types::*;
use crate::engine::providers::AnyProvider;
use crate::engine::state::EngineState;
use crate::engine::tools;
use crate::engine::util::safe_truncate;
use log::{info, warn};
use tauri::Manager;

/// Maximum rounds the worker gets to execute a task.
const WORKER_MAX_ROUNDS: u32 = 8;

/// Check if a worker model is configured (quick, non-async check).
pub fn has_worker(app_handle: &tauri::AppHandle) -> bool {
    if let Some(state) = app_handle.try_state::<EngineState>() {
        let cfg = state.config.lock();
        cfg.model_routing
            .worker_model
            .as_ref()
            .is_some_and(|m| !m.is_empty())
    } else {
        false
    }
}

/// Attempt to delegate a tool call to the local worker model.
///
/// Returns `Some(ToolResult)` if delegation was performed (success or failure).
/// Returns `None` if no worker model is configured or the provider can't be resolved,
/// signaling the caller to fall back to direct execution.
pub async fn delegate_to_worker(
    tool_call: &ToolCall,
    app_handle: &tauri::AppHandle,
    agent_id: &str,
) -> Option<ToolResult> {
    let state = app_handle.try_state::<EngineState>()?;

    // Read worker model from config
    let (worker_model, providers) = {
        let cfg = state.config.lock();
        let wm = cfg.model_routing.worker_model.clone();
        let providers = cfg.providers.clone();
        (wm, providers)
    };

    let worker_model = worker_model.filter(|m| !m.is_empty())?;

    info!(
        "[worker-delegate] Delegating '{}' to worker model '{}'",
        tool_call.function.name, worker_model
    );

    // Resolve provider for the worker model
    let provider_config = resolve_worker_provider(&worker_model, &providers)?;
    let provider = AnyProvider::from_config(&provider_config);

    // Gather tool definitions the worker can use:
    // - fetch for API calls
    // - MCP tools for n8n-bridged services
    // - n8n management tools
    // NOTE: exec is intentionally excluded — it requires HIL approval which
    // the worker bypasses. Shell commands must go through the main agent loop.
    let mut worker_tools: Vec<ToolDefinition> = Vec::new();
    worker_tools.extend(crate::engine::tools::fetch::definitions());
    worker_tools.extend(ToolDefinition::mcp_tools(app_handle));
    worker_tools.extend(crate::engine::tools::n8n::definitions());

    if worker_tools.is_empty() {
        warn!("[worker-delegate] No tools available for worker — falling back");
        return None;
    }

    // Build the task prompt from the brain's tool call
    let task_prompt = format!(
        "Execute this tool call:\n\nTool: {}\nArguments: {}\n\n\
        Use the available MCP tools to complete this task. \
        Return only the result — no explanation needed.",
        tool_call.function.name, tool_call.function.arguments
    );

    // Build system prompt for the worker
    let system_prompt = "You are the FOREMAN (Worker Agent) for OpenPawz.\n\n\
        Your job is to receive Task Orders and execute them using your available tools.\n\
        You are a silent execution unit — never engage in conversation, never explain your reasoning.\n\n\
        ## Available Tools\n\
        - `fetch` — Make HTTP requests (GET/POST) to any API. Use for data lookups, price checks, API calls.\n\
        - `mcp_*` — MCP bridge tools for n8n-connected services.\n\n\
        ## Execution Rules\n\
        1. Parse the Task Order. Identify what data/action is needed.\n\
        2. Use `fetch` for simple API calls, `mcp_*` for n8n services.\n\
        3. If a tool call fails, retry ONCE with corrected parameters.\n\
        4. Return ONLY the result data — no explanation, no commentary.\n\
        5. For structured data (JSON), return relevant fields only, summarized concisely.\n\n\
        ## Important\n\
        - Execute efficiently. Minimize round trips.\n\
        - Do NOT explain what you're doing. Just execute and return the result.\n\
        - If the task cannot be completed, say ERROR: followed by the reason."
        .to_string();

    // Build messages
    let mut messages = vec![
        Message {
            role: Role::System,
            content: MessageContent::Text(system_prompt),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
        Message {
            role: Role::User,
            content: MessageContent::Text(task_prompt),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
    ];

    // Run the worker's mini agent loop
    let result = run_worker_loop(
        app_handle,
        &provider,
        &worker_model,
        &mut messages,
        &mut worker_tools,
        agent_id,
    )
    .await;

    let (output, success) = match result {
        Ok(text) => {
            info!(
                "[worker-delegate] Worker completed '{}': {} chars",
                tool_call.function.name,
                text.len()
            );
            (text, true)
        }
        Err(e) => {
            warn!(
                "[worker-delegate] Worker failed on '{}': {}",
                tool_call.function.name, e
            );
            (format!("Worker execution failed: {}", e), false)
        }
    };

    Some(ToolResult {
        tool_call_id: tool_call.id.clone(),
        output,
        success,
    })
}

/// Minimal agent loop for the worker: call model → execute tools → repeat.
/// Worker tools are restricted to safe operations (fetch, MCP, n8n).
/// Dangerous tools (exec, write_file, etc.) are blocked.
/// All tool results are scanned for prompt injection before being fed back.
async fn run_worker_loop(
    app_handle: &tauri::AppHandle,
    provider: &AnyProvider,
    model: &str,
    messages: &mut Vec<Message>,
    tools: &mut [ToolDefinition],
    agent_id: &str,
) -> Result<String, String> {
    for round in 1..=WORKER_MAX_ROUNDS {
        info!(
            "[worker-delegate] Worker round {}/{}",
            round, WORKER_MAX_ROUNDS
        );

        // Call the local model
        let chunks = provider
            .chat_stream(messages, tools, model, Some(0.0), None)
            .await
            .map_err(|e| format!("Worker model error: {}", e))?;

        // Assemble response
        let mut text_accum = String::new();
        let mut tool_call_map: std::collections::HashMap<usize, (String, String, String)> =
            std::collections::HashMap::new();
        let mut has_tool_calls = false;

        for chunk in &chunks {
            if let Some(dt) = &chunk.delta_text {
                text_accum.push_str(dt);
            }
            for tc_delta in &chunk.tool_calls {
                has_tool_calls = true;
                let entry = tool_call_map
                    .entry(tc_delta.index)
                    .or_insert_with(|| (String::new(), String::new(), String::new()));
                if let Some(id) = &tc_delta.id {
                    entry.0 = id.clone();
                }
                if let Some(name) = &tc_delta.function_name {
                    entry.1 = name.clone();
                }
                if let Some(args) = &tc_delta.arguments_delta {
                    entry.2.push_str(args);
                }
            }
        }

        // No tool calls → final response
        if !has_tool_calls || tool_call_map.is_empty() {
            if text_accum.is_empty() {
                return Err("Worker returned empty response".into());
            }
            return Ok(text_accum);
        }

        // Build tool calls
        let mut tc_list: Vec<ToolCall> = Vec::new();
        let mut sorted_indices: Vec<usize> = tool_call_map.keys().cloned().collect();
        sorted_indices.sort();

        for idx in sorted_indices {
            let (id, name, arguments) = tool_call_map.get(&idx).unwrap();
            let call_id = if id.is_empty() {
                format!("worker_{}", uuid::Uuid::new_v4())
            } else {
                id.clone()
            };
            tc_list.push(ToolCall {
                id: call_id,
                call_type: "function".into(),
                function: FunctionCall {
                    name: name.clone(),
                    arguments: arguments.clone(),
                },
                thought_signature: None,
                thought_parts: Vec::new(),
            });
        }

        // Add assistant message with tool calls
        messages.push(Message {
            role: Role::Assistant,
            content: MessageContent::Text(text_accum),
            tool_calls: Some(tc_list.clone()),
            tool_call_id: None,
            name: None,
        });

        // Execute each tool call directly via MCP (no recursion through execute_tool)
        for tc in &tc_list {
            info!(
                "[worker-delegate] Worker executing: {} args={}",
                tc.function.name,
                safe_truncate(&tc.function.arguments, 200)
            );

            let result = execute_worker_tool(tc, app_handle, agent_id).await;

            // §Security: scan worker tool results for prompt injection payloads
            // before feeding them back into the model context.
            let sanitized_output = {
                let scan = crate::engine::injection::scan_for_injection(&result.output);
                if scan.is_injection
                    && scan.severity >= Some(crate::engine::injection::InjectionSeverity::High)
                {
                    warn!(
                        "[worker-delegate] Injection detected in tool result for '{}' (score={}), sanitizing",
                        tc.function.name, scan.score
                    );
                    crate::engine::engram::encryption::sanitize_recalled_memory(&result.output)
                } else {
                    result.output.clone()
                }
            };

            info!(
                "[worker-delegate] Worker tool result: {} success={} len={}",
                tc.function.name,
                result.success,
                result.output.len()
            );

            messages.push(Message {
                role: Role::Tool,
                content: MessageContent::Text(sanitized_output),
                tool_calls: None,
                tool_call_id: Some(tc.id.clone()),
                name: Some(tc.function.name.clone()),
            });
        }

        // Loop back — worker will see tool results and either respond or call more tools
    }

    Err(format!(
        "Worker hit max rounds ({}) without completing",
        WORKER_MAX_ROUNDS
    ))
}

/// Resolve the provider config for the worker model.
/// Supports ANY provider — Gemini, Claude, GPT, Ollama, OpenRouter, etc.
/// The worker model is just a model name; the resolver identifies the right provider.
fn resolve_worker_provider(model: &str, providers: &[ProviderConfig]) -> Option<ProviderConfig> {
    // 1. Use the standard provider resolver (handles Gemini, Claude, GPT, DeepSeek, etc.)
    //    This covers all cloud providers by model-name prefix matching.
    if let Some(p) = crate::engine::state::resolve_provider_for_model(model, providers) {
        return Some(p);
    }

    // 2. Ollama fallback — local model names that don't match cloud prefixes
    //    (worker-qwen, qwen2.5:7b, llama3.2:3b, phi3:mini, etc.)
    if model.starts_with("worker-")
        || model.contains(':')
        || model.starts_with("llama")
        || model.starts_with("qwen")
        || model.starts_with("phi")
        || model.starts_with("nomic")
        || model.starts_with("starcoder")
    {
        if let Some(p) = providers.iter().find(|p| p.kind == ProviderKind::Ollama) {
            return Some(p.clone());
        }
    }

    // 3. Last resort — try first enabled provider
    providers.first().cloned()
}

/// Execute a tool call from the worker agent.
/// Routes MCP tools directly to the MCP registry and n8n management tools
/// to the n8n module — bypassing `execute_tool` to avoid recursion.
///
/// §Security: Dangerous tools (exec, write_file, delete_file, append_file) are
/// blocked to maintain HIL integrity. The worker only gets safe tools.
async fn execute_worker_tool(
    tool_call: &ToolCall,
    app_handle: &tauri::AppHandle,
    _agent_id: &str,
) -> ToolResult {
    let name = &tool_call.function.name;
    let args: serde_json::Value =
        serde_json::from_str(&tool_call.function.arguments).unwrap_or(serde_json::json!({}));

    // §Security: Block dangerous tools that require HIL approval.
    // These must go through the main agent loop where the user can approve/deny.
    // Case-insensitive to prevent casing bypass (e.g., "Exec", "WRITE_FILE").
    const BLOCKED_WORKER_TOOLS: &[&str] = &[
        "exec",
        "write_file",
        "delete_file",
        "append_file",
        "email_send",
        "webhook_send",
        "rest_api_call",
        "slack_send",
        "github_api",
    ];
    let name_lower = name.to_lowercase();
    if BLOCKED_WORKER_TOOLS.iter().any(|b| name_lower == *b) {
        warn!(
            "[worker-delegate] Blocked dangerous tool '{}' from worker execution",
            name
        );
        return ToolResult {
            tool_call_id: tool_call.id.clone(),
            output: format!(
                "Error: Tool '{}' requires human approval and cannot be executed by the worker. \
                 Return the task to the main agent for execution.",
                name
            ),
            success: false,
        };
    }

    // §Security: Block MCP tools whose names contain dangerous operation keywords.
    // This prevents a rogue MCP server from exposing "mcp_server_exec" or
    // "mcp_server_write_file" that would bypass the direct tool blocklist above.
    if let Some(after_prefix) = name_lower.strip_prefix("mcp_") {
        const BLOCKED_MCP_PATTERNS: &[&str] = &[
            "exec",
            "shell",
            "run_command",
            "terminal",
            "system",
            "write_file",
            "delete_file",
            "remove_file",
            "file_write",
            "rm_rf",
            "rmdir",
            "unlink",
        ];
        for pattern in BLOCKED_MCP_PATTERNS {
            if after_prefix.contains(pattern) {
                warn!(
                    "[worker-delegate] Blocked MCP tool matching dangerous pattern '{}': {}",
                    pattern, name
                );
                return ToolResult {
                    tool_call_id: tool_call.id.clone(),
                    output: format!(
                        "Error: MCP tool '{}' matches blocked pattern '{}'. \
                         Dangerous MCP operations cannot be executed by the worker.",
                        name, pattern
                    ),
                    success: false,
                };
            }
        }
    }

    let result = if let Some(r) = tools::fetch::execute(name, &args, app_handle).await {
        // fetch — HTTP requests for API calls
        r
    } else if name.starts_with("mcp_") {
        // MCP tools → direct JSON-RPC to MCP server
        if let Some(state) = app_handle.try_state::<EngineState>() {
            let reg = state.mcp_registry.lock().await;
            match reg.execute_tool(name, &args).await {
                Some(r) => r,
                None => Err(format!("Unknown MCP tool: {}", name)),
            }
        } else {
            Err("Engine state not available".into())
        }
    } else if let Some(r) = tools::n8n::execute(name, &args, app_handle).await {
        // n8n management tools (install_n8n_node, search_ncnodes, etc.)
        r
    } else {
        Err(format!("Worker cannot execute tool: {}", name))
    };

    match result {
        Ok(output) => ToolResult {
            tool_call_id: tool_call.id.clone(),
            output,
            success: true,
        },
        Err(err) => ToolResult {
            tool_call_id: tool_call.id.clone(),
            output: format!("Error: {}", err),
            success: false,
        },
    }
}
