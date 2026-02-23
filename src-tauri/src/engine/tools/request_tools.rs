// Paw Agent Engine — Tool RAG: request_tools meta-tool
//
// The "librarian" tool — the agent calls this to discover and load tools
// it needs for a specific task. Uses semantic search over the tool index
// to find the most relevant tools and inject them into the current round.
//
// This replaces the brute-force approach of dumping 75+ tool definitions
// into every request. The agent sees a compact skill domain summary and
// calls request_tools when it needs specific capabilities.

use crate::atoms::types::*;
use crate::engine::state::EngineState;
use crate::engine::tool_index;
use log::info;
use tauri::Manager;
use crate::atoms::error::EngineResult;

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "request_tools".into(),
                description: "Load tools from your skill library. You have many capabilities \
                    but they're not all loaded at once. Describe what you need \
                    (e.g., 'send an email', 'trade crypto on solana', 'create a squad') \
                    and the relevant tools will be loaded for your next action. You can also \
                    request a specific domain: 'email', 'trading', 'web', 'squads', etc."
                    .into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "What you need to do — describe the task or name the skill domain. Examples: 'send email to user', 'crypto trading', 'agent squad management', 'web browsing and screenshots'"
                        },
                        "domain": {
                            "type": "string",
                            "description": "Optional: request all tools from a specific domain directly. One of: system, filesystem, web, identity, memory, agents, communication, squads, tasks, skills, dashboard, storage, email, messaging, github, integrations, trading"
                        }
                    },
                    "required": ["query"]
                }),
            },
        },
    ]
}

pub async fn execute(
    name: &str,
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    agent_id: &str,
) -> Option<Result<String, String>> {
    match name {
        "request_tools" => Some(execute_request_tools(args, app_handle, agent_id).await.map_err(|e| e.to_string())),
        _ => None,
    }
}

async fn execute_request_tools(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    agent_id: &str,
) -> EngineResult<String> {
    let query = args["query"].as_str().unwrap_or("general tools");
    let domain = args["domain"].as_str();

    info!(
        "[tool-rag] request_tools: query='{}' domain={:?} agent={}",
        &query[..query.len().min(100)],
        domain,
        agent_id
    );

    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    // ── If a specific domain was requested, return all tools from that domain ──
    if let Some(dom) = domain {
        let tool_index = state.tool_index.lock().await;
        let domain_tools = tool_index.get_domain_tools(dom);
        if domain_tools.is_empty() {
            return Ok(format!(
                "No tools found for domain '{}'. Available domains: {}",
                dom,
                tool_index::domain_summaries()
                    .iter()
                    .map(|(id, _, desc)| format!("{} ({})", id, desc))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        // Store the loaded tool names for round carryover
        {
            let mut loaded = state.loaded_tools.lock();
            for t in &domain_tools {
                loaded.insert(t.function.name.clone());
            }
        }

        let tool_names: Vec<String> = domain_tools.iter()
            .map(|t| format!("  • **{}** — {}", t.function.name, t.function.description))
            .collect();

        return Ok(format!(
            "✅ Loaded {} tools from the **{}** domain:\n\n{}\n\n\
            These tools are now available for your next action. Call them directly.",
            domain_tools.len(),
            dom,
            tool_names.join("\n")
        ));
    }

    // ── Semantic search for the best matching tools ────────────────────────
    let emb_client = state.embedding_client().ok_or(
        "Embedding client not configured — cannot search tool index. \
        Try requesting a specific domain instead (e.g., domain='email')."
    )?;

    // Ensure the tool index is built
    {
        let mut tool_index = state.tool_index.lock().await;
        if !tool_index.is_ready() {
            info!("[tool-rag] Tool index not ready, building on first request...");
            let all_tools = build_all_tools_for_index(&state);
            tool_index.build(&all_tools, &emb_client).await;
        }
    }

    // Search
    let tool_index = state.tool_index.lock().await;
    let results = tool_index.search(query, 6, &emb_client).await?;

    if results.is_empty() {
        return Ok(format!(
            "No matching tools found for '{}'. Try a more specific query or request a domain directly.\n\
            Available domains: {}",
            query,
            tool_index::domain_summaries()
                .iter()
                .map(|(id, _, _)| *id)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    // Store the loaded tool names for round carryover
    {
        let mut loaded = state.loaded_tools.lock();
        for t in &results {
            loaded.insert(t.function.name.clone());
        }
    }

    let tool_names: Vec<String> = results.iter()
        .map(|t| format!("  • **{}** — {}", t.function.name, t.function.description))
        .collect();

    // Filter out core tools from the result list (they're already loaded)
    let non_core: Vec<&ToolDefinition> = results.iter()
        .filter(|t| !tool_index::CORE_TOOLS.contains(&t.function.name.as_str()))
        .collect();

    Ok(format!(
        "✅ Loaded {} tools for your request:\n\n{}\n\n\
        These tools are now available. Call them directly to proceed.",
        non_core.len(),
        tool_names.join("\n")
    ))
}

/// Build the complete list of tools for indexing.
/// This includes builtins + all possible skill tools (regardless of enabled state).
fn build_all_tools_for_index(state: &EngineState) -> Vec<ToolDefinition> {
    let mut tools = ToolDefinition::builtins();

    // Include ALL skill tools for indexing (even disabled ones)
    // so the agent can discover them and we can tell it "enable the X skill first"
    let all_skill_ids = vec![
        "email".to_string(),
        "slack".to_string(),
        "telegram".to_string(),
        "github".to_string(),
        "rest_api".to_string(),
        "webhook".to_string(),
        "image_gen".to_string(),
        "coinbase".to_string(),
        "dex".to_string(),
        "solana_dex".to_string(),
    ];
    tools.extend(ToolDefinition::skill_tools(&all_skill_ids));

    // Include MCP tools if available
    // (we can't easily get these without the app_handle, so skip for now —
    //  MCP tools are always included in the active tool list anyway)

    // Don't include request_tools itself in the index
    tools.retain(|t| t.function.name != "request_tools");

    let _ = state; // used for future: checking enabled skills for status info
    tools
}
