// Paw Agent Engine — Memory tools
// memory_store, memory_search

use crate::atoms::error::EngineResult;
use crate::atoms::types::*;
use crate::engine::engram;
use crate::engine::memory;
use crate::engine::state::EngineState;
use crate::engine::util::safe_truncate;
use log::{info, warn};
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
        "memory_delete" => {
            Some(execute_memory_delete(args, app_handle, agent_id).map_err(|e| e.to_string()))
        }
        "memory_update" => Some(
            execute_memory_update(args, app_handle, agent_id)
                .await
                .map_err(|e| e.to_string()),
        ),
        "memory_list" => {
            Some(execute_memory_list(args, app_handle, agent_id).map_err(|e| e.to_string()))
        }
        "memory_feedback" => {
            Some(execute_memory_feedback(args, app_handle, agent_id).map_err(|e| e.to_string()))
        }
        "memory_relate" => {
            Some(execute_memory_relate(args, app_handle, agent_id).map_err(|e| e.to_string()))
        }
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
        Some(&state.hnsw_index),
    )
    .await?;

    // Also store in legacy system for backward compatibility (with dedup)
    let legacy_importance = (importance * 10.0).round().clamp(0.0, 255.0) as u8;
    let _ = memory::store_memory_dedup(
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
        safe_truncate(query, 100),
        limit,
        agent_id
    );
    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;
    let emb_client = state.embedding_client();

    // Search via Engram gated search (§55) — intent-aware, quality-gated retrieval
    let scope = crate::atoms::engram_types::MemoryScope::agent(agent_id);
    let search_config = crate::atoms::engram_types::MemorySearchConfig::default();
    // Issue a signed capability token for read-path scope verification (§43.4)
    let read_cap = crate::engine::engram::memory_bus::issue_read_capability(agent_id).ok();
    let gated_result = engram::gated_search::gated_search(
        &state.store,
        &engram::gated_search::GatedSearchRequest {
            query,
            scope: &scope,
            config: &search_config,
            embedding_client: emb_client.as_ref(),
            budget_tokens: 0, // no token budget limit for tool search
            momentum: None,   // no momentum embeddings
            model: None,      // tool search — conservative injection limits
            capability: read_cap.as_ref(),
            hnsw_index: Some(&state.hnsw_index),
        },
    )
    .await?;

    if !gated_result.memories.is_empty() {
        let mut output = format!(
            "Found {} relevant memories:\n\n",
            gated_result.memories.len()
        );
        for (i, mem) in gated_result.memories.iter().enumerate() {
            output.push_str(&format!(
                "{}. [{}] ({}) {} (id: {}, score: {:.2})\n",
                i + 1,
                mem.category,
                mem.memory_type,
                mem.content,
                safe_truncate(&mem.memory_id, 8),
                mem.trust_score.composite(),
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

    // §Security: enforce size limits on triple fields to prevent abuse
    const MAX_TRIPLE_FIELD_LEN: usize = 2000;
    if subject.len() > MAX_TRIPLE_FIELD_LEN {
        return Err(format!(
            "memory_knowledge: 'subject' exceeds maximum length ({} > {})",
            subject.len(),
            MAX_TRIPLE_FIELD_LEN
        )
        .into());
    }
    if predicate.len() > MAX_TRIPLE_FIELD_LEN {
        return Err(format!(
            "memory_knowledge: 'predicate' exceeds maximum length ({} > {})",
            predicate.len(),
            MAX_TRIPLE_FIELD_LEN
        )
        .into());
    }
    if object.len() > MAX_TRIPLE_FIELD_LEN {
        return Err(format!(
            "memory_knowledge: 'object' exceeds maximum length ({} > {})",
            object.len(),
            MAX_TRIPLE_FIELD_LEN
        )
        .into());
    }

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
        safe_truncate(&id, 8)
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
    agent_id: &str,
) -> EngineResult<String> {
    let memory_id = args["memory_id"]
        .as_str()
        .ok_or("memory_delete: missing 'memory_id' argument")?;

    info!(
        "[engine] memory_delete: id={} agent={}",
        memory_id, agent_id
    );

    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    // §Security: verify agent owns this memory before deleting (fail-closed)
    match state.store.engram_get_episodic(memory_id) {
        Ok(Some(mem)) => {
            let mem_agent = mem.scope.agent_id.as_deref().unwrap_or("");
            if !mem_agent.is_empty() && mem_agent != agent_id {
                warn!(
                    "[engine] memory_delete: agent '{}' attempted to delete memory owned by '{}'",
                    agent_id, mem_agent
                );
                return Err(format!(
                    "memory_delete: memory {} belongs to another agent. You can only delete your own memories.",
                    safe_truncate(memory_id, 8)
                ).into());
            }
        }
        Ok(None) => {
            return Err(format!(
                "memory_delete: memory {} not found.",
                safe_truncate(memory_id, 8)
            )
            .into());
        }
        Err(e) => {
            warn!("[engine] memory_delete: ownership lookup failed: {}", e);
            return Err("memory_delete: failed to verify memory ownership.".into());
        }
    }

    // Try deleting from Engram episodic tier
    let deleted = state.store.engram_delete_episodic(memory_id).is_ok();

    // Also try legacy memory delete
    let _ = state.store.delete_memory(memory_id);

    if deleted {
        Ok(format!(
            "Memory {} deleted successfully.",
            safe_truncate(memory_id, 8)
        ))
    } else {
        Ok(format!(
            "Memory {} not found or already deleted.",
            safe_truncate(memory_id, 8)
        ))
    }
}

async fn execute_memory_update(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    agent_id: &str,
) -> EngineResult<String> {
    let memory_id = args["memory_id"]
        .as_str()
        .ok_or("memory_update: missing 'memory_id' argument")?;
    let new_content = args["content"]
        .as_str()
        .ok_or("memory_update: missing 'content' argument")?;

    info!(
        "[engine] memory_update: id={} new_len={} agent={}",
        memory_id,
        new_content.len(),
        agent_id
    );

    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    // §Security: verify agent owns this memory before updating (fail-closed)
    match state.store.engram_get_episodic(memory_id) {
        Ok(Some(mem)) => {
            let mem_agent = mem.scope.agent_id.as_deref().unwrap_or("");
            if !mem_agent.is_empty() && mem_agent != agent_id {
                warn!(
                    "[engine] memory_update: agent '{}' attempted to update memory owned by '{}'",
                    agent_id, mem_agent
                );
                return Err(format!(
                    "memory_update: memory {} belongs to another agent. You can only update your own memories.",
                    safe_truncate(memory_id, 8)
                ).into());
            }
        }
        Ok(None) => {
            return Err(format!(
                "memory_update: memory {} not found.",
                safe_truncate(memory_id, 8)
            )
            .into());
        }
        Err(e) => {
            warn!("[engine] memory_update: ownership lookup failed: {}", e);
            return Err("memory_update: failed to verify memory ownership.".into());
        }
    }

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
            safe_truncate(memory_id, 8)
        ))
    } else {
        Ok(format!(
            "Memory {} not found — cannot update.",
            safe_truncate(memory_id, 8)
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
            safe_truncate(&mem.id, 8),
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
    agent_id: &str,
) -> EngineResult<String> {
    let memory_id = args["memory_id"]
        .as_str()
        .ok_or("memory_feedback: missing 'memory_id' argument")?;
    let helpful = args["helpful"]
        .as_bool()
        .ok_or("memory_feedback: missing 'helpful' argument")?;
    let context = args["context"].as_str();

    info!(
        "[engine] memory_feedback: id={} helpful={} context={:?} agent={}",
        safe_truncate(memory_id, 8),
        helpful,
        context.map(|c| safe_truncate(c, 50)),
        agent_id
    );

    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    // §Security: verify agent owns this memory before modifying trust scores (fail-closed)
    match state.store.engram_get_episodic(memory_id) {
        Ok(Some(mem)) => {
            let mem_agent = mem.scope.agent_id.as_deref().unwrap_or("");
            if !mem_agent.is_empty() && mem_agent != agent_id {
                warn!(
                    "[engine] memory_feedback: agent '{}' attempted to modify trust of memory owned by '{}'",
                    agent_id, mem_agent
                );
                return Err(format!(
                    "memory_feedback: memory {} belongs to another agent. You can only provide feedback on your own memories.",
                    safe_truncate(memory_id, 8)
                ).into());
            }
        }
        Ok(None) => {
            return Err(format!(
                "memory_feedback: memory {} not found.",
                safe_truncate(memory_id, 8)
            )
            .into());
        }
        Err(e) => {
            warn!("[engine] memory_feedback: ownership lookup failed: {}", e);
            return Err("memory_feedback: failed to verify memory ownership.".into());
        }
    }

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
            safe_truncate(memory_id, 8)
        ))
    } else {
        // Negative feedback: reduce importance
        // If context is provided, add to negative_contexts for contextual suppression (§7)
        if let Some(ctx) = context {
            // Store the negative context for this memory
            state.store.engram_add_negative_context(memory_id, ctx)?;
            Ok(format!(
                "Negative feedback recorded for memory {} in context '{}'. It will be suppressed when similar context arises.",
                safe_truncate(memory_id, 8),
                safe_truncate(ctx, 50)
            ))
        } else {
            // General negative feedback — reduce strength
            state.store.engram_record_access(memory_id, -0.15)?;
            Ok(format!(
                "Negative feedback recorded for memory {}. It will rank lower in future searches.",
                safe_truncate(memory_id, 8)
            ))
        }
    }
}

/// §14: memory_relate — create a typed edge between two memories.
fn execute_memory_relate(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    agent_id: &str,
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
        safe_truncate(source_id, 8),
        relation,
        safe_truncate(target_id, 8),
        weight,
    );

    let state = app_handle
        .try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    // §Security: verify agent owns at least one of the memories being related (fail-closed)
    let source_ownership = state.store.engram_get_episodic(source_id);
    let target_ownership = state.store.engram_get_episodic(target_id);

    let source_ok = match &source_ownership {
        Ok(Some(mem)) => {
            mem.scope.agent_id.as_deref() == Some(agent_id) || mem.scope.agent_id.is_none()
        }
        Ok(None) => false,
        Err(_) => false,
    };
    let target_ok = match &target_ownership {
        Ok(Some(mem)) => {
            mem.scope.agent_id.as_deref() == Some(agent_id) || mem.scope.agent_id.is_none()
        }
        Ok(None) => false,
        Err(_) => false,
    };

    // At least one must exist and be owned by this agent
    if !source_ok && !target_ok {
        // If both lookups failed, report the error
        if source_ownership.is_err() && target_ownership.is_err() {
            warn!(
                "[engine] memory_relate: ownership lookups failed for agent '{}'",
                agent_id
            );
            return Err("memory_relate: failed to verify memory ownership.".into());
        }
        warn!(
            "[engine] memory_relate: agent '{}' attempted to relate memories it doesn't own",
            agent_id
        );
        return Err(
            "memory_relate: you can only create relationships involving your own memories.".into(),
        );
    }

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
        safe_truncate(source_id, 8),
        relation,
        safe_truncate(target_id, 8)
    ))
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── Tool Definition Schema Tests ─────────────────────────────────────

    #[test]
    fn test_definitions_memory_delete_uses_memory_id() {
        let defs = definitions();
        let delete_def = defs
            .iter()
            .find(|d| d.function.name == "memory_delete")
            .expect("memory_delete definition must exist");
        let props = &delete_def.function.parameters["properties"];
        assert!(
            props.get("memory_id").is_some(),
            "memory_delete must use 'memory_id' (not 'id') in schema"
        );
        assert!(
            props.get("id").is_none(),
            "memory_delete must NOT have legacy 'id' field"
        );
        let required = delete_def.function.parameters["required"]
            .as_array()
            .expect("required must be an array");
        assert!(
            required.iter().any(|v| v.as_str() == Some("memory_id")),
            "memory_delete must require 'memory_id'"
        );
    }

    #[test]
    fn test_definitions_memory_update_uses_memory_id() {
        let defs = definitions();
        let update_def = defs
            .iter()
            .find(|d| d.function.name == "memory_update")
            .expect("memory_update definition must exist");
        let props = &update_def.function.parameters["properties"];
        assert!(
            props.get("memory_id").is_some(),
            "memory_update must use 'memory_id'"
        );
        let required = update_def.function.parameters["required"]
            .as_array()
            .expect("required must be an array");
        assert!(
            required.iter().any(|v| v.as_str() == Some("memory_id")),
            "memory_update must require 'memory_id'"
        );
    }

    #[test]
    fn test_definitions_memory_feedback_uses_memory_id() {
        let defs = definitions();
        let feedback_def = defs
            .iter()
            .find(|d| d.function.name == "memory_feedback")
            .expect("memory_feedback definition must exist");
        let props = &feedback_def.function.parameters["properties"];
        assert!(
            props.get("memory_id").is_some(),
            "memory_feedback must use 'memory_id'"
        );
    }

    #[test]
    fn test_definitions_all_tools_present() {
        let defs = definitions();
        let names: Vec<&str> = defs.iter().map(|d| d.function.name.as_str()).collect();
        assert!(names.contains(&"memory_store"), "missing memory_store");
        assert!(names.contains(&"memory_search"), "missing memory_search");
        assert!(names.contains(&"memory_delete"), "missing memory_delete");
        assert!(names.contains(&"memory_update"), "missing memory_update");
        assert!(
            names.contains(&"memory_feedback"),
            "missing memory_feedback"
        );
        assert!(names.contains(&"memory_stats"), "missing memory_stats");
        assert!(names.contains(&"memory_list"), "missing memory_list");
        assert!(
            names.contains(&"memory_knowledge"),
            "missing memory_knowledge"
        );
        assert!(names.contains(&"memory_relate"), "missing memory_relate");
    }

    // ── Output Format Regression Tests ───────────────────────────────────

    #[test]
    fn test_search_output_format_uses_new_field_names() {
        use crate::atoms::engram_types::{
            CompressionLevel, MemoryType, RetrievedMemory, TrustScore,
        };

        // Simulate the output format string from execute_memory_search
        let mem = RetrievedMemory {
            content: "User prefers dark mode".to_string(),
            compression_level: CompressionLevel::Full,
            memory_id: "abc12345-6789-0000-0000-000000000000".to_string(),
            memory_type: MemoryType::Episodic,
            trust_score: TrustScore::default(),
            token_cost: 10,
            category: "preference".to_string(),
            created_at: "2025-01-01T00:00:00Z".to_string(),
            agent_id: String::new(),
        };

        // This is the exact format string from execute_memory_search
        let output = format!(
            "{}. [{}] ({}) {} (id: {}, score: {:.2})\n",
            1,
            mem.category,
            mem.memory_type,
            mem.content,
            safe_truncate(&mem.memory_id, 8),
            mem.trust_score.composite(),
        );

        // Verify the output contains the truncated memory_id (not legacy 'id')
        assert!(
            output.contains("id: abc12345"),
            "Output must show truncated memory_id: got '{}'",
            output
        );
        // Verify score is from trust_score.composite()
        assert!(
            output.contains("score: "),
            "Output must show composite trust score: got '{}'",
            output
        );
        // Verify memory_type is shown (Display impl uses lowercase)
        assert!(output.contains("episodic"), "Output must show memory type");
        // Verify category is shown
        assert!(output.contains("preference"), "Output must show category");
    }

    #[test]
    fn test_trust_score_composite_in_output_range() {
        use crate::atoms::engram_types::TrustScore;

        let ts = TrustScore::default();
        let score = ts.composite();
        assert!(
            (0.0..=1.0).contains(&score),
            "Default TrustScore.composite() must be in [0, 1]: got {}",
            score
        );
    }

    #[test]
    fn test_memory_id_truncation_short_id() {
        // Edge case: memory_id shorter than 8 chars
        let short_id = "abc";
        let truncated = safe_truncate(&short_id, 8);
        assert_eq!(truncated, "abc", "Short IDs should not panic on truncation");
    }

    #[test]
    fn test_definitions_no_legacy_id_field() {
        // Regression: ensure NO tool definition uses plain "id" as a param name
        let defs = definitions();
        for def in &defs {
            let props = &def.function.parameters["properties"];
            if let Some(obj) = props.as_object() {
                assert!(
                    !obj.contains_key("id"),
                    "Tool '{}' must not use legacy 'id' field — use 'memory_id' instead",
                    def.function.name
                );
            }
        }
    }
}
