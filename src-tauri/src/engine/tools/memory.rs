// Paw Agent Engine — Memory tools
// memory_store, memory_search

use crate::atoms::types::*;
use crate::engine::state::EngineState;
use crate::engine::memory;
use log::info;
use tauri::Manager;

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "memory_store".into(),
                description: "Store a fact or piece of information in your long-term memory. These memories persist across conversations and are automatically recalled when relevant. Use this to remember user preferences, important facts, project details, etc.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "content": { "type": "string", "description": "The fact or information to remember" },
                        "category": {
                            "type": "string",
                            "description": "Category for organization: 'user_preference', 'project', 'fact', 'instruction', 'general'",
                            "enum": ["user_preference", "project", "fact", "instruction", "general"]
                        }
                    },
                    "required": ["content"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "memory_search".into(),
                description: "Search your long-term memories for information relevant to a query. Returns the most relevant stored facts.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query to find relevant memories" },
                        "limit": { "type": "integer", "description": "Maximum number of memories to return (default: 5)" }
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
) -> Option<Result<String, String>> {
    match name {
        "memory_store"  => Some(execute_memory_store(args, app_handle).await),
        "memory_search" => Some(execute_memory_search(args, app_handle).await),
        _ => None,
    }
}

async fn execute_memory_store(args: &serde_json::Value, app_handle: &tauri::AppHandle) -> Result<String, String> {
    let content = args["content"].as_str().ok_or("memory_store: missing 'content' argument")?;
    let category = args["category"].as_str().unwrap_or("general");
    info!("[engine] memory_store: category={} len={}", category, content.len());
    let state = app_handle.try_state::<EngineState>().ok_or("Engine state not available")?;
    let emb_client = state.embedding_client();
    let id = memory::store_memory(&state.store, content, category, 5, emb_client.as_ref(), None).await?;
    Ok(format!("Memory stored (id: {}). I'll recall this automatically when it's relevant.", &id[..8]))
}

async fn execute_memory_search(args: &serde_json::Value, app_handle: &tauri::AppHandle) -> Result<String, String> {
    let query = args["query"].as_str().ok_or("memory_search: missing 'query' argument")?;
    let limit = args["limit"].as_u64().unwrap_or(5) as usize;
    info!("[engine] memory_search: query='{}' limit={}", &query[..query.len().min(100)], limit);
    let state = app_handle.try_state::<EngineState>().ok_or("Engine state not available")?;
    let emb_client = state.embedding_client();
    let results = memory::search_memories(&state.store, query, limit, 0.1, emb_client.as_ref(), None).await?;
    if results.is_empty() {
        return Ok("No relevant memories found.".into());
    }
    let mut output = format!("Found {} relevant memories:\n\n", results.len());
    for (i, mem) in results.iter().enumerate() {
        output.push_str(&format!("{}. [{}] {} (score: {:.2})\n", i + 1, mem.category, mem.content, mem.score.unwrap_or(0.0)));
    }
    Ok(output)
}
