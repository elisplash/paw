// Paw Agent Engine — Memory tools
// memory_store, memory_search

use crate::atoms::error::EngineResult;
use crate::atoms::types::*;
use crate::engine::engram;
use crate::engine::memory;
use crate::engine::state::EngineState;
use log::info;
use tauri::Manager;

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "memory_store".into(),
                description: "Store a fact or piece of information in your long-term memory. These memories persist across conversations. Use memory_search to recall them later — they are NOT automatically injected into context. Use this to remember user preferences, important facts, project details, etc.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "content": { "type": "string", "description": "The fact or information to remember" },
                        "category": {
                            "type": "string",
                            "description": "Category for organization. Choose the most specific match.",
                            "enum": ["general", "preference", "fact", "skill", "context", "instruction", "correction", "feedback", "project", "person", "technical", "session", "task_result", "summary", "conversation", "insight", "error_log", "procedure"]
                        },
                        "importance": {
                            "type": "number",
                            "description": "How important this memory is (0.0 to 1.0). Higher importance memories are recalled more readily and resist decay. Default: 0.5"
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
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "memory_knowledge".into(),
                description: "Store a structured knowledge triple (subject-predicate-object) in semantic memory. Use this for factual relationships: 'User prefers dark mode', 'Project uses Rust', 'API rate limit is 100/minute'. Triples with the same subject+predicate are automatically updated (reconsolidation).".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "subject": { "type": "string", "description": "The subject entity (e.g., 'user', 'project', 'API')" },
                        "predicate": { "type": "string", "description": "The relationship or property (e.g., 'prefers', 'uses', 'has_limit')" },
                        "object": { "type": "string", "description": "The value or target (e.g., 'dark mode', 'Rust', '100 per minute')" },
                        "category": { "type": "string", "description": "Category: 'preference', 'project', 'fact', 'instruction', 'person', 'technical'" }
                    },
                    "required": ["subject", "predicate", "object"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "memory_stats".into(),
                description: "Get statistics about your memory system — how many episodic, semantic, and procedural memories are stored.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "memory_delete".into(),
                description: "Delete a specific memory by its ID. Use memory_search first to find the ID of the memory you want to delete.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "memory_id": { "type": "string", "description": "The ID of the memory to delete (from memory_search results)" }
                    },
                    "required": ["memory_id"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "memory_update".into(),
                description: "Update an existing memory's content. Use memory_search first to find the ID of the memory you want to update.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "memory_id": { "type": "string", "description": "The ID of the memory to update" },
                        "content": { "type": "string", "description": "The new content for the memory" }
                    },
                    "required": ["memory_id", "content"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "memory_list".into(),
                description: "List your stored memories, optionally filtered by category. Useful for browsing what you remember.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "category": { "type": "string", "description": "Filter by category (optional)" },
                        "limit": { "type": "integer", "description": "Maximum number of memories to return (default: 20)" }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "memory_feedback".into(),
                description: "Provide feedback on a recalled memory — mark it as helpful or unhelpful. This adjusts the memory's trust score so it ranks higher or lower in future searches. Use after memory_search to improve recall quality over time.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "memory_id": { "type": "string", "description": "The ID of the memory to provide feedback on" },
                        "helpful": { "type": "boolean", "description": "true if the memory was helpful/relevant, false if it was unhelpful/wrong" },
                        "context": { "type": "string", "description": "Optional: describe the context where this memory was unhelpful (enables context-aware suppression)" }
                    },
                    "required": ["memory_id", "helpful"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "memory_relate".into(),
                description: "Create a relationship between two memories. This strengthens the memory graph and improves multi-hop retrieval. Use when you discover that two memories are related, contradictory, or one supports/supersedes the other.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "source_id": { "type": "string", "description": "The ID of the first memory" },
                        "target_id": { "type": "string", "description": "The ID of the second memory" },
                        "relation": {
                            "type": "string",
                            "description": "Type of relationship",
                            "enum": ["related_to", "supports", "contradicts", "supersedes", "caused_by", "example_of", "part_of", "inferred_from"]
                        },
                        "weight": { "type": "number", "description": "Relationship strength (0.0-1.0, default: 0.7)" }
                    },
                    "required": ["source_id", "target_id", "relation"]
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
        "memory_store" => Some(
            execute_memory_store(args, app_handle, agent_id)
                .await
                .map_err(|e| e.to_string()),
        ),
        "memory_search" => Some(
            execute_memory_search(args, app_handle, agent_id)
                .await
                .map_err(|e| e.to_string()),
        ),
        "memory_knowledge" => Some(
            execute_memory_knowledge(args, app_handle, agent_id)
                .await
                .map_err(|e| e.to_string()),
        ),
        "memory_stats" => Some(execute_memory_stats(app_handle).map_err(|e| e.to_string())),
        "memory_delete" => Some(execute_memory_delete(args, app_handle).map_err(|e| e.to_string())),
        "memory_update" => Some(
            execute_memory_update(args, app_handle)
                .await
                .map_err(|e| e.to_string()),
        ),
        "memory_list" => {
            Some(execute_memory_list(args, app_handle, agent_id).map_err(|e| e.to_string()))
        }
        "memory_feedback" => {
            Some(execute_memory_feedback(args, app_handle).map_err(|e| e.to_string()))
        }
        "memory_relate" => Some(execute_memory_relate(args, app_handle).map_err(|e| e.to_string())),
        _ => None,
    }
}

async fn execute_memory_store(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    agent_id: &str,
) -> EngineResult<String> {
    let content = args["content"]
        .as_str()
        .ok_or("memory_store: missing 'content' argument")?;
    let category = args["category"].as_str().unwrap_or("general");
    let importance = args["importance"]
        .as_f64()
        .map(|v| v as f32)
        .unwrap_or(0.5)
        .clamp(0.0, 1.0);
    info!(
        "[engine] memory_store: category={} importance={:.1} len={} agent={}",
        category,
        importance,
        content.len(),
        agent_id
    );
    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;
    let emb_client = state.embedding_client();

    // Store via Engram (three-tier memory system) — scoped to calling agent
    let result = engram::bridge::store(
        &state.store,
        content,
        category,
        importance,
        emb_client.as_ref(),
        Some(agent_id),
        None, // session_id
    )
    .await?;

    // Also store in legacy system for backward compatibility
    let legacy_importance = (importance * 10.0).round().clamp(0.0, 255.0) as u8;
    let _ = memory::store_memory(
        &state.store,
        content,
        category,
        legacy_importance,
        emb_client.as_ref(),
        Some(agent_id),
    )
    .await;

    match result {
        Some(id) => Ok(format!(
            "Memory stored (id: {}). Use memory_search to recall it in future sessions.",
            &id[..8.min(id.len())]
        )),
        None => Ok("Memory deduplicated — a similar memory already exists.".into()),
    }
}

async fn execute_memory_search(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    agent_id: &str,
) -> EngineResult<String> {
    let query = args["query"]
        .as_str()
        .ok_or("memory_search: missing 'query' argument")?;
    let limit = args["limit"].as_u64().unwrap_or(10) as usize;
    info!(
        "[engine] memory_search: query='{}' limit={} agent={}",
        &query[..query.len().min(100)],
        limit,
        agent_id
    );
    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;
    let emb_client = state.embedding_client();

    // Search via Engram (BM25 + vector + graph fusion) — scoped to calling agent
    let engram_results = engram::bridge::search(
        &state.store,
        query,
        limit,
        0.1,
        emb_client.as_ref(),
        Some(agent_id),
    )
    .await?;

    if !engram_results.is_empty() {
        let mut output = format!("Found {} relevant memories:\n\n", engram_results.len());
        for (i, mem) in engram_results.iter().enumerate() {
            output.push_str(&format!(
                "{}. [{}] ({}) {} (id: {}, score: {:.2})\n",
                i + 1,
                mem.category,
                mem.memory_type,
                mem.content,
                &mem.id[..mem.id.len().min(8)],
                mem.score,
            ));
        }
        return Ok(output);
    }

    // Fallback to legacy memory search
    let results = memory::search_memories(
        &state.store,
        query,
        limit,
        0.1,
        emb_client.as_ref(),
        Some(agent_id),
    )
    .await?;
    if results.is_empty() {
        return Ok("No relevant memories found.".into());
    }
    let mut output = format!("Found {} relevant memories:\n\n", results.len());
    for (i, mem) in results.iter().enumerate() {
        output.push_str(&format!(
            "{}. [{}] {} (score: {:.2})\n",
            i + 1,
            mem.category,
            mem.content,
            mem.score.unwrap_or(0.0)
        ));
    }
    Ok(output)
}

async fn execute_memory_knowledge(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    agent_id: &str,
) -> EngineResult<String> {
    use crate::atoms::engram_types::{MemoryScope, SemanticMemory};

    let subject = args["subject"]
        .as_str()
        .ok_or("memory_knowledge: missing 'subject' argument")?;
    let predicate = args["predicate"]
        .as_str()
        .ok_or("memory_knowledge: missing 'predicate' argument")?;
    let object = args["object"]
        .as_str()
        .ok_or("memory_knowledge: missing 'object' argument")?;
    let category = args["category"].as_str().unwrap_or("fact");

    info!(
        "[engine] memory_knowledge: {} {} {} agent={}",
        subject, predicate, object, agent_id
    );

    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;
    let emb_client = state.embedding_client();

    let mem = SemanticMemory {
        id: uuid::Uuid::new_v4().to_string(),
        subject: subject.to_string(),
        predicate: predicate.to_string(),
        object: object.to_string(),
        full_text: format!("{} {} {}", subject, predicate, object),
        category: category.to_string(),
        confidence: 0.8,
        is_user_explicit: true,
        contradiction_of: None,
        scope: MemoryScope {
            global: false,
            agent_id: Some(agent_id.to_string()),
            ..Default::default()
        },
        embedding: None,
        embedding_model: None,
        version: 1,
        created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        updated_at: None,
    };

    let id = engram::graph::store_semantic_dedup(&state.store, mem, emb_client.as_ref()).await?;

    Ok(format!(
        "Knowledge stored: '{}' {} '{}' (id: {}). This triple will be auto-recalled in relevant contexts.",
        subject,
        predicate,
        object,
        &id[..id.len().min(8)]
    ))
}

fn execute_memory_stats(app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    let stats = engram::bridge::stats(&state.store)?;

    Ok(format!(
        "Memory Statistics:\n- Episodic memories: {}\n- Semantic triples: {}\n- Procedural memories: {}\n- Graph edges: {}",
        stats.episodic, stats.semantic, stats.procedural, stats.edges,
    ))
}

fn execute_memory_delete(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
) -> EngineResult<String> {
    let memory_id = args["memory_id"]
        .as_str()
        .ok_or("memory_delete: missing 'memory_id' argument")?;

    info!("[engine] memory_delete: id={}", memory_id);

    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    // Try deleting from Engram episodic tier
    let deleted = state.store.engram_delete_episodic(memory_id).is_ok();

    // Also try legacy memory delete
    let _ = state.store.delete_memory(memory_id);

    if deleted {
        Ok(format!(
            "Memory {} deleted successfully.",
            &memory_id[..memory_id.len().min(8)]
        ))
    } else {
        Ok(format!(
            "Memory {} not found or already deleted.",
            &memory_id[..memory_id.len().min(8)]
        ))
    }
}

async fn execute_memory_update(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
) -> EngineResult<String> {
    let memory_id = args["memory_id"]
        .as_str()
        .ok_or("memory_update: missing 'memory_id' argument")?;
    let new_content = args["content"]
        .as_str()
        .ok_or("memory_update: missing 'content' argument")?;

    info!(
        "[engine] memory_update: id={} new_len={}",
        memory_id,
        new_content.len()
    );

    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;
    let emb_client = state.embedding_client();

    // Re-embed the updated content
    let embedding = if let Some(client) = emb_client.as_ref() {
        client.embed(new_content).await.ok()
    } else {
        None
    };

    // Update in Engram
    let updated = state
        .store
        .engram_update_episodic_content(memory_id, new_content, embedding.as_deref())
        .unwrap_or(false);

    if updated {
        Ok(format!(
            "Memory {} updated successfully.",
            &memory_id[..memory_id.len().min(8)]
        ))
    } else {
        Ok(format!(
            "Memory {} not found — cannot update.",
            &memory_id[..memory_id.len().min(8)]
        ))
    }
}

fn execute_memory_list(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    agent_id: &str,
) -> EngineResult<String> {
    let category = args["category"].as_str();
    let limit = args["limit"].as_u64().unwrap_or(20) as usize;

    info!(
        "[engine] memory_list: category={:?} limit={} agent={}",
        category, limit, agent_id
    );

    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    let scope = crate::atoms::engram_types::MemoryScope {
        global: false,
        agent_id: Some(agent_id.to_string()),
        ..Default::default()
    };

    let memories = state.store.engram_list_episodic(&scope, category, limit)?;

    if memories.is_empty() {
        return Ok("No memories stored yet.".into());
    }

    let mut output = format!("Showing {} memories:\n\n", memories.len());
    for (i, mem) in memories.iter().enumerate() {
        output.push_str(&format!(
            "{}. [{}] {} (id: {}, strength: {:.2})\n",
            i + 1,
            mem.category,
            mem.content.full,
            &mem.id[..mem.id.len().min(8)],
            mem.strength,
        ));
    }
    Ok(output)
}

/// §14: memory_feedback — adjust trust score based on user/agent feedback.
/// Positive feedback boosts utility; negative feedback records contextual suppression.
fn execute_memory_feedback(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
) -> EngineResult<String> {
    let memory_id = args["memory_id"]
        .as_str()
        .ok_or("memory_feedback: missing 'memory_id' argument")?;
    let helpful = args["helpful"]
        .as_bool()
        .ok_or("memory_feedback: missing 'helpful' argument")?;
    let context = args["context"].as_str();

    info!(
        "[engine] memory_feedback: id={} helpful={} context={:?}",
        &memory_id[..memory_id.len().min(8)],
        helpful,
        context.map(|c| &c[..c.len().min(50)])
    );

    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    if helpful {
        // Positive feedback: boost importance and utility trust dimension
        let trust = crate::atoms::engram_types::TrustScore {
            relevance: 0.0, // don't modify
            accuracy: 0.0,  // don't modify
            freshness: 0.0, // don't modify
            utility: 0.1,   // boost utility
        };
        state.store.engram_update_trust(memory_id, &trust)?;
        // Also boost importance/strength
        state.store.engram_record_access(memory_id, 0.1)?;

        Ok(format!(
            "Positive feedback recorded for memory {}. It will rank higher in future searches.",
            &memory_id[..memory_id.len().min(8)]
        ))
    } else {
        // Negative feedback: reduce importance
        // If context is provided, add to negative_contexts for contextual suppression (§7)
        if let Some(ctx) = context {
            // Store the negative context for this memory
            state.store.engram_add_negative_context(memory_id, ctx)?;
            Ok(format!(
                "Negative feedback recorded for memory {} in context '{}'. It will be suppressed when similar context arises.",
                &memory_id[..memory_id.len().min(8)],
                &ctx[..ctx.len().min(50)]
            ))
        } else {
            // General negative feedback — reduce strength
            state.store.engram_record_access(memory_id, -0.15)?;
            Ok(format!(
                "Negative feedback recorded for memory {}. It will rank lower in future searches.",
                &memory_id[..memory_id.len().min(8)]
            ))
        }
    }
}

/// §14: memory_relate — create a typed edge between two memories.
fn execute_memory_relate(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
) -> EngineResult<String> {
    let source_id = args["source_id"]
        .as_str()
        .ok_or("memory_relate: missing 'source_id' argument")?;
    let target_id = args["target_id"]
        .as_str()
        .ok_or("memory_relate: missing 'target_id' argument")?;
    let relation = args["relation"]
        .as_str()
        .ok_or("memory_relate: missing 'relation' argument")?;
    let weight = args["weight"]
        .as_f64()
        .map(|v| v as f32)
        .unwrap_or(0.7)
        .clamp(0.0, 1.0);

    info!(
        "[engine] memory_relate: {}--[{}]--> {} (w={:.2})",
        &source_id[..source_id.len().min(8)],
        relation,
        &target_id[..target_id.len().min(8)],
        weight,
    );

    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    // Parse the relation string to EdgeType
    let edge_type = match relation {
        "related_to" => crate::atoms::engram_types::EdgeType::RelatedTo,
        "supports" => crate::atoms::engram_types::EdgeType::SupportedBy,
        "contradicts" => crate::atoms::engram_types::EdgeType::Contradicts,
        "supersedes" => crate::atoms::engram_types::EdgeType::Supersedes,
        "caused_by" => crate::atoms::engram_types::EdgeType::CausedBy,
        "example_of" => crate::atoms::engram_types::EdgeType::ExampleOf,
        "part_of" => crate::atoms::engram_types::EdgeType::PartOf,
        "inferred_from" => crate::atoms::engram_types::EdgeType::InferredFrom,
        _ => crate::atoms::engram_types::EdgeType::RelatedTo,
    };

    engram::graph::relate(&state.store, source_id, target_id, edge_type, weight)?;

    Ok(format!(
        "Relationship created: {} --[{}]--> {}. This strengthens multi-hop retrieval.",
        &source_id[..source_id.len().min(8)],
        relation,
        &target_id[..target_id.len().min(8)]
    ))
}
