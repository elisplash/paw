// Worker Delegation — Route MCP tool calls through a local Ollama worker model.
//
// When a worker_model is configured (e.g. "worker-qwen"), MCP tool calls from
// the brain model are delegated to the worker instead of being executed directly.
// The worker receives the MCP tool schemas via the standard tool interface,
// executes them, and returns the result — all locally, at zero API cost.
//
// Flow: Brain decides "call mcp_n8n_slack_post" →
//       Worker-qwen receives task + MCP tool schemas →
//       Worker calls MCP tools via JSON-RPC (local) →
//       Result returned to brain as tool output.

use crate::atoms::types::*;
use crate::engine::providers::AnyProvider;
use crate::engine::state::EngineState;
use crate::engine::tools;
use log::{info, warn};
use tauri::Manager;

/// Maximum rounds the worker gets to execute a task.
const WORKER_MAX_ROUNDS: u32 = 8;

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

    // Gather MCP tool definitions (the worker needs to know what's available)
    let mcp_tools = ToolDefinition::mcp_tools(app_handle);
    if mcp_tools.is_empty() {
        warn!("[worker-delegate] No MCP tools available — falling back to direct execution");
        return None;
    }

    // Also include n8n management tools so the worker can install/refresh
    let mut worker_tools = mcp_tools;
    worker_tools.extend(crate::engine::tools::n8n::definitions());

    // Build the task prompt from the brain's tool call
    let task_prompt = format!(
        "Execute this tool call:\n\nTool: {}\nArguments: {}\n\n\
        Use the available MCP tools to complete this task. \
        Return only the result — no explanation needed.",
        tool_call.function.name, tool_call.function.arguments
    );

    // Build system prompt for the worker
    let system_prompt = "You are the LOCAL FOREMAN (Worker Agent) for OpenPawz.\n\n\
        Your job is to receive Task Orders and translate them into precise MCP tool calls.\n\
        You are a silent execution unit — never engage in conversation, never explain your reasoning.\n\n\
        ## Execution Rules\n\
        1. Parse the Task Order. Identify the tool call needed.\n\
        2. Execute the MCP tool call with the correct parameters.\n\
        3. If a tool call fails, retry ONCE with corrected parameters.\n\
        4. Return the tool result as your final response — nothing else.\n\n\
        ## Important\n\
        - MCP tools are prefixed with `mcp_` — use them as provided.\n\
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
/// No HIL approval (worker tools are pre-approved), no streaming to frontend,
/// no budget tracking. Just silent local execution.
async fn run_worker_loop(
    app_handle: &tauri::AppHandle,
    provider: &AnyProvider,
    model: &str,
    messages: &mut Vec<Message>,
    tools: &mut Vec<ToolDefinition>,
    agent_id: &str,
) -> Result<String, String> {
    for round in 1..=WORKER_MAX_ROUNDS {
        info!("[worker-delegate] Worker round {}/{}", round, WORKER_MAX_ROUNDS);

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
                    entry.0.push_str(id);
                }
                if let Some(name) = &tc_delta.function_name {
                    entry.1.push_str(name);
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
                &tc.function.arguments[..tc.function.arguments.len().min(200)]
            );

            let result = execute_worker_tool(tc, app_handle, agent_id).await;

            info!(
                "[worker-delegate] Worker tool result: {} success={} len={}",
                tc.function.name,
                result.success,
                result.output.len()
            );

            messages.push(Message {
                role: Role::Tool,
                content: MessageContent::Text(result.output),
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
/// Handles Ollama-style names (worker-qwen), colon-style (qwen2.5:7b), and standard prefixes.
fn resolve_worker_provider(model: &str, providers: &[ProviderConfig]) -> Option<ProviderConfig> {
    // 1. Exact match on default_model
    if let Some(p) = providers
        .iter()
        .find(|p| p.default_model.as_deref() == Some(model))
    {
        return Some(p.clone());
    }

    // 2. Known Ollama model names (worker-qwen, custom Ollama models)
    if model.starts_with("worker-")
        || model.contains(':')
        || model.starts_with("llama")
        || model.starts_with("qwen")
        || model.starts_with("phi")
        || model.starts_with("mistral")
        || model.starts_with("codestral")
        || model.starts_with("deepseek-coder")
        || model.starts_with("nomic")
        || model.starts_with("starcoder")
    {
        if let Some(p) = providers
            .iter()
            .find(|p| p.kind == ProviderKind::Ollama)
        {
            return Some(p.clone());
        }
    }

    // 3. Fall back to the state.rs resolver
    crate::engine::state::resolve_provider_for_model(model, providers)
}

/// Execute a tool call from the worker agent.
/// Routes MCP tools directly to the MCP registry and n8n management tools
/// to the n8n module — bypassing `execute_tool` to avoid recursion.
async fn execute_worker_tool(
    tool_call: &ToolCall,
    app_handle: &tauri::AppHandle,
    _agent_id: &str,
) -> ToolResult {
    let name = &tool_call.function.name;
    let args: serde_json::Value =
        serde_json::from_str(&tool_call.function.arguments).unwrap_or(serde_json::json!({}));

    let result = if name.starts_with("mcp_") {
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
        Err(format!("Worker cannot execute non-MCP tool: {}", name))
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
